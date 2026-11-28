use crate::policy::{PolicyConfig, PolicyEngine};
use crate::repomap::{extract_symbols, is_skipped_dir};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{Duration, Instant};
use walkdir::WalkDir;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}
type BoxedToolFuture = Pin<Box<dyn Future<Output = Result<String>> + Send>>;
type ToolFunc = Arc<dyn Fn(HashMap<String, Value>) -> BoxedToolFuture + Send + Sync>;

/// Directories that are never useful for code search and can be huge.
/// (Implementación compartida con `repomap`.)
fn arg_str<'a>(args: &'a HashMap<String, Value>, key: &str, default: &'a str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

fn arg_usize(args: &HashMap<String, Value>, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(default)
}

fn arg_bool(args: &HashMap<String, Value>, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Detecta comandos de shell que reemplazan o borran un manifiesto del
/// proyecto (`> Cargo.toml`, `cat > …`, `tee …`, `rm …`, `mv … Cargo.toml`,
/// `dd of=…`, `truncate …`).
///
/// Es una guarda de mejor esfuerzo: no interpreta el shell. La ofuscación
/// (`cat $VAR > $MANIFEST`, `python -c "..."`, `bash -c "..."`) puede saltarla;
/// se declara como límite en `AGENTS.md` y `SECURITY.md`.
fn shell_manifest_violation(command: &str) -> Option<String> {
    let words = shell_words(command);
    let mut i = 0;
    while i < words.len() {
        let word = &words[i];
        // Redirecciones: > file, >> file, 2> file, >file
        let target = if word == ">" || word == ">>" {
            words.get(i + 1).cloned()
        } else {
            word.strip_prefix(">>")
                .or_else(|| word.strip_prefix('>'))
                .map(|rest| rest.to_string())
        };
        if let Some(target) = target {
            let target = target.trim_matches(|c| c == '"' || c == '\'');
            if !target.starts_with('&') && is_manifest_file(Path::new(target)) {
                return Some(format!("redirección a '{target}'"));
            }
        }

        let destructive = matches!(word.as_str(), "tee" | "rm" | "truncate");
        if destructive {
            let mut j = i + 1;
            while j < words.len() {
                let candidate = words[j].trim_matches(|c| c == '"' || c == '\'');
                if !candidate.starts_with('-') && is_manifest_file(Path::new(candidate)) {
                    return Some(format!("'{word}' sobre '{candidate}'"));
                }
                j += 1;
            }
        }

        if matches!(word.as_str(), "mv" | "cp" | "install") {
            // El destino es el último operando; si es un manifiesto, se
            // sobrescribe (o se crea, que no destruye nada si no existía).
            let operands: Vec<&String> = words[i + 1..]
                .iter()
                .filter(|w| !w.starts_with('-'))
                .collect();
            if let Some(dest) = operands.last() {
                let dest = dest.trim_matches(|c| c == '"' || c == '\'');
                if is_manifest_file(Path::new(dest)) && Path::new(dest).exists() {
                    return Some(format!("'{word}' sobrescribe '{dest}'"));
                }
            }
        }

        if word == "dd" {
            for arg in &words[i + 1..] {
                if let Some(target) = arg.strip_prefix("of=") {
                    if is_manifest_file(Path::new(target)) {
                        return Some(format!("dd of={target}"));
                    }
                }
            }
        }

        i += 1;
    }
    None
}

/// Divide un comando de shell en palabras respetando comillas simples/dobles.
fn shell_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else if ch == '\\' && q == '"' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(ch);
                }
            }
            None => {
                if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                } else if ch.is_whitespace() {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                } else {
                    current.push(ch);
                }
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Busca una ventana de líneas en `content` que coincida con `old` ignorando
/// los espacios iniciales y finales de cada línea (técnica de Warp).
///
/// Devuelve el texto **real** del archivo (con su indentación) solo si la
/// coincidencia es única. Si hay cero o varias, devuelve `None`: aplicar una
/// coincidencia ambigua sería peor que fallar.
fn indentation_agnostic_span(content: &str, old: &str) -> Option<String> {
    let old_lines: Vec<&str> = old.lines().collect();
    if old_lines.is_empty() {
        return None;
    }
    let content_lines: Vec<&str> = content.split_inclusive('\n').collect();
    if content_lines.len() < old_lines.len() {
        return None;
    }
    let target: Vec<&str> = old_lines.iter().map(|line| line.trim()).collect();

    let mut matches: Vec<String> = Vec::new();
    for start in 0..=(content_lines.len() - old_lines.len()) {
        let window = &content_lines[start..start + old_lines.len()];
        let ok = window
            .iter()
            .zip(target.iter())
            .all(|(line, expected)| line.trim() == *expected);
        if !ok {
            continue;
        }
        let mut span = String::new();
        for (index, line) in window.iter().enumerate() {
            let is_last = index + 1 == window.len();
            if is_last && !old.ends_with('\n') {
                // No arrastrar el salto de línea de la última línea: `old` no lo
                // tenía y sustituirlo cambiaría el resto del archivo.
                span.push_str(line.trim_end_matches('\n').trim_end_matches('\r'));
            } else {
                span.push_str(line);
            }
        }
        matches.push(span);
        if matches.len() > 1 {
            return None; // ambiguo
        }
    }
    matches.pop()
}

/// Reaplica la indentación de `template` a las líneas de `new` cuando ambas
/// tienen el mismo número de líneas. Conservador: no toca líneas vacías ni
/// líneas que ya empiezan con la indentación esperada.
fn reindent_like(template: &str, new: &str) -> String {
    let template_lines: Vec<&str> = template.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    if template_lines.len() != new_lines.len() {
        return new.to_string();
    }
    let mut out = Vec::with_capacity(new_lines.len());
    for (old_line, new_line) in template_lines.iter().zip(new_lines.iter()) {
        let indent: String = old_line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if new_line.trim().is_empty() || indent.is_empty() || new_line.starts_with(&indent) {
            out.push((*new_line).to_string());
        } else {
            out.push(format!("{indent}{new_line}"));
        }
    }
    let mut result = out.join("\n");
    if new.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Cita un argumento para `bash -c` (usado al envolver `run_python` en bwrap).
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Mata el proceso y, en Unix, todo su grupo (hijos incluidos).
fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        // El hijo se lanzó con process_group(0), así que su PID es el PGID.
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

/// Ejecuta un proceso con un límite de tiempo real.
///
/// `Command::output()` no tiene timeout: si un test o un servidor se cuelga,
/// el bucle del agente espera para siempre. El esquema de las herramientas ya
/// anunciaba `timeout`, pero se ignoraba. Los pipes se leen en hilos separados
/// para que un proceso que escribe mucho no bloquee el `wait`.
fn run_with_timeout(mut command: Command, timeout: Duration) -> Result<std::process::Output> {
    use std::io::Read;

    // En Unix, `sh -c "sleep 5"` crea un hijo; matar solo al shell deja vivo
    // al nieto y los pipes abiertos. Un grupo de procesos propio permite
    // matar el árbol completo al agotar el timeout.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;

    let stdout_reader = child.stdout.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });
    let stderr_reader = child.stderr.take().map(|mut pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                kill_process_tree(&mut child);
                if let Some(reader) = stdout_reader {
                    let _ = reader.join();
                }
                if let Some(reader) = stderr_reader {
                    let _ = reader.join();
                }
                return Err(e.into());
            }
        }

        if start.elapsed() >= timeout {
            kill_process_tree(&mut child);
            if let Some(reader) = stdout_reader {
                let _ = reader.join();
            }
            if let Some(reader) = stderr_reader {
                let _ = reader.join();
            }
            anyhow::bail!(
                "command timed out after {}s and was killed",
                timeout.as_secs().max(1)
            );
        }

        std::thread::sleep(Duration::from_millis(20));
    };

    let stdout = stdout_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let stderr = stderr_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// Read a whole file or a 1-based line range.
///
/// Large files are the main way a local model blows its context window, so the
/// default is capped and the header tells the model exactly which lines it saw.
fn read_file_tool(args: &HashMap<String, Value>) -> Result<String> {
    const DEFAULT_LINES: usize = 400;
    const MAX_LINES: usize = 2_000;

    let path = arg_str(args, "path", "");
    if path.is_empty() {
        return Err(anyhow::anyhow!("read_file requires a non-empty 'path'"));
    }
    if !arg_bool(args, "allow_sensitive_read", false) {
        if let Some(reason) = sensitive_read_violation(path) {
            return Err(anyhow::anyhow!(
                "read_file rechazado: '{path}' parece una credencial ({reason}). \
                 El agente no debe leer claves ni tokens. Si de verdad lo necesitas, \
                 repite con \"allow_sensitive_read\": true."
            ));
        }
    }
    let bytes = fs::read(path).map_err(|e| anyhow::anyhow!("read_file {path}: {e}"))?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return Err(anyhow::anyhow!(
            "read_file {path}: binary file (contains NUL bytes)"
        ));
    }
    let content = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    if total == 0 {
        return Ok(format!("{path}: empty file"));
    }

    let start = arg_usize(args, "start_line", 1).max(1);
    let end_requested = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .filter(|v| *v > 0);
    let max_requested = args
        .get("max_lines")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .filter(|v| *v > 0);

    let mut end = end_requested.unwrap_or(total).min(total);
    let limit = match max_requested {
        Some(n) => n,
        None if end_requested.is_some() => end.saturating_sub(start) + 1,
        None => DEFAULT_LINES,
    }
    .min(MAX_LINES);
    if end >= start {
        end = end.min(start.saturating_add(limit.saturating_sub(1)));
    }

    if start > total {
        return Ok(format!(
            "{path}: start_line {start} is past end of file ({total} lines)"
        ));
    }
    let end = end.max(start).min(total);
    let mut out = format!("{path}: lines {start}-{end} of {total}\n");
    out.push_str(&lines[start - 1..end].join("\n"));
    if end < total {
        out.push_str(&format!(
            "\n... ({} more lines; use start_line/end_line to read them)",
            total - end
        ));
    }
    Ok(out)
}

/// Search files with a literal or regex matcher, skipping build/VCS noise.
fn grep_tool(args: &HashMap<String, Value>) -> Result<String> {
    const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

    let pattern = arg_str(args, "pattern", "");
    if pattern.is_empty() {
        return Err(anyhow::anyhow!("grep requires a non-empty 'pattern'"));
    }
    let path = arg_str(args, "path", ".");
    if let Some(reason) = sensitive_read_violation(path) {
        return Err(anyhow::anyhow!(
            "grep rechazado: '{path}' parece una ruta de credenciales ({reason})."
        ));
    }
    // Un `path` inexistente devolvía "No matches", y el modelo interpretaba que
    // el patrón no estaba y probaba rutas inventadas (medido el 2026-11-27 en
    // pytest-5809: 10 iteraciones con /testbed/src/_pytest/terminal/pytest/...).
    if !Path::new(path).exists() {
        return Err(anyhow::anyhow!(
            "grep: la ruta '{path}' no existe. Usa una ruta real del proyecto              (por ejemplo '.' o 'src') o find_files para localizar el archivo."
        ));
    }
    let use_regex = arg_bool(args, "regex", false);
    let case_insensitive = arg_bool(args, "case_insensitive", false);
    let glob_filter = {
        let raw = arg_str(args, "glob", "");
        if raw.is_empty() {
            None
        } else {
            Some(
                glob::Pattern::new(raw)
                    .map_err(|e| anyhow::anyhow!("invalid glob {raw:?}: {e}"))?,
            )
        }
    };
    let max_results = arg_usize(args, "max_results", 200).clamp(1, 1_000);

    let regex = if use_regex {
        Some(
            regex::RegexBuilder::new(pattern)
                .case_insensitive(case_insensitive)
                .build()
                .map_err(|e| anyhow::anyhow!("invalid regex {pattern:?}: {e}"))?,
        )
    } else {
        None
    };
    let needle = pattern.to_lowercase();

    let mut matches = Vec::new();
    let walker = WalkDir::new(path)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            !e.file_type().is_dir() || !is_skipped_dir(&e.file_name().to_string_lossy())
        });
    for entry in walker.filter_map(|e| e.ok()) {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if let Some(filter) = &glob_filter {
            let rel = p.strip_prefix(path).unwrap_or(p);
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            if !filter.matches_path(rel) && !filter.matches(&name) {
                continue;
            }
        }
        if let Ok(meta) = entry.metadata() {
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
        }
        let bytes = match fs::read(p) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue;
        }
        let content = String::from_utf8_lossy(&bytes);
        for (i, line) in content.lines().enumerate() {
            let hit = match &regex {
                Some(re) => re.is_match(line),
                None if case_insensitive => line.to_lowercase().contains(&needle),
                None => line.contains(pattern),
            };
            if !hit {
                continue;
            }
            let mut shown: String = line.chars().take(300).collect();
            if line.chars().count() > 300 {
                shown.push('…');
            }
            matches.push(format!("{}:{}: {}", p.display(), i + 1, shown));
            if matches.len() >= max_results {
                break;
            }
        }
        if matches.len() >= max_results {
            break;
        }
    }

    if matches.is_empty() {
        Ok(format!("No matches for {pattern:?} in {path}"))
    } else {
        Ok(matches.join("\n"))
    }
}

/// Find files by glob without panicking on an invalid pattern.
fn find_files_tool(args: &HashMap<String, Value>) -> Result<String> {
    let pattern = arg_str(args, "pattern", "*");
    let path = arg_str(args, "path", ".");
    if !Path::new(path).exists() {
        return Err(anyhow::anyhow!(
            "find_files: la ruta '{path}' no existe. Usa una ruta real del proyecto              (por ejemplo '.') o list_dir para ver qué hay."
        ));
    }
    let max_results = arg_usize(args, "max_results", 200).clamp(1, 1_000);
    let glob_pattern = glob::Pattern::new(pattern)
        .map_err(|e| anyhow::anyhow!("invalid glob {pattern:?}: {e}"))?;

    let mut matches = Vec::new();
    let walker = WalkDir::new(path)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| {
            !e.file_type().is_dir() || !is_skipped_dir(&e.file_name().to_string_lossy())
        });
    for entry in walker.filter_map(|e| e.ok()) {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let rel = p.strip_prefix(path).unwrap_or(p);
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        if glob_pattern.matches_path(rel) || glob_pattern.matches(&name) {
            matches.push(p.display().to_string());
            if matches.len() >= max_results {
                break;
            }
        }
    }
    Ok(serde_json::to_string(&matches)?)
}

/// Umbral de la guarda anti-destrucción de `write_file`.
///
/// Medido en el piloto SWE-bench del 2026-11-26: con `qwen2.5-coder:7b`, el
/// agente reemplazó `django/db/models/base.py` (1911 líneas) por las 18 líneas
/// del ejemplo del issue, rompiendo la importación de todo el paquete. Sin esta
/// guarda el error no se detecta hasta que fallan los tests.
const SHRINK_MIN_ORIGINAL_LINES: usize = 40;
const SHRINK_RATIO_PERCENT: usize = 25;

/// Manifiestos y archivos de bloqueo del proyecto.
///
/// Reemplazarlos enteros casi siempre rompe la build: en el benchmark del
/// 2026-11-26 el agente sustituyó `Cargo.toml` por uno inventado. Los cambios
/// legítimos (añadir una dependencia) se hacen con `edit`, que conserva el
/// resto del archivo.
fn is_manifest_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    matches!(
        name,
        "Cargo.toml"
            | "Cargo.lock"
            | "package.json"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "pyproject.toml"
            | "setup.py"
            | "setup.cfg"
            | "requirements.txt"
            | "go.mod"
            | "go.sum"
            | "Gemfile"
            | "Gemfile.lock"
            | "composer.json"
            | "composer.lock"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
    )
}

/// Devuelve la razón por la que un parche sobre un manifiesto debe rechazarse,
/// o `None` si es un cambio acotado (o no toca manifiestos).
///
/// Se considera destructivo cuando el parche elimina al menos el 75 % de las
/// líneas actuales del manifiesto (reemplazo encubierto) o cuando lo borra
/// (`+++ /dev/null`). Añadir una dependencia o cambiar una línea queda
/// permitido.
fn manifest_patch_violation(patch: &str, cwd: &Path) -> Option<String> {
    let mut current: Option<String> = None;
    let mut in_hunk = false;
    let mut removed = 0usize;
    let mut added = 0usize;
    let mut deletes_manifest = false;

    fn evaluate(path: &Option<String>, cwd: &Path, removed: usize, added: usize) -> Option<String> {
        let path = path.as_deref()?;
        if !is_manifest_file(Path::new(path)) || removed == 0 {
            return None;
        }
        let existing = fs::read_to_string(cwd.join(path))
            .map(|content| content.lines().count())
            .unwrap_or(0);
        if existing > 0 && removed * 100 >= existing * 75 {
            return Some(format!(
                "el parche elimina {removed} de {existing} líneas del manifiesto \
                 '{path}' ({added} añadidas): es un reemplazo encubierto"
            ));
        }
        None
    }

    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            if in_hunk {
                if let Some(reason) = evaluate(&current, cwd, removed, added) {
                    return Some(reason);
                }
            }
            in_hunk = false;
            current = None;
            continue;
        }
        if !in_hunk && line.starts_with("--- ") {
            let path = line[4..].trim();
            current = if path == "/dev/null" {
                None
            } else {
                Some(path.trim_start_matches("a/").to_string())
            };
            continue;
        }
        if !in_hunk && line.starts_with("+++ ") {
            let path = line[4..].trim();
            if path == "/dev/null" {
                if let Some(old) = &current {
                    if is_manifest_file(Path::new(old)) {
                        deletes_manifest = true;
                    }
                }
            }
            continue;
        }
        if line.starts_with("@@ ") {
            if in_hunk {
                if let Some(reason) = evaluate(&current, cwd, removed, added) {
                    return Some(reason);
                }
            }
            in_hunk = true;
            removed = 0;
            added = 0;
            continue;
        }
        if in_hunk {
            if line.starts_with('-') {
                removed += 1;
            } else if line.starts_with('+') {
                added += 1;
            } else if !line.starts_with(' ') && !line.starts_with('\\') {
                // Fin del hunk (cabecera de otro archivo, texto libre…).
                if let Some(reason) = evaluate(&current, cwd, removed, added) {
                    return Some(reason);
                }
                in_hunk = false;
            }
        }
    }

    if in_hunk {
        if let Some(reason) = evaluate(&current, cwd, removed, added) {
            return Some(reason);
        }
    }
    if deletes_manifest {
        Some("el parche elimina un manifiesto del proyecto".to_string())
    } else {
        None
    }
}

/// Detecta lecturas de credenciales del usuario.
///
/// El sandbox protege la ejecución de comandos, pero `read_file` es una
/// herramienta del proceso lai y lee con los permisos del usuario. Un README
/// con prompt injection podría pedir `~/.ssh/id_ed25519` y el modelo verter el
/// contenido al contexto. Esta guarda bloquea las rutas de credenciales más
/// comunes; es una lista de nombres, no un análisis del contenido.
fn sensitive_read_violation(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let components: Vec<&str> = lower.split('/').filter(|c| !c.is_empty()).collect();

    // Componentes de directorio que nunca se leen desde el agente.
    const SENSITIVE_DIRS: &[&str] = &[".ssh", ".aws", ".gnupg", ".azure", ".kube"];
    for dir in SENSITIVE_DIRS {
        if components.contains(dir) {
            return Some(format!("directorio {dir}"));
        }
    }
    if components.contains(&".docker") && components.last() == Some(&"config.json") {
        return Some("credenciales de Docker".to_string());
    }

    // Archivos de credenciales por nombre.
    const SENSITIVE_FILES: &[&str] = &[
        ".netrc",
        ".npmrc",
        ".pypirc",
        ".git-credentials",
        ".env.local",
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "id_dsa",
    ];
    if let Some(name) = components.last() {
        if SENSITIVE_FILES.contains(name) {
            return Some(format!("archivo {name}"));
        }
    }

    // Claves privadas por extensión, pero no certificados públicos.
    if let Some(name) = components.last() {
        if (name.ends_with(".pem") || name.ends_with(".key")) && !name.ends_with(".pub") {
            return Some(format!("posible clave privada {name}"));
        }
    }
    None
}

fn write_file_tool(args: &HashMap<String, Value>) -> Result<String> {
    let path = arg_str(args, "path", "");
    if path.is_empty() {
        return Err(anyhow::anyhow!("write_file requiere 'path'"));
    }
    // No usar "" por defecto: un modelo que olvida `content` borraría el
    // archivo (o crearía uno vacío) y el resultado parecería correcto.
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("write_file requiere 'content'"))?;
    let target = Path::new(path);
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    // Guarda anti-destrucción: sobrescribir un archivo existente con una
    // fracción mínima de su contenido casi siempre significa que el modelo
    // "escribió un ejemplo" en lugar de editar el archivo real.
    if target.is_file() {
        if let Ok(existing) = fs::read_to_string(target) {
            // Un manifiesto se edita, no se reemplaza: el agente inventó un
            // Cargo.toml y rompió el proyecto (benchmark del 2026-11-26).
            if is_manifest_file(target)
                && existing != content
                && !arg_bool(args, "allow_manifest_overwrite", false)
            {
                return Err(anyhow::anyhow!(
                    "write_file rechazado: '{path}' es un manifiesto del proyecto. \
                     Reemplazarlo entero suele romper la build. Usa `edit` para cambiar \
                     solo la línea necesaria (por ejemplo, añadir una dependencia), o \
                     repite con \"allow_manifest_overwrite\": true si de verdad quieres \
                     sustituirlo."
                ));
            }
            let old_lines = existing.lines().count();
            let new_lines = content.lines().count();
            let shrunk = old_lines >= SHRINK_MIN_ORIGINAL_LINES
                && new_lines * 100 < old_lines * SHRINK_RATIO_PERCENT;
            if shrunk && !arg_bool(args, "allow_shrink", false) {
                return Err(anyhow::anyhow!(
                    "write_file rechazado: '{}' tiene {} líneas y el contenido nuevo solo {} \
                     ({:.0}% del original). Esto normalmente destruye el archivo. \
                     Si de verdad quieres reemplazarlo, repite la llamada con \
                     \"allow_shrink\": true, o usa apply_patch para cambiar solo lo necesario.",
                    path,
                    old_lines,
                    new_lines,
                    (new_lines as f64 / old_lines as f64) * 100.0
                ));
            }
        }
    }

    fs::write(target, content)?;
    Ok(format!("Written to {} ({} chars)", path, content.len()))
}

/// Edita un archivo reemplazando un fragmento exacto por otro.
///
/// Es la vía más fiable para modelos pequeños: no tienen que reproducir el
/// archivo completo (`write_file`) ni generar un diff con números de línea
/// (`apply_patch`), solo copiar el texto que acaban de leer. Si el fragmento no
/// existe o es ambiguo, falla en voz alta y no escribe nada.
fn edit_file_tool(args: &HashMap<String, Value>) -> Result<String> {
    let path = arg_str(args, "path", "");
    if path.is_empty() {
        return Err(anyhow::anyhow!("edit requiere 'path'"));
    }
    let old = args
        .get("old_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("edit requiere 'old_string'"))?;
    let new = args
        .get("new_string")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("edit requiere 'new_string'"))?;
    if old.is_empty() {
        return Err(anyhow::anyhow!(
            "edit: 'old_string' no puede estar vacío; usa write_file para crear un archivo"
        ));
    }
    if old == new {
        return Err(anyhow::anyhow!(concat!(
            "edit: 'old_string' y 'new_string' son idénticos, así que el archivo no cambiaría. ",
            "Reescribe SOLO las líneas del arreglo: cambia el valor, la condición o la ",
            "llamada concreta que causa el bug y deja el resto del fragmento igual. ",
            "Si aún no sabes qué línea cambiar, lee el rango exacto con read_file ",
            "(start_line/end_line) antes de editar."
        )));
    }

    let content = fs::read_to_string(path).map_err(|e| anyhow::anyhow!("edit {path}: {e}"))?;
    let replace_all = arg_bool(args, "replace_all", false);
    // Fallback tolerante a indentación (técnica de Warp): si el fragmento no
    // aparece literalmente, se busca una ventana de líneas que coincida
    // ignorando espacios iniciales y finales. Solo se acepta si es única; así
    // un modelo pequeño que copia mal la indentación no queda bloqueado.
    let mut effective_old: String = old.to_string();
    let occurrences = if content.matches(old).count() == 0 {
        match indentation_agnostic_span(&content, old) {
            Some(span) => {
                effective_old = span;
                content.matches(effective_old.as_str()).count()
            }
            None => 0,
        }
    } else {
        content.matches(old).count()
    };
    if occurrences == 0 {
        return Err(anyhow::anyhow!(
            "edit: el texto no aparece en '{path}', ni siquiera ignorando la \
             indentación. Cópialo exactamente de read_file o usa apply_patch."
        ));
    }
    if occurrences > 1 && !replace_all {
        return Err(anyhow::anyhow!(
            "edit: el texto aparece {occurrences} veces en '{path}'. Añade más contexto \
             para que sea único o pasa \"replace_all\": true."
        ));
    }

    // Un `edit` que abarca el manifiesto completo es un reemplazo encubierto:
    // se aplica la misma guarda que `write_file`.
    if is_manifest_file(Path::new(path))
        && effective_old.trim() == content.trim()
        && !arg_bool(args, "allow_manifest_overwrite", false)
    {
        return Err(anyhow::anyhow!(
            "edit rechazado: 'old_string' abarca todo el manifiesto '{path}'. \
             Cambia solo el fragmento necesario (por ejemplo, la línea de una \
             dependencia) o repite con \"allow_manifest_overwrite\": true si de \
             verdad quieres sustituirlo entero."
        ));
    }

    let replacement = if effective_old != old {
        // El fallback encontró el fragmento con otra indentación: aplicar la
        // indentación real del archivo a las líneas nuevas para no romper el
        // código (Python depende de ella).
        reindent_like(&effective_old, new)
    } else {
        new.to_string()
    };
    let updated = if replace_all {
        content.replace(effective_old.as_str(), &replacement)
    } else {
        content.replacen(effective_old.as_str(), &replacement, 1)
    };

    // Misma guarda anti-destrucción que `write_file`: con `old_string` igual a
    // casi todo el archivo, `edit` puede borrarlo igual que un write_file.
    // Medido el 2026-11-27: qwen3-8b redujo src/_pytest/pastebin.py de 71 a 5
    // líneas con un único `edit`, y el parche resultante no podía pasar nada.
    let old_lines = content.lines().count();
    let new_lines = updated.lines().count();
    let shrunk = old_lines >= SHRINK_MIN_ORIGINAL_LINES
        && new_lines * 100 < old_lines * SHRINK_RATIO_PERCENT;
    if shrunk && !arg_bool(args, "allow_shrink", false) {
        return Err(anyhow::anyhow!(
            "edit rechazado: el reemplazo dejaría '{path}' con {new_lines} líneas de {old_lines}              ({:.0}% del original). Eso casi siempre destruye el archivo. Cambia solo el              fragmento necesario, o repite con \"allow_shrink\": true si de verdad quieres              borrar la mayor parte del archivo.",
            (new_lines as f64 / old_lines as f64) * 100.0
        ));
    }

    fs::write(path, &updated)?;
    Ok(format!(
        "Edited {path}: {} reemplazo(s), {} → {} líneas",
        if replace_all { occurrences } else { 1 },
        content.lines().count(),
        updated.lines().count()
    ))
}

fn list_dir_tool(args: &HashMap<String, Value>) -> Result<String> {
    let path = arg_str(args, "path", ".");
    let mut entries: Vec<String> = fs::read_dir(path)
        .map_err(|e| anyhow::anyhow!("list_dir {path}: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                format!("{name}/")
            } else {
                name
            }
        })
        .collect();
    entries.sort();
    Ok(serde_json::to_string(&entries)?)
}

/// Apply a unified diff, preferring `git apply` and falling back to `patch`.
fn apply_patch_tool(args: &HashMap<String, Value>) -> Result<String> {
    let path = arg_str(args, "path", ".");
    let patch = arg_str(args, "patch", "");
    if patch.trim().is_empty() {
        return Err(anyhow::anyhow!("apply_patch requires a non-empty 'patch'"));
    }
    // Antes, una ruta que no fuese un directorio caía silenciosamente a ".",
    // así que un modelo que pasaba el archivo en vez del repositorio podía
    // aplicar el parche en el directorio equivocado. Fallar en voz alta.
    if !Path::new(path).is_dir() {
        return Err(anyhow::anyhow!(
            "apply_patch: 'path' debe ser un directorio existente (la raíz del \
             repositorio), no '{}'. Usa el directorio del proyecto.",
            path
        ));
    }
    if !arg_bool(args, "allow_manifest_overwrite", false) {
        if let Some(reason) = manifest_patch_violation(patch, Path::new(path)) {
            return Err(anyhow::anyhow!(
                "apply_patch rechazado: {reason}. Reemplazar o borrar un manifiesto \
                 entero suele romper la build. Cambia solo la línea necesaria con \
                 `edit` (por ejemplo, añadir una dependencia), o repite con \
                 \"allow_manifest_overwrite\": true si de verdad quieres sustituirlo."
            ));
        }
    }
    let cwd = path;

    let run = |program: &str, extra: &[&str]| -> Result<std::process::Output> {
        let mut child = Command::new(program)
            .args(extra)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("could not run {program}: {e}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(patch.as_bytes())?;
        }
        Ok(child.wait_with_output()?)
    };

    // Dos pasos siempre: primero comprobar, después aplicar. Un parche que no
    // encaja debe dejar el árbol EXACTAMENTE como estaba. Medido en Windows:
    // `patch` reescribe el archivo con CRLF y deja .orig/.rej incluso cuando el
    // hunk falla, así que un intento fallido contaminaba el diff del agente.
    let git_check = run("git", &["apply", "--check", "--whitespace=nowarn", "-"]);
    if let Ok(output) = &git_check {
        if output.status.success() {
            let applied = run("git", &["apply", "--whitespace=nowarn", "-"])?;
            if applied.status.success() {
                return Ok("Patch applied with git apply".into());
            }
            return Err(anyhow::anyhow!(
                "Patch passed the check but git apply failed:\n{}",
                String::from_utf8_lossy(&applied.stderr).trim()
            ));
        }
    }

    let git_stderr = match &git_check {
        Ok(output) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
        Err(e) => e.to_string(),
    };

    // Solo se recurre a `patch` si git no está instalado o no puede trabajar
    // aquí (directorio que no es un repositorio). En ese caso se usa
    // --dry-run para no escribir nada.
    let git_unavailable = git_check.is_err();
    let not_a_repo = git_stderr.contains("not a git repository");
    if git_unavailable || not_a_repo {
        let dry = run(
            "patch",
            &["-p1", "--forward", "--dry-run", "--no-backup-if-mismatch"],
        );
        if let Ok(output) = &dry {
            if output.status.success() {
                let applied = run("patch", &["-p1", "--forward", "--no-backup-if-mismatch"])?;
                if applied.status.success() {
                    return Ok("Patch applied with patch -p1".into());
                }
            }
            return Err(anyhow::anyhow!(
                "Patch did NOT apply (nothing was written).\npatch -p1 said:\n{}\n\
                 Read the real file first with read_file (start_line/end_line) and build the \
                 diff against the content you actually saw.",
                String::from_utf8_lossy(&output.stdout).trim()
            ));
        }
    }

    Err(anyhow::anyhow!(
        "Patch did NOT apply (nothing was written).\ngit apply said:\n{git_stderr}\n\
         Read the real file first with read_file (start_line/end_line) and build the diff \
         against the content you actually saw."
    ))
}

/// Símbolos del repo map con el formato de una línea por símbolo.
///
/// Delega en `repomap::extract_symbols` para que `lai repo-map` y la
/// herramienta `repo_map` del agente no diverjan.
fn symbols_in_source(content: &str) -> Vec<String> {
    extract_symbols(content)
        .into_iter()
        .map(|symbol| format!("  {} {}", symbol.kind, symbol.name))
        .collect()
}

/// Perfil de herramientas que se anuncia al modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolProfile {
    /// Todas las herramientas especializadas (lectura por rangos, grep, patch…).
    Full,
    /// Solo `bash`, para modelos pequeños y contextos ajustados.
    BashOnly,
}

impl ToolProfile {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "full" => Some(ToolProfile::Full),
            "bash" | "bash-only" | "bash_only" | "minimal" => Some(ToolProfile::BashOnly),
            _ => None,
        }
    }
}

/// Ejecuta un comando de shell con timeout. Compartido por `run_command` y
/// `bash` (perfil mínimo).
///
/// El sandbox es opt-in con `"sandbox": true`, salvo que `LAI_REQUIRE_SANDBOX`
/// (o `lai --sandbox`) lo exija, en cuyo caso todas las ejecuciones van por
/// bubblewrap y no se puede desactivar.
fn run_shell_tool(args: &HashMap<String, Value>) -> Result<String> {
    run_shell_tool_with(args, crate::sandbox::required_from_env())
}

fn run_shell_tool_with(args: &HashMap<String, Value>, require_sandbox: bool) -> Result<String> {
    let command = arg_str(args, "command", "");
    if command.trim().is_empty() {
        return Err(anyhow::anyhow!("bash requiere 'command'"));
    }
    let cwd = args.get("cwd").and_then(|v| v.as_str());
    let timeout_secs = arg_usize(args, "timeout", 300).clamp(1, 3_600);
    let explicit_sandbox = args.get("sandbox").and_then(|v| v.as_bool());
    if require_sandbox && explicit_sandbox == Some(false) {
        return Err(anyhow::anyhow!(
            "sandbox requerido por LAI_REQUIRE_SANDBOX/--sandbox: no se puede \
             desactivar con \"sandbox\": false. Quita la variable o el flag para \
             ejecutar sin aislamiento."
        ));
    }
    let want_sandbox = require_sandbox || explicit_sandbox.unwrap_or(false);
    let allow_net =
        arg_bool(args, "sandbox_network", false) || crate::sandbox::allow_net_from_env();
    if !arg_bool(args, "allow_manifest_overwrite", false) {
        if let Some(reason) = shell_manifest_violation(command) {
            return Err(anyhow::anyhow!(
                "bash rechazado: {reason} (manifiesto del proyecto). Reemplazar o \
                 borrar un manifiesto entero suele romper la build. Cambia solo la \
                 línea necesaria con `edit`, o repite con \
                 \"allow_manifest_overwrite\": true si de verdad quieres sustituirlo."
            ));
        }
    }

    let output = if want_sandbox {
        let workspace = match cwd {
            Some(dir) => std::path::PathBuf::from(dir),
            None => std::env::current_dir()?,
        };
        let cmd = crate::sandbox::build_command_full(
            command,
            &workspace,
            require_sandbox,
            allow_net,
            crate::sandbox::preflight(),
        )?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "sandbox solicitado pero no disponible: {}. Ejecuta sin \
                 \"sandbox\": true o instala bubblewrap.",
                crate::sandbox::preflight().detail
            )
        })?;
        run_with_timeout(cmd, Duration::from_secs(timeout_secs as u64))?
    } else {
        // Use appropriate shell for the OS
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg(command);
            cmd
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(command);
            cmd
        };
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        run_with_timeout(cmd, Duration::from_secs(timeout_secs as u64))?
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut result = stdout.to_string();
    if !stderr.is_empty() {
        result.push_str("\nSTDERR:\n");
        result.push_str(&stderr);
    }
    if !output.status.success() {
        result.push_str(&format!(
            "\nEXIT CODE: {}",
            output.status.code().unwrap_or(-1)
        ));
    }
    if result.trim().is_empty() {
        result = format!(
            "(no output, exit code {})",
            output.status.code().unwrap_or(-1)
        );
    }
    Ok(result)
}

#[derive(Clone)]
pub struct ToolRegistry {
    pub tools: HashMap<String, ToolFunc>,
    pub require_approval: bool,
    pub sensitive_tools: Vec<String>,
    pub policy_engine: Arc<SyncMutex<PolicyEngine>>,
}
impl ToolRegistry {
    pub fn new() -> Self {
        let policy_engine = Arc::new(SyncMutex::new(PolicyEngine::new(PolicyConfig::from_env())));
        let mut registry = Self {
            tools: HashMap::new(),
            require_approval: false,
            sensitive_tools: vec![
                "write_file".into(),
                "run_command".into(),
                "apply_patch".into(),
                "run_python".into(),
                "git_add".into(),
                "git_commit".into(),
                "git_rollback".into(),
            ],
            policy_engine,
        };
        registry.register_core_tools();
        registry
    }
    fn register_tool<F, Fut>(&mut self, name: &str, func: F)
    where
        F: Fn(HashMap<String, Value>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<String>> + Send + 'static,
    {
        let func = Arc::new(func);
        let wrapped: ToolFunc = Arc::new(move |args: HashMap<String, Value>| {
            let func = Arc::clone(&func);
            Box::pin(async move { func(args).await })
        });
        self.tools.insert(name.to_string(), wrapped);
    }
    fn register_core_tools(&mut self) {
        self.register_tool("read_file", |args| async move { read_file_tool(&args) });
        self.register_tool("write_file", |args| async move { write_file_tool(&args) });
        self.register_tool("edit", |args| async move { edit_file_tool(&args) });
        self.register_tool("list_dir", |args| async move { list_dir_tool(&args) });
        self.register_tool("run_command", |args| async move { run_shell_tool(&args) });
        // Perfil mínimo (inspirado en mini-SWE-agent): una sola herramienta de
        // shell reduce el prompt de 13 esquemas a 1, que es lo que más pesa en
        // modelos de 8k-32k tokens.
        self.register_tool("bash", |args| async move { run_shell_tool(&args) });
        self.register_tool("grep", |args| async move { grep_tool(&args) });
        self.register_tool("find_files", |args| async move { find_files_tool(&args) });
        self.register_tool("repo_map", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let max_chars = args
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .unwrap_or(8000) as usize;
            let mut blocks = Vec::new();
            let mut used = 0;
            let walker = WalkDir::new(path)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|e| {
                    !e.file_type().is_dir() || !is_skipped_dir(&e.file_name().to_string_lossy())
                });
            for entry in walker.filter_map(|e| e.ok()) {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "py" && ext != "rs" {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(p) {
                    let symbols = symbols_in_source(&content);
                    if !symbols.is_empty() {
                        let rel = p.strip_prefix(path).unwrap_or(p);
                        let block = format!("{}:\n{}", rel.display(), symbols.join("\n"));
                        let bl = block.len();
                        if used + bl > max_chars {
                            blocks.push("... (truncated)".into());
                            return Ok(blocks.join("\n\n"));
                        }
                        blocks.push(block);
                        used += bl;
                    }
                }
            }
            Ok(if blocks.is_empty() {
                "(no symbols found)".into()
            } else {
                blocks.join("\n\n")
            })
        });
        self.register_tool("run_python", |args| async move {
            let code = args.get("code").and_then(|v| v.as_str()).unwrap_or("");
            if code.trim().is_empty() {
                return Err(anyhow::anyhow!("run_python requiere 'code'"));
            }
            let timeout_secs = arg_usize(&args, "timeout", 60).clamp(1, 600);
            // `into_temp_path()` cierra el handle antes de ejecutar (en Windows
            // un NamedTempFile abierto puede impedir que Python lo lea) y sigue
            // borrando el archivo al salir del ámbito.
            let temp_path = tempfile::NamedTempFile::new()?.into_temp_path();
            fs::write(&temp_path, code)?;
            let require_sandbox = crate::sandbox::required_from_env();
            let explicit_sandbox = args.get("sandbox").and_then(|v| v.as_bool());
            if require_sandbox && explicit_sandbox == Some(false) {
                return Err(anyhow::anyhow!(
                    "sandbox requerido por LAI_REQUIRE_SANDBOX/--sandbox: no se puede \
                     desactivar con \"sandbox\": false."
                ));
            }
            let want_sandbox = require_sandbox || explicit_sandbox.unwrap_or(false);
            let cmd = if want_sandbox {
                let workspace = std::env::current_dir()?;
                let script = shell_quote(&temp_path.to_string_lossy());
                let allow_net = arg_bool(&args, "sandbox_network", false)
                    || crate::sandbox::allow_net_from_env();
                crate::sandbox::build_command_full(
                    &format!("python3 {script}"),
                    &workspace,
                    require_sandbox,
                    allow_net,
                    crate::sandbox::preflight(),
                )?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "sandbox solicitado pero no disponible: {}",
                        crate::sandbox::preflight().detail
                    )
                })?
            } else {
                let mut cmd = Command::new("python3");
                cmd.arg(&temp_path);
                cmd
            };
            let output = run_with_timeout(cmd, Duration::from_secs(timeout_secs as u64))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut result = stdout.to_string();
            if !stderr.is_empty() {
                result.push_str("\nSTDERR:\n");
                result.push_str(&stderr);
            }
            if !output.status.success() {
                result.push_str(&format!(
                    "\nEXIT CODE: {}",
                    output.status.code().unwrap_or(-1)
                ));
            }
            if result.trim().is_empty() {
                result = format!(
                    "(no output, exit code {})",
                    output.status.code().unwrap_or(-1)
                );
            }
            Ok(result)
        });
        self.register_tool("apply_patch", |args| async move { apply_patch_tool(&args) });
        self.register_tool("git_add", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let files = args.get("files").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("git")
                .args(["add", files])
                .current_dir(path)
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });
        self.register_tool("git_commit", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let message = args
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Auto commit");
            let output = Command::new("git")
                .args(["commit", "-m", message])
                .current_dir(path)
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });
        self.register_tool("git_status", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("git")
                .args(["status"])
                .current_dir(path)
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });
        self.register_tool("git_diff", |args| async move {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let output = Command::new("git")
                .args(["diff"])
                .current_dir(path)
                .output()?;
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        });
    }
    pub async fn call_tool(&self, name: &str, args: HashMap<String, Value>) -> Result<String> {
        let decision = {
            let mut policy = self.policy_engine.lock().unwrap();
            policy.check_tool(name, &args)
        };
        if !decision.allowed {
            return Err(anyhow::anyhow!(
                "PolicyEngine blocked tool '{}': {}",
                name,
                decision.reason
            ));
        }
        if let Some(func) = self.tools.get(name) {
            func(args).await
        } else {
            Err(anyhow::anyhow!("Tool not found: {}", name))
        }
    }
    /// Perfil de herramientas expuesto al modelo.
    ///
    /// `Full` son las 14 herramientas especializadas. `BashOnly` expone solo
    /// `bash`: el prompt pesa mucho menos y los modelos pequeños no tienen que
    /// elegir entre 14 esquemas (mini-SWE-agent demuestra que un agente
    /// bash-only es competitivo y arranca más rápido).
    pub fn get_tools_schema_for(&self, profile: ToolProfile) -> Vec<ToolSchema> {
        let all = self.get_tools_schema();
        match profile {
            ToolProfile::Full => all,
            ToolProfile::BashOnly => all.into_iter().filter(|tool| tool.name == "bash").collect(),
        }
    }

    pub fn get_tools_schema(&self) -> Vec<ToolSchema> {
        vec![
            ToolSchema {
                name: "read_file".into(),
                description: "Read a file or a 1-based line range. Large files are capped at 400 lines by default; use start_line/end_line/max_lines for precise reads. Refuses to read credential files (~/.ssh, ~/.aws, *.pem, id_rsa, ...) unless allow_sensitive_read is true.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer","description":"First line, 1-based (default 1)"},"end_line":{"type":"integer","description":"Last line, inclusive"},"max_lines":{"type":"integer","description":"Maximum lines to return (default 400, max 2000)"},"allow_sensitive_read":{"type":"boolean","description":"Permit reading a credential-like path"}},"required":["path"]}),
            },
            ToolSchema {
                name: "write_file".into(),
                description: "Write a file, creating parent directories. Refuses to replace an existing file of 40+ lines with less than 25% of its original length unless allow_shrink is true (that pattern usually means an example was written over real code). Also refuses to replace an existing project manifest (Cargo.toml, package.json, pyproject.toml, ...) unless allow_manifest_overwrite is true; use `edit` for a targeted manifest change. Prefer `edit` for targeted edits.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"allow_shrink":{"type":"boolean","description":"Permit replacing a large file with a much smaller one"},"allow_manifest_overwrite":{"type":"boolean","description":"Permit replacing an existing project manifest (Cargo.toml, package.json, ...) entirely"}},"required":["path","content"]}),
            },
            ToolSchema {
                name: "edit".into(),
                description: "Edit an existing file by replacing an exact fragment. Args: path, old_string (copied exactly from read_file), new_string. Fails if old_string is missing or appears more than once (use replace_all or more context). Refuses to shrink a 40+ line file below 25% of its length unless allow_shrink is true, and refuses to replace a whole project manifest unless allow_manifest_overwrite is true. PREFERRED over write_file for existing files.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"},"replace_all":{"type":"boolean","description":"Replace every occurrence (default false)"},"allow_shrink":{"type":"boolean","description":"Permit a replacement that removes most of the file"},"allow_manifest_overwrite":{"type":"boolean","description":"Permit replacing a whole project manifest (Cargo.toml, package.json, ...)"}},"required":["path","old_string","new_string"]}),
            },
            ToolSchema {
                name: "list_dir".into(),
                description: "List a directory (sorted; directories end with '/')".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            },
            ToolSchema {
                name: "run_command".into(),
                description: "Run a shell command. The process is killed if it exceeds the timeout. Set sandbox=true to run inside bubblewrap (no network by default, workspace writable); with LAI_REQUIRE_SANDBOX=1 the sandbox is mandatory and fails closed. Refuses obvious manifest destruction (redirect/rm/tee over Cargo.toml, package.json, ...) unless allow_manifest_overwrite is true.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"timeout":{"type":"integer","description":"Seconds before the command is killed (default 300, max 3600)"},"sandbox":{"type":"boolean","description":"Run inside bubblewrap (default false unless LAI_REQUIRE_SANDBOX is set)"},"sandbox_network":{"type":"boolean","description":"Allow network inside the sandbox (default false)"},"allow_manifest_overwrite":{"type":"boolean","description":"Permit a command that overwrites or deletes a project manifest"}},"required":["command"]}),
            },
            ToolSchema {
                name: "bash".into(),
                description: "Run one shell command (same as run_command). Use it to inspect the repo and edit files with heredocs. Honors the same sandbox and manifest rules.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"timeout":{"type":"integer","description":"Seconds before the command is killed (default 300, max 3600)"},"sandbox":{"type":"boolean","description":"Run inside bubblewrap (default false unless LAI_REQUIRE_SANDBOX is set)"},"sandbox_network":{"type":"boolean","description":"Allow network inside the sandbox (default false)"},"allow_manifest_overwrite":{"type":"boolean","description":"Permit a command that overwrites or deletes a project manifest"}},"required":["command"]}),
            },
            ToolSchema {
                name: "grep".into(),
                description: "Search files for a literal or regex pattern. Skips .git, target, node_modules and similar build/VCS directories. Returns 'path:line: text'.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string","description":"File or directory (default '.')"},"regex":{"type":"boolean","description":"Treat pattern as a regular expression"},"case_insensitive":{"type":"boolean"},"glob":{"type":"string","description":"Only search files matching this glob, e.g. '*.rs'"},"max_results":{"type":"integer","description":"Default 200, max 1000"}},"required":["pattern"]}),
            },
            ToolSchema {
                name: "find_files".into(),
                description: "Find files by glob pattern (supports 'src/**/*.rs'). Skips build/VCS directories.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string","description":"Directory (default '.')"},"max_results":{"type":"integer","description":"Default 200, max 1000"}},"required":["pattern"]}),
            },
            ToolSchema {
                name: "repo_map".into(),
                description: "Repo symbol map for Python/Rust files (skips build/VCS directories)".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"max_chars":{"type":"integer","description":"Maximum output size (default 8000)"}},"required":["path"]}),
            },
            ToolSchema {
                name: "run_python".into(),
                description: "Run Python code in a temporary file. The process is killed if it exceeds the timeout. Set sandbox=true to run inside bubblewrap; with LAI_REQUIRE_SANDBOX=1 the sandbox is mandatory and fails closed.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"code":{"type":"string"},"timeout":{"type":"integer","description":"Seconds before the process is killed (default 60, max 600)"},"sandbox":{"type":"boolean","description":"Run inside bubblewrap (default false unless LAI_REQUIRE_SANDBOX is set)"},"sandbox_network":{"type":"boolean","description":"Allow network inside the sandbox (default false)"}},"required":["code"]}),
            },
            ToolSchema {
                name: "apply_patch".into(),
                description: "Apply a unified diff to an existing file (git apply --check first, then apply). 'path' is the repository root. Returns an ERROR if the hunk does not match: nothing is written in that case, so read the file with read_file before retrying. Refuses to replace or delete a whole project manifest unless allow_manifest_overwrite is true.".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"patch":{"type":"string"},"allow_manifest_overwrite":{"type":"boolean","description":"Permit a patch that replaces or deletes a whole project manifest"}},"required":["path","patch"]}),
            },
            ToolSchema {
                name: "git_add".into(),
                description: "Git add".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"files":{"type":"string"}},"required":["path"]}),
            },
            ToolSchema {
                name: "git_commit".into(),
                description: "Git commit".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"message":{"type":"string"}},"required":["path"]}),
            },
            ToolSchema {
                name: "git_status".into(),
                description: "Git status".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            },
            ToolSchema {
                name: "git_diff".into(),
                description: "Git diff".into(),
                parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            },
        ]
    }
}
impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(value: Value) -> HashMap<String, Value> {
        serde_json::from_value(value).expect("test arguments must be an object")
    }

    fn text(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn read_file_honours_line_range() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sample.txt");
        fs::write(&file, "one\ntwo\nthree\nfour\n").unwrap();

        let out = read_file_tool(&args(json!({
            "path": file.to_string_lossy(),
            "start_line": 2,
            "end_line": 3
        })))
        .unwrap();

        assert!(out.contains("lines 2-3 of 4"), "{out}");
        assert!(out.contains("two\nthree"), "{out}");
        assert!(!out.contains("one"), "{out}");
    }

    #[test]
    fn read_file_caps_large_files_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("large.txt");
        let content: String = (1..=500).map(|i| format!("line {i}\n")).collect();
        fs::write(&file, content).unwrap();

        let out = read_file_tool(&args(json!({"path": file.to_string_lossy()}))).unwrap();

        assert!(out.contains("lines 1-400 of 500"), "{out}");
        assert!(out.contains("100 more lines"), "{out}");
    }

    #[test]
    fn read_file_rejects_binary_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("binary.bin");
        fs::write(&file, [0xff, 0x00, 0x41]).unwrap();

        let err = read_file_tool(&args(json!({"path": file.to_string_lossy()}))).unwrap_err();
        assert!(err.to_string().contains("binary file"), "{err}");
    }

    #[test]
    fn grep_supports_regex_case_and_glob_and_skips_noise() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("target")).unwrap();
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("src/a.rs"), "Hello world\nsecond HELLO\n").unwrap();
        fs::write(dir.path().join("target/b.rs"), "Hello target\n").unwrap();
        fs::write(dir.path().join("node_modules/c.rs"), "Hello node\n").unwrap();

        let out = grep_tool(&args(json!({
            "pattern": "h.llo",
            "path": dir.path().to_string_lossy(),
            "regex": true,
            "case_insensitive": true,
            "glob": "*.rs"
        })))
        .unwrap();

        assert!(out.contains("a.rs:1:"), "{out}");
        assert!(out.contains("a.rs:2:"), "{out}");
        assert!(!out.contains("target"), "{out}");
        assert!(!out.contains("node_modules"), "{out}");
    }

    #[test]
    fn grep_reports_missing_path_instead_of_no_matches() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-existe");
        let err = grep_tool(&args(json!({
            "pattern": "x",
            "path": missing.to_string_lossy()
        })))
        .unwrap_err()
        .to_string();
        assert!(err.contains("no existe"), "{err}");
    }

    #[test]
    fn find_files_reports_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-existe");
        let err = find_files_tool(&args(json!({
            "pattern": "*.rs",
            "path": missing.to_string_lossy()
        })))
        .unwrap_err()
        .to_string();
        assert!(err.contains("no existe"), "{err}");
    }

    #[test]
    fn grep_reports_invalid_regex_instead_of_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let err = grep_tool(&args(json!({
            "pattern": "(",
            "path": dir.path().to_string_lossy(),
            "regex": true
        })))
        .unwrap_err();
        assert!(err.to_string().contains("invalid regex"), "{err}");
    }

    #[test]
    fn find_files_reports_invalid_glob_instead_of_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let err = find_files_tool(&args(json!({
            "pattern": "[",
            "path": dir.path().to_string_lossy()
        })))
        .unwrap_err();
        assert!(err.to_string().contains("invalid glob"), "{err}");
    }

    #[test]
    fn find_files_matches_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src/nested")).unwrap();
        fs::write(dir.path().join("src/nested/a.rs"), "fn main() {}\n").unwrap();
        fs::write(dir.path().join("src/ignored.txt"), "x\n").unwrap();

        let out = find_files_tool(&args(json!({
            "pattern": "src/**/*.rs",
            "path": dir.path().to_string_lossy()
        })))
        .unwrap();

        assert!(out.contains("a.rs"), "{out}");
        assert!(!out.contains("ignored.txt"), "{out}");
    }

    #[test]
    fn list_dir_is_sorted_and_marks_directories() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("beta")).unwrap();
        fs::create_dir(dir.path().join("alpha")).unwrap();
        fs::write(dir.path().join("z.txt"), "z").unwrap();

        let out = list_dir_tool(&args(json!({"path": dir.path().to_string_lossy()}))).unwrap();
        let entries: Vec<String> = serde_json::from_str(&out).unwrap();

        assert_eq!(entries, vec!["alpha/", "beta/", "z.txt"]);
        assert!(text(&out).contains("alpha/"));
    }

    /// El caso real del piloto: 1911 líneas del core de Django reemplazadas por
    /// las 18 líneas de un ejemplo. Sin la guarda, el agente destruye el archivo.
    #[test]
    fn write_file_refuses_to_destroy_a_large_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("base.py");
        let original: String = (0..1911).map(|i| format!("line {i}\n")).collect();
        fs::write(&path, &original).unwrap();

        let err = write_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "content": "from uuid import uuid4\nfrom django.db import models\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("1911"), "{err}");
        assert!(err.contains("allow_shrink"), "{err}");
        // El archivo original sigue intacto.
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn write_file_allows_shrink_with_explicit_flag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.py");
        let original: String = (0..100).map(|i| format!("line {i}\n")).collect();
        fs::write(&path, &original).unwrap();

        let out = write_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "content": "short\n",
            "allow_shrink": true
        })))
        .unwrap();

        assert!(out.contains("Written to"), "{out}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "short\n");
    }

    #[test]
    fn write_file_allows_growing_and_moderate_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mod.rs");
        fs::write(&path, "a\nb\nc\n").unwrap();

        // Crecer: siempre permitido.
        write_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "content": "a\nb\nc\nd\ne\n"
        })))
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\nc\nd\ne\n");

        // Archivo pequeño: la guarda no aplica aunque se reduzca mucho.
        let small = dir.path().join("small.rs");
        fs::write(&small, "one\ntwo\nthree\n").unwrap();
        write_file_tool(&args(json!({
            "path": small.to_string_lossy(),
            "content": "one\n"
        })))
        .unwrap();
        assert_eq!(fs::read_to_string(&small).unwrap(), "one\n");

        // Reducción por encima del umbral del 25 %: permitida.
        let mid = dir.path().join("mid.rs");
        let original: String = (0..100).map(|i| format!("line {i}\n")).collect();
        fs::write(&mid, &original).unwrap();
        let kept: String = (0..40).map(|i| format!("line {i}\n")).collect();
        write_file_tool(&args(json!({
            "path": mid.to_string_lossy(),
            "content": kept
        })))
        .unwrap();
        assert_eq!(fs::read_to_string(&mid).unwrap(), kept);
    }

    /// Caso real del benchmark del 2026-11-26: el agente sustituyó `Cargo.toml`
    /// por uno inventado y el proyecto dejó de compilar.
    #[test]
    fn write_file_refuses_to_replace_a_manifest_without_flag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        let original = "[package]\nname = \"real\"\nversion = \"0.1.0\"\n";
        fs::write(&path, original).unwrap();

        let err = write_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "content": "[package]\nname = \"invented\"\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("manifiesto"), "{err}");
        assert!(err.contains("allow_manifest_overwrite"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn write_file_allows_manifest_overwrite_with_explicit_flag() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("package.json");
        fs::write(&path, "{\"name\":\"real\"}\n").unwrap();

        write_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "content": "{\"name\":\"new\"}\n",
            "allow_manifest_overwrite": true
        })))
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"name\":\"new\"}\n");
    }

    #[test]
    fn write_file_can_create_a_new_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pyproject.toml");

        write_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "content": "[project]\nname = \"nuevo\"\n"
        })))
        .unwrap();

        assert!(path.is_file());
    }

    #[test]
    fn edit_replaces_a_unique_fragment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.py");
        fs::write(&path, "def suma(a, b):\n    return a - b\n").unwrap();

        let out = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "return a - b",
            "new_string": "return a + b"
        })))
        .unwrap();

        assert!(out.contains("1 reemplazo"), "{out}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "def suma(a, b):\n    return a + b\n"
        );
    }

    #[test]
    fn edit_refuses_ambiguous_fragments_without_replace_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.py");
        let original = "x = 1\ny = 1\n";
        fs::write(&path, original).unwrap();

        let err = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "= 1",
            "new_string": "= 2"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("2 veces"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn edit_reports_missing_fragment_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.py");
        fs::write(&path, "hola\n").unwrap();

        let err = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "adios",
            "new_string": "hola"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("no aparece"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "hola\n");
    }

    #[test]
    fn edit_replace_all_changes_every_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.py");
        fs::write(&path, "x = 1\ny = 1\n").unwrap();

        edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "= 1",
            "new_string": "= 2",
            "replace_all": true
        })))
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "x = 2\ny = 2\n");
    }

    #[test]
    fn edit_refuses_to_destroy_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pastebin.py");
        let original: String = (0..71).map(|i| format!("line {i}\n")).collect();
        fs::write(&path, &original).unwrap();

        let err = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": original,
            "new_string": "import sys\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("edit rechazado"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn edit_refuses_to_replace_a_whole_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pyproject.toml");
        let original = "[project]\nname = \"real\"\nversion = \"1.0\"\n";
        fs::write(&path, original).unwrap();

        let err = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": original,
            "new_string": "[project]\nname = \"invented\"\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("manifiesto"), "{err}");
        assert!(err.contains("allow_manifest_overwrite"), "{err}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn edit_allows_a_targeted_manifest_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        fs::write(&path, "[package]\nname = \"real\"\n\n[dependencies]\n").unwrap();

        edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "[dependencies]\n",
            "new_string": "[dependencies]\nserde = \"1\"\n"
        })))
        .unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[package]\nname = \"real\"\n\n[dependencies]\nserde = \"1\"\n"
        );
    }

    #[test]
    fn edit_rejects_identical_strings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.py");
        fs::write(&path, "hola\n").unwrap();

        let err = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "hola",
            "new_string": "hola"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("idénticos"), "{err}");
    }

    #[test]
    fn write_file_requires_path() {
        let err = write_file_tool(&args(json!({"content": "x"})))
            .unwrap_err()
            .to_string();
        assert!(err.contains("path"), "{err}");
    }

    #[test]
    fn write_file_requires_content_instead_of_truncating() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keep.txt");
        fs::write(&path, "important\n").unwrap();

        let err = write_file_tool(&args(json!({"path": path.to_string_lossy()})))
            .unwrap_err()
            .to_string();

        assert!(err.contains("content"), "{err}");
        // Un `content` ausente no puede convertirse en un archivo vacío.
        assert_eq!(fs::read_to_string(&path).unwrap(), "important\n");
    }

    #[test]
    fn apply_patch_rejects_a_file_instead_of_silently_using_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("real.txt");
        fs::write(&file, "alpha\n").unwrap();

        let err = apply_patch_tool(&args(json!({
            "path": file.to_string_lossy(),
            "patch": "--- a/real.txt\n+++ b/real.txt\n@@ -1 +1 @@\n-alpha\n+beta\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("directorio"), "{err}");
        assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\n");
    }

    #[test]
    fn repo_map_extracts_rust_visibility_and_async_symbols() {
        let source = "\
pub async fn fetch() {}\n\
async fn load() {}\n\
pub fn run() {}\n\
fn private() {}\n\
struct Foo {\n\
}\n\
pub struct Bar<T> {}\n\
class Baz(Base):\n\
    pass\n\
async def go():\n\
    pass\n\
";
        let symbols = symbols_in_source(source);

        for expected in [
            "  fn fetch",
            "  fn load",
            "  fn run",
            "  fn private",
            "  struct Foo",
            "  struct Bar",
            "  class Baz",
            "  def go",
        ] {
            assert!(
                symbols.iter().any(|s| s == expected),
                "missing {expected:?} in {symbols:?}"
            );
        }
        assert!(
            !symbols.iter().any(|s| s.contains("c fn")),
            "async fn was sliced incorrectly: {symbols:?}"
        );
    }

    #[test]
    fn bash_profile_exposes_only_the_shell_tool() {
        let registry = ToolRegistry::new();
        let full = registry.get_tools_schema_for(ToolProfile::Full);
        let bash = registry.get_tools_schema_for(ToolProfile::BashOnly);

        assert!(full.len() > 10);
        assert_eq!(bash.len(), 1, "el perfil mínimo debe exponer una sola tool");
        assert_eq!(bash[0].name, "bash");
        assert!(registry.tools.contains_key("bash"));
    }

    #[test]
    fn tool_profile_parses_known_names() {
        assert_eq!(ToolProfile::from_str("full"), Some(ToolProfile::Full));
        assert_eq!(ToolProfile::from_str("bash"), Some(ToolProfile::BashOnly));
        assert_eq!(
            ToolProfile::from_str("MINIMAL"),
            Some(ToolProfile::BashOnly)
        );
        assert_eq!(ToolProfile::from_str("otro"), None);
    }

    #[tokio::test]
    async fn bash_tool_runs_a_command() {
        let registry = ToolRegistry::new();
        let func = registry.tools.get("bash").unwrap();
        let out = func(args(json!({"command": "echo hola"}))).await.unwrap();
        assert!(out.contains("hola"), "{out}");
    }

    #[tokio::test]
    async fn run_command_requires_a_command() {
        let registry = ToolRegistry::new();
        let func = registry.tools.get("run_command").unwrap();
        let err = func(args(json!({"timeout": 1}))).await.unwrap_err();
        assert!(err.to_string().contains("command"), "{err}");
    }

    #[tokio::test]
    async fn run_command_honours_the_timeout() {
        let registry = ToolRegistry::new();
        let func = registry.tools.get("run_command").unwrap();
        let started = Instant::now();
        let err = func(args(json!({"command": "sleep 5", "timeout": 1})))
            .await
            .unwrap_err();

        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "timeout took too long: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn run_python_requires_code() {
        let registry = ToolRegistry::new();
        let func = registry.tools.get("run_python").unwrap();
        let err = func(args(json!({"timeout": 1}))).await.unwrap_err();
        assert!(err.to_string().contains("code"), "{err}");
    }

    #[test]
    fn write_file_creates_missing_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("new.rs");
        write_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "content": "fn main() {}\n"
        })))
        .unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fn main() {}\n");
    }

    #[test]
    fn apply_patch_reports_failure_as_error() {
        // Un parche que no encaja NO puede devolver Ok: el bucle del agente
        // imprimiría "Done" y el modelo repetiría la misma llamada.
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("real.txt");
        fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();

        let err = apply_patch_tool(&args(json!({
            "path": dir.path().to_string_lossy(),
            "patch": "--- a/real.txt\n+++ b/real.txt\n@@ -1,3 +1,3 @@\n-this line does not exist\n+replacement\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("did NOT apply"), "{err}");
        // Y el archivo queda intacto, byte por byte: no se escribió nada.
        assert_eq!(fs::read_to_string(&file).unwrap(), "alpha\nbeta\ngamma\n");
        assert!(!dir.path().join("real.txt.rej").exists());
        assert!(!dir.path().join("real.txt.orig").exists());
    }

    #[test]
    fn apply_patch_applies_a_valid_hunk_in_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ok = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(root)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return; // sin git no hay nada que comprobar aquí
        }
        let file = root.join("real.txt");
        fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();

        let out = apply_patch_tool(&args(json!({
            "path": root.to_string_lossy(),
            "patch": "--- a/real.txt\n+++ b/real.txt\n@@ -1,3 +1,3 @@\n alpha\n-beta\n+BETA\n gamma\n"
        })))
        .unwrap();

        assert!(out.contains("applied"), "{out}");
        // git puede normalizar finales de línea según core.autocrlf; lo que
        // importa es que el hunk se aplicó.
        assert_eq!(
            fs::read_to_string(&file).unwrap().replace("\r\n", "\n"),
            "alpha\nBETA\ngamma\n"
        );
    }

    #[test]
    fn apply_patch_requires_patch_text() {
        let err = apply_patch_tool(&args(json!({"path": ".", "patch": "   "})))
            .unwrap_err()
            .to_string();
        assert!(err.contains("patch"), "{err}");
    }

    fn init_git_repo(dir: &Path) -> bool {
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn apply_patch_rejects_replacing_a_whole_manifest() {
        let dir = tempfile::tempdir().unwrap();
        if !init_git_repo(dir.path()) {
            return;
        }
        let manifest = dir.path().join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1\"\n",
        )
        .unwrap();

        let err = apply_patch_tool(&args(json!({
            "path": dir.path().to_string_lossy(),
            "patch": "--- a/Cargo.toml\n+++ b/Cargo.toml\n@@ -1,6 +1,2 @@\n-[package]\n-name = \"demo\"\n-version = \"0.1.0\"\n-\n-[dependencies]\n-serde = \"1\"\n+[package]\n+name = \"invented\"\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("manifiesto"), "{err}");
        // Nada se escribió.
        assert!(fs::read_to_string(&manifest).unwrap().contains("serde"));
    }

    #[test]
    fn apply_patch_allows_a_targeted_manifest_change() {
        let dir = tempfile::tempdir().unwrap();
        if !init_git_repo(dir.path()) {
            return;
        }
        let manifest = dir.path().join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
        )
        .unwrap();

        let out = apply_patch_tool(&args(json!({
            "path": dir.path().to_string_lossy(),
            "patch": "--- a/Cargo.toml\n+++ b/Cargo.toml\n@@ -4,2 +4,3 @@\n \n [dependencies]\n+serde = \"1\"\n"
        })))
        .unwrap();

        assert!(out.contains("applied"), "{out}");
        assert!(fs::read_to_string(&manifest).unwrap().contains("serde"));
    }

    #[test]
    fn apply_patch_rejects_deleting_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        if !init_git_repo(dir.path()) {
            return;
        }
        let manifest = dir.path().join("package.json");
        fs::write(&manifest, "{\n  \"name\": \"demo\"\n}\n").unwrap();

        let err = apply_patch_tool(&args(json!({
            "path": dir.path().to_string_lossy(),
            "patch": "--- a/package.json\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-{\n-  \"name\": \"demo\"\n-}\n"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("manifiesto"), "{err}");
        assert!(manifest.exists());
    }

    #[test]
    fn apply_patch_allows_replacing_a_manifest_with_opt_in() {
        let dir = tempfile::tempdir().unwrap();
        if !init_git_repo(dir.path()) {
            return;
        }
        let manifest = dir.path().join("Cargo.toml");
        fs::write(&manifest, "[package]\nname = \"demo\"\n").unwrap();

        let out = apply_patch_tool(&args(json!({
            "path": dir.path().to_string_lossy(),
            "patch": "--- a/Cargo.toml\n+++ b/Cargo.toml\n@@ -1,2 +1,2 @@\n-[package]\n-name = \"demo\"\n+[package]\n+name = \"renamed\"\n",
            "allow_manifest_overwrite": true
        })))
        .unwrap();

        assert!(out.contains("applied"), "{out}");
        assert!(fs::read_to_string(&manifest).unwrap().contains("renamed"));
    }

    #[test]
    fn run_shell_tool_without_sandbox_runs_plain() {
        let out = run_shell_tool_with(&args(json!({"command": "echo plain-ok"})), false).unwrap();
        assert!(out.contains("plain-ok"), "{out}");
    }

    #[test]
    fn run_shell_tool_required_rejects_explicit_disable() {
        let err = run_shell_tool_with(&args(json!({"command": "echo hi", "sandbox": false})), true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("no se puede"), "{err}");
    }

    #[test]
    fn run_shell_tool_runs_under_sandbox_when_requested() {
        if !crate::sandbox::preflight().available {
            return; // sin bwrap no se puede comprobar el camino feliz
        }
        let out = run_shell_tool_with(
            &args(json!({"command": "echo sandbox-ok", "sandbox": true})),
            true,
        )
        .unwrap();
        assert!(out.contains("sandbox-ok"), "{out}");
    }

    #[test]
    fn sandbox_blocks_network_access() {
        if !crate::sandbox::preflight().available {
            return;
        }
        let out = run_shell_tool_with(
            &args(json!({
                "command": "getent hosts example.com >/dev/null 2>&1 && echo NET_UP || echo NO_NET",
                "sandbox": true
            })),
            true,
        )
        .unwrap();
        assert!(
            out.contains("NO_NET"),
            "la red no debería estar disponible: {out}"
        );
    }

    #[test]
    fn shell_guard_rejects_redirect_into_manifest() {
        let err = run_shell_tool_with(&args(json!({"command": "echo x > Cargo.toml"})), false)
            .unwrap_err()
            .to_string();
        assert!(err.contains("manifiesto"), "{err}");
    }

    #[test]
    fn shell_guard_rejects_heredoc_into_manifest() {
        let err = run_shell_tool_with(
            &args(json!({"command": "cat > package.json <<EOF\n{}\nEOF"})),
            false,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("manifiesto"), "{err}");
    }

    #[test]
    fn shell_guard_rejects_rm_and_tee_on_manifest() {
        for command in [
            "rm -f pyproject.toml",
            "tee go.mod",
            "dd if=/dev/zero of=Cargo.lock",
        ] {
            let err = run_shell_tool_with(&args(json!({"command": command})), false)
                .unwrap_err()
                .to_string();
            assert!(err.contains("manifiesto"), "{command}: {err}");
        }
    }

    #[test]
    fn shell_guard_allows_reading_and_targeted_writes() {
        // Leer un manifiesto y escribir otro archivo no se bloquea.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let out = run_shell_tool_with(
            &args(json!({
                "command": "cat Cargo.toml >/dev/null 2>&1; echo ok > notas.txt",
                "cwd": dir.path().to_string_lossy()
            })),
            false,
        )
        .unwrap();
        assert!(!out.contains("rechazado"), "{out}");
        assert!(dir.path().join("notas.txt").exists());
    }

    #[test]
    fn shell_guard_can_be_disabled_explicitly() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Cargo.toml");
        let out = run_shell_tool_with(
            &args(json!({
                "command": "echo x > Cargo.toml",
                "cwd": dir.path().to_string_lossy(),
                "allow_manifest_overwrite": true
            })),
            false,
        )
        .unwrap();
        assert!(!out.contains("rechazado"), "{out}");
        assert_eq!(fs::read_to_string(&target).unwrap().trim(), "x");
    }

    #[test]
    fn shell_words_handles_quotes() {
        let words = shell_words(r#"echo "a b" 'c d' > "Cargo.toml""#);
        assert_eq!(words, vec!["echo", "a b", "c d", ">", "Cargo.toml"]);
    }

    #[test]
    fn edit_falls_back_to_indentation_agnostic_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("code.py");
        fs::write(&path, "def f():\n    return 1\n\nprint(f())\n").unwrap();

        // old_string con indentación incorrecta (sin los 4 espacios).
        let out = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "def f():\nreturn 1",
            "new_string": "def f():\n    return 2"
        })))
        .unwrap();

        assert!(out.contains("Edited"), "{out}");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "def f():\n    return 2\n\nprint(f())\n"
        );
    }

    #[test]
    fn edit_indentation_fallback_refuses_ambiguous_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("code.py");
        fs::write(&path, "if a:\n    x = 1\nif b:\n    x = 1\n").unwrap();

        let err = edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "x = 1",
            "new_string": "x = 2"
        })))
        .unwrap_err()
        .to_string();

        assert!(err.contains("veces"), "{err}");
        // Nada se escribió.
        assert!(fs::read_to_string(&path).unwrap().contains("x = 1"));
    }

    #[test]
    fn edit_indentation_fallback_keeps_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("code.py");
        fs::write(&path, "  a = 1\n  b = 2\nc = 3\n").unwrap();

        edit_file_tool(&args(json!({
            "path": path.to_string_lossy(),
            "old_string": "a = 1\nb = 2",
            "new_string": "a = 9\nb = 8"
        })))
        .unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "  a = 9\n  b = 8\nc = 3\n"
        );
    }

    #[test]
    fn indentation_agnostic_span_returns_none_without_match() {
        assert!(indentation_agnostic_span("alpha\nbeta\n", "gamma").is_none());
    }

    #[test]
    fn sandbox_hides_home_directory_files() {
        if !crate::sandbox::preflight().available {
            return;
        }
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let decoy = std::path::PathBuf::from(home)
            .join(format!(".lai-sandbox-probe-{}", std::process::id()));
        if fs::write(&decoy, "secret").is_err() {
            return;
        }
        let out = run_shell_tool_with(
            &args(json!({
                "command": format!(
                    "cat '{}' >/dev/null 2>&1 && echo LEAK || echo HIDDEN",
                    decoy.display()
                ),
                "sandbox": true
            })),
            true,
        )
        .unwrap();
        let _ = fs::remove_file(&decoy);
        assert!(
            out.contains("HIDDEN"),
            "HOME no debería estar montado: {out}"
        );
    }

    #[test]
    fn sandbox_scrubs_secret_environment_variables() {
        if !crate::sandbox::preflight().available {
            return;
        }
        let out = run_shell_tool_with(
            &args(json!({
                "command": "echo \"tok=[$GITHUB_TOKEN] ssh=[$SSH_AUTH_SOCK]\"",
                "sandbox": true
            })),
            true,
        )
        .unwrap();
        assert!(!out.contains("tok=[") || out.contains("tok=[]"), "{out}");
        assert!(out.contains("tok=[]"), "{out}");
        assert!(out.contains("ssh=[]"), "{out}");
    }

    #[test]
    fn every_registered_tool_has_a_schema() {
        // Lección de Bees: nunca anunciar al modelo una herramienta que no
        // puede ver, ni registrar una que no se anuncia.
        let registry = ToolRegistry::new();
        let schemas: Vec<String> = registry
            .get_tools_schema()
            .into_iter()
            .map(|s| s.name)
            .collect();
        for name in registry.tools.keys() {
            assert!(
                schemas.contains(name),
                "la herramienta '{name}' está registrada pero no tiene esquema"
            );
        }
        for name in &schemas {
            assert!(
                registry.tools.contains_key(name),
                "el esquema '{name}' no corresponde a ninguna herramienta"
            );
        }
    }

    #[test]
    fn bash_profile_exposes_exactly_one_tool() {
        let registry = ToolRegistry::new();
        let schemas = registry.get_tools_schema_for(ToolProfile::BashOnly);
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].name, "bash");
    }

    #[test]
    fn sandbox_blocks_writes_outside_the_workspace() {
        if !crate::sandbox::preflight().available {
            return;
        }
        let out = run_shell_tool_with(
            &args(json!({
                "command": "touch /etc/lai-sandbox-was-here 2>/dev/null && echo WROTE || echo READONLY",
                "sandbox": true
            })),
            true,
        )
        .unwrap();
        assert!(
            out.contains("READONLY"),
            "/etc debería ser de solo lectura: {out}"
        );
    }

    #[test]
    fn sandbox_masks_run_directory() {
        if !crate::sandbox::preflight().available {
            return;
        }
        let out = run_shell_tool_with(
            &args(json!({
                "command": "ls -A /run 2>/dev/null | head -5",
                "sandbox": true
            })),
            true,
        )
        .unwrap();
        assert!(
            out.trim().is_empty() || out.contains("(no output"),
            "/run debería estar vacío: {out}"
        );
    }

    #[test]
    fn read_file_rejects_credential_paths() {
        for path in [
            "/home/user/.ssh/id_ed25519",
            "/home/user/.aws/credentials",
            "/home/user/project/id_rsa",
            "C:\\Users\\me\\.ssh\\id_rsa",
            "/home/user/.npmrc",
            "/home/user/private.pem",
            "/home/user/.docker/config.json",
        ] {
            let err = read_file_tool(&args(json!({"path": path})))
                .unwrap_err()
                .to_string();
            assert!(err.contains("credencial"), "{path}: {err}");
        }
    }

    #[test]
    fn read_file_allows_public_and_normal_files() {
        let dir = tempfile::tempdir().unwrap();
        let normal = dir.path().join("main.rs");
        fs::write(&normal, "fn main() {}\n").unwrap();
        assert!(read_file_tool(&args(json!({"path": normal.to_string_lossy()}))).is_ok());

        let public_key = dir.path().join("id_rsa.pub");
        fs::write(&public_key, "ssh-rsa AAA\n").unwrap();
        assert!(read_file_tool(&args(json!({"path": public_key.to_string_lossy()}))).is_ok());
    }

    #[test]
    fn read_file_can_override_sensitive_guard() {
        let dir = tempfile::tempdir().unwrap();
        let key = dir.path().join("id_rsa");
        fs::write(&key, "secret\n").unwrap();
        let out = read_file_tool(&args(json!({
            "path": key.to_string_lossy(),
            "allow_sensitive_read": true
        })))
        .unwrap();
        assert!(out.contains("secret"), "{out}");
    }

    #[test]
    fn grep_rejects_credential_directories() {
        let err = grep_tool(&args(json!({
            "pattern": "PRIVATE KEY",
            "path": "/home/user/.ssh"
        })))
        .unwrap_err()
        .to_string();
        assert!(err.contains("credencial"), "{err}");
    }
}
