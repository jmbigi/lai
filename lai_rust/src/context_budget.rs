//! context_budget.rs - Presupuesto de contexto para modelos locales pequeños.
//!
//! Los modelos locales suelen correr con ventanas de 8k-32k tokens. Un bucle
//! de agente que nunca recorta el historial acaba expulsando la tarea original
//! (el primer mensaje) y el modelo empieza a responder preguntas en vez de
//! editar código. Medido en el piloto del 2026-11-26: el `read_file` de un
//! archivo grande consumía la ventana entera.
//!
//! La estrategia sigue lo que hacen los agentes ligeros actuales
//! (mini-SWE-agent, harness de código con elisión): se conservan el mensaje de
//! sistema y la tarea, se mantienen intactos los turnos recientes y se
//! **eliden** los resultados de herramientas antiguos, que son el 90 % del
//! volumen. No se borran mensajes: cada `tool` debe seguir emparejado con su
//! `tool_call_id` o el protocolo de function calling se rompe.

use crate::llm::Message;

/// Estimación conservadora de tokens: ~3 caracteres por token en código.
///
/// No sustituye al tokenizador del modelo (no está disponible sin cargarlo),
/// pero sobreestima ligeramente, que es el lado seguro para un presupuesto.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count() / 3 + 1
}

/// Caracteres disponibles para el historial a partir de la ventana del modelo.
///
/// Se reserva el 40 % para el prompt de sistema, las herramientas y la
/// respuesta. El resultado se mide en caracteres porque es lo que podemos
/// contar sin tokenizador.
pub fn budget_chars_from_window(window_tokens: usize) -> usize {
    (window_tokens * 3 * 60) / 100
}

/// Ventana de contexto declarada por el operador (`LAI_CONTEXT_WINDOW`).
///
/// Por defecto 32768, el valor con el que se midió el piloto. No se consulta
/// al servidor: un valor supuesto nunca debe presentarse como medido.
pub fn context_window_from_env() -> usize {
    std::env::var("LAI_CONTEXT_WINDOW")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value >= 1024)
        .unwrap_or(32_768)
}

/// Resultado de una condensación, para poder registrarlo en la telemetría.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CondenseStats {
    pub chars_before: usize,
    pub chars_after: usize,
    pub elided: usize,
}

impl CondenseStats {
    pub fn changed(&self) -> bool {
        self.chars_after < self.chars_before
    }
}

fn total_chars(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum()
}

/// Sustituye un resultado de herramienta por un marcador de una línea.
fn elide(content: &str) -> String {
    format!(
        "[salida de herramienta elidida para ahorrar contexto: {} caracteres. \
         Repite la lectura con read_file (start_line/end_line) o grep si la necesitas.]",
        content.chars().count()
    )
}

/// Recorta un texto largo conservando cabeza y cola.
fn truncate_keep_ends(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let keep = max_chars / 2;
    let head: String = text.chars().take(keep).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let omitted = text.chars().count() - head.chars().count() - tail.chars().count();
    format!("{head}\n\n[... {omitted} caracteres omitidos ...]\n\n{tail}")
}

/// Condensa el historial hasta `max_chars` sin romper el protocolo de tools.
///
/// - Conserva siempre el mensaje de sistema (índice 0) y la tarea original
///   (primer mensaje de usuario).
/// - Conserva intactos los últimos `keep_recent` mensajes.
/// - Elide los resultados de herramientas antiguos.
/// - Si aún no cabe, recorta el contenido de los mensajes de asistente
///   antiguos y, como último recurso, la cola del mensaje de sistema.
///
/// Devuelve estadísticas; si no había nada que hacer, `changed()` es `false`.
pub fn condense(messages: &mut [Message], max_chars: usize, keep_recent: usize) -> CondenseStats {
    let chars_before = total_chars(messages);
    let mut stats = CondenseStats {
        chars_before,
        chars_after: chars_before,
        elided: 0,
    };

    if chars_before <= max_chars || messages.len() <= keep_recent + 2 {
        return stats;
    }

    let keep_recent = keep_recent.min(messages.len().saturating_sub(2));
    let protected_until = messages.len().saturating_sub(keep_recent);
    // El primer mensaje de usuario es la tarea: no se toca.
    let first_user = messages
        .iter()
        .position(|message| message.role == "user")
        .unwrap_or(usize::MAX);

    for (index, message) in messages
        .iter_mut()
        .enumerate()
        .take(protected_until)
        .skip(1)
    {
        if index == first_user {
            continue;
        }
        if message.role == "tool" && message.content.chars().count() > 200 {
            message.content = elide(&message.content);
            stats.elided += 1;
        }
    }

    // Segundo paso: si aun así no cabe (ventanas de 1k-4k tokens con turnos
    // recientes grandes), se eliden también las salidas de herramientas de los
    // turnos recientes, salvo la última: el modelo necesita ver el resultado de
    // lo que acaba de ejecutar.
    if total_chars(messages) > max_chars {
        let last = messages.len().saturating_sub(1);
        for message in messages.iter_mut().take(last).skip(protected_until) {
            if message.role == "tool" && message.content.chars().count() > 200 {
                message.content = elide(&message.content);
                stats.elided += 1;
            }
        }
    }

    // Tercer paso: recortar contenido largo de turnos antiguos de asistente.
    let current = total_chars(messages);
    if current > max_chars {
        let overflow = current - max_chars;
        for index in 1..protected_until {
            if index == first_user {
                continue;
            }
            let message = &mut messages[index];
            if message.role != "assistant" {
                continue;
            }
            let len = message.content.chars().count();
            if len > 400 {
                let target = len.saturating_sub(overflow.min(len / 2)).max(400);
                message.content = truncate_keep_ends(&message.content, target);
            }
            if total_chars(messages) <= max_chars {
                break;
            }
        }
    }

    // Último recurso: recortar la cola del contexto de sistema. Se conserva la
    // cabeza, que es donde el prompt pone las reglas y la tarea.
    let current = total_chars(messages);
    if current > max_chars && !messages.is_empty() {
        let overflow = current - max_chars;
        let len = messages[0].content.chars().count();
        let target = len.saturating_sub(overflow).max(1_000);
        messages[0].content = truncate_keep_ends(&messages[0].content, target);
    }

    stats.chars_after = total_chars(messages);
    stats
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    #[test]
    fn short_conversations_are_untouched() {
        let mut messages = vec![message("system", "reglas"), message("user", "tarea")];
        let before = messages.clone();
        let stats = condense(&mut messages, 10_000, 4);
        assert!(!stats.changed());
        assert_eq!(messages.len(), before.len());
        assert_eq!(messages[0].content, "reglas");
        assert_eq!(messages[1].content, "tarea");
    }

    #[test]
    fn old_tool_outputs_are_elided_and_recent_ones_kept() {
        let big = "x".repeat(5_000);
        let mut messages = vec![
            message("system", "reglas"),
            message("user", "tarea"),
            message("assistant", "leo el archivo"),
            message("tool", &big),
            message("assistant", "edito"),
            message("tool", &big),
            message("assistant", "reciente"),
        ];

        let stats = condense(&mut messages, 8_000, 2);

        assert!(stats.changed());
        assert_eq!(stats.elided, 1, "solo el tool antiguo se elide");
        assert!(
            messages[3].content.contains("elidida"),
            "{}",
            messages[3].content
        );
        assert_eq!(messages[5].content, big, "el tool reciente se conserva");
        assert_eq!(messages[0].content, "reglas");
        assert_eq!(messages[1].content, "tarea");
    }

    #[test]
    fn recent_tool_outputs_are_elided_when_the_budget_is_very_small() {
        let big = "y".repeat(3_000);
        let mut messages = vec![
            message("system", "reglas"),
            message("user", "tarea"),
            message("assistant", "uno"),
            message("tool", &big),
            message("assistant", "dos"),
            message("tool", &big),
            message("assistant", "tres"),
            message("tool", &big),
        ];

        // keep_recent=4 protege los últimos 4 mensajes; aun así el presupuesto
        // obliga a elidir el tool intermedio y conservar solo el último.
        let stats = condense(&mut messages, 2_000, 4);

        assert!(stats.changed());
        assert!(
            messages[7].content.starts_with("yyy"),
            "el último resultado se conserva"
        );
        assert!(messages[3].content.contains("elidida"));
        assert!(messages[5].content.contains("elidida"));
    }

    #[test]
    fn system_and_task_survive_even_when_budget_is_tiny() {
        let mut messages = vec![
            message("system", &"s".repeat(8_000)),
            message("user", "arregla el bug de base.py"),
            message("assistant", &"a".repeat(8_000)),
            message("tool", &"t".repeat(8_000)),
            message("assistant", "final"),
        ];

        condense(&mut messages, 6_000, 1);

        assert!(messages[0].content.starts_with("ssss"));
        assert_eq!(messages[1].content, "arregla el bug de base.py");
        assert!(messages[4].content == "final");
    }

    #[test]
    fn estimate_and_budget_are_conservative() {
        assert_eq!(estimate_tokens("abc"), 2);
        assert!(budget_chars_from_window(8_192) < budget_chars_from_window(32_768));
        assert_eq!(budget_chars_from_window(10_000), 18_000);
    }

    #[test]
    fn truncate_keep_ends_marks_the_omission() {
        let text = "a".repeat(1_000);
        let out = truncate_keep_ends(&text, 100);
        assert!(out.chars().count() < 1_000);
        assert!(out.contains("omitidos"));
    }
}
