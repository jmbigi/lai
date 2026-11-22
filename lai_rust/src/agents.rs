//! Multi-Agent Orchestrator - Specialized sub-agents for code review, critique, etc.
//! Ported from scripts/lai_agents.py with full feature parity.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use crate::llm::{ChatResult, LlmClient, Message};
use crate::tools::ToolRegistry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub role: String,
    pub system_prompt: String,
    pub temperature: f64,
    pub max_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    pub agent: String,
    pub output: String,
    pub tool_calls: Vec<Value>,
    pub score: Option<f64>,
}

pub struct MultiAgentOrchestrator {
    pub agents: HashMap<String, AgentConfig>,
    pub tools: Arc<ToolRegistry>,
    pub llm: Arc<LlmClient>,
    pub shared_memory: Arc<Mutex<HashMap<String, Value>>>,
}

impl MultiAgentOrchestrator {
    pub fn new(
        tools: Arc<ToolRegistry>,
        model: &str,
        base_url: &str,
        api_key: Option<String>,
        allow_remote: bool,
    ) -> Self {
        let llm = Arc::new(LlmClient::new(base_url, model, allow_remote));

        let mut agents = HashMap::new();

        // Critic agent
        agents.insert("critic".into(), AgentConfig {
            name: "critic".into(),
            role: "Crítico de código experto".into(),
            system_prompt: "/no_think
Eres un crítico de código experto. Tu trabajo es evaluar salidas de otros agentes
contra criterios concretos. Sé estricto, objetivo y basado en evidencia.
NUNCA apruebes código que no cumpla los criterios.
Formato de respuesta obligatorio:
VEREDICTO: APROBADO | RECHAZADO | PARCIAL
PUNTUACION: <0.0-10.0>
FEEDBACK: <feedback concreto y accionable>".into(),
            temperature: 0.1,
            max_tokens: 2048,
        });

        // Reviewer agent
        agents.insert("reviewer".into(), AgentConfig {
            name: "reviewer".into(),
            role: "Revisor de código senior".into(),
            system_prompt: "/no_think
Eres un revisor de código senior. Encuentra problemas de:
- Seguridad (P0)
- Rendimiento (P1)
- Mantenibilidad (P2)
- Estilo y convenciones (P3)

Para cada hallazgo incluye:
- severity (P0/P1/P2/P3)
- línea aproximada
- problema
- solución sugerida

Si no hay problemas, responde exactamente: No issues found.".into(),
            temperature: 0.2,
            max_tokens: 4096,
        });

        // Planner agent
        agents.insert("planner".into(), AgentConfig {
            name: "planner".into(),
            role: "Planificador de tareas".into(),
            system_prompt: "/no_think
Eres un planificador de tareas para un agente de programación autónomo.
Descompone metas en subtareas pequeñas y verificables con criterios claros.
Devuelve SOLO una lista numerada:
1. <descripción> | Criterios: <c1>; <c2>".into(),
            temperature: 0.3,
            max_tokens: 2048,
        });

        // Executor agent
        agents.insert("executor".into(), AgentConfig {
            name: "executor".into(),
            role: "Ejecutor de tareas con herramientas".into(),
            system_prompt: "/no_think
Eres un ejecutor autónomo con acceso a herramientas.
Usa el método ReAct: Thought → Action → Observation.
Inspecciona archivos antes de editarlos.
Usa write_file con contenido COMPLETO.
Cuando termines, responde sin llamar a herramientas.".into(),
            temperature: 0.1,
            max_tokens: 4096,
        });

        // Coder agent
        agents.insert("coder".into(), AgentConfig {
            name: "coder".into(),
            role: "Generador de código experto".into(),
            system_prompt: "/no_think
Generas SOLO código válido y completo, sin explicaciones ni markdown.
Requisitos:
- Python 3.10+ válido y ejecutable
- Type hints en TODAS las funciones
- Google-style docstrings
- Error handling con try/except
- if __name__ == '__main__': block
- main() function as entry point
- ASCII only, no curly quotes".into(),
            temperature: 0.0,
            max_tokens: 4096,
        });

        // Architect agent
        agents.insert("architect".into(), AgentConfig {
            name: "architect".into(),
            role: "Arquitecto de software".into(),
            system_prompt: "/no_think
Eres un arquitecto de software. Diseñas estructuras de proyectos,
defines módulos, interfaces y patrones. Piensa en escalabilidad,
mantenibilidad y separación de concerns.".into(),
            temperature: 0.3,
            max_tokens: 4096,
        });

        Self {
            agents,
            tools,
            llm,
            shared_memory: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn critique(&self, output: &str, criteria: &[String], context: &str) -> Result<AgentResult> {
        let agent = self.agents.get("critic").unwrap();
        let criteria_str = criteria.join("; ");

        let prompt = format!(
            r#"{context}

Salida a evaluar:
---
{output}
---

Criterios de aceptación: {criteria_str}

Evalúa estrictamente. Responde EXACTAMENTE:
VEREDICTO: APROBADO | RECHAZADO | PARCIAL
PUNTUACION: <0.0-10.0>
FEEDBACK: <feedback concreto>"#
        );

        let messages = vec![
            Message { role: "system".into(), content: agent.system_prompt.clone() },
            Message { role: "user".into(), content: prompt },
        ];

        let response = self.llm.chat_messages(messages, None, false, false).await?;
        Ok(AgentResult {
            agent: "critic".into(),
            output: response.content.unwrap_or_default(),
            tool_calls: response.tool_calls.unwrap_or_default(),
            score: None,
        })
    }

    pub async fn review(&self, files: &[String], focus: &str) -> Result<Vec<AgentResult>> {
        let agent = self.agents.get("reviewer").unwrap();
        let mut results = Vec::new();

        for file in files {
            let content = std::fs::read_to_string(file).unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();

            for chunk_start in (0..lines.len()).step_by(120) {
                let chunk_end = (chunk_start + 120).min(lines.len());
                let chunk: Vec<&str> = lines[chunk_start..chunk_end].to_vec();
                let numbered = chunk.iter().enumerate()
                    .map(|(i, line)| format!("{:4}: {}", chunk_start + i + 1, line))
                    .collect::<Vec<_>>()
                    .join("\n");

                let prompt = format!(
                    r#"Eres un revisor de código senior.
Archivo: {}
Focus: {}

Retorna hallazgos concisos y accionables. Para cada uno incluye:
- severity (P0/P1/P2/P3)
- línea
- problema
- solución sugerida

Si no hay problemas, responde exactamente: No issues found.

Código:
{}"#,
                    file, focus, numbered
                );

                let messages = vec![
                    Message { role: "system".into(), content: agent.system_prompt.clone() },
                    Message { role: "user".into(), content: prompt },
                ];

                let response = self.llm.chat_messages(messages, None, false, false).await?;
                if !response.content.as_deref().unwrap_or("").starts_with("Error:") {
                    results.push(AgentResult {
                        agent: "reviewer".into(),
                        output: response.content.unwrap_or_default(),
                        tool_calls: response.tool_calls.unwrap_or_default(),
                        score: None,
                    });
                }
            }
        }

        Ok(results)
    }

    pub async fn plan(&self, goal: &str) -> Result<AgentResult> {
        let agent = self.agents.get("planner").unwrap();

        let messages = vec![
            Message { role: "system".into(), content: agent.system_prompt.clone() },
            Message { role: "user".into(), content: format!("Meta: {}", goal) },
        ];

        let response = self.llm.chat_messages(messages, None, false, false).await?;
        Ok(AgentResult {
            agent: "planner".into(),
            output: response.content.unwrap_or_default(),
            tool_calls: response.tool_calls.unwrap_or_default(),
            score: None,
        })
    }

    pub async fn execute(&self, task: &str, criteria: &[String]) -> Result<AgentResult> {
        let agent = self.agents.get("executor").unwrap();
        let criteria_str = criteria.join("; ");

        let messages = vec![
            Message { role: "system".into(), content: agent.system_prompt.clone() },
            Message {
                role: "user".into(),
                content: format!(
                    "Tarea: {}\nCriterios: {}\nResuelve usando ReAct. Cuando termines, responde sin herramientas.",
                    task, criteria_str
                ),
            },
        ];

        let tools_schema = self.tools.get_tools_schema();
        let response = self.llm.chat_messages(messages, Some(serde_json::to_value(&tools_schema)?), false, false).await?;
        Ok(AgentResult {
            agent: "executor".into(),
            output: response.content.unwrap_or_default(),
            tool_calls: response.tool_calls.unwrap_or_default(),
            score: None,
        })
    }

    pub async fn generate_code(&self, description: &str, filename: &str) -> Result<AgentResult> {
        let agent = self.agents.get("coder").unwrap();

        let prompt = format!(
            r#"Genera SOLO código Python válido y completo para: {}

Archivo: {}

Requisitos:
- Python 3.10+ ejecutable standalone
- Type hints en TODAS las funciones
- Google-style docstrings
- Error handling (try/except)
- if __name__ == '__main__': block
- main() function
- ASCII only, no curly quotes
- NO markdown, NO explicaciones"#,
            description, filename
        );

        let messages = vec![
            Message { role: "system".into(), content: agent.system_prompt.clone() },
            Message { role: "user".into(), content: prompt },
        ];

        let response = self.llm.chat_messages(messages, None, false, false).await?;
        Ok(AgentResult {
            agent: "coder".into(),
            output: response.content.unwrap_or_default(),
            tool_calls: response.tool_calls.unwrap_or_default(),
            score: None,
        })
    }

    pub async fn architect(&self, description: &str) -> Result<AgentResult> {
        let agent = self.agents.get("architect").unwrap();

        let prompt = format!(
            "Diseña la arquitectura para: {}\n\nProporciona:\n1. Estructura de directorios\n2. Módulos principales\n3. Interfaces clave\n4. Patrones recomendados\n5. Dependencias\n\nSé conciso y específico.",
            description
        );

        let messages = vec![
            Message { role: "system".into(), content: agent.system_prompt.clone() },
            Message { role: "user".into(), content: prompt },
        ];

        let response = self.llm.chat_messages(messages, None, false, false).await?;
        Ok(AgentResult {
            agent: "architect".into(),
            output: response.content.unwrap_or_default(),
            tool_calls: response.tool_calls.unwrap_or_default(),
            score: None,
        })
    }

    pub async fn store_memory(&self, key: &str, value: Value) -> Result<()> {
        let mut mem = self.shared_memory.lock().await;
        mem.insert(key.to_string(), value);
        Ok(())
    }

    pub async fn get_memory(&self, key: &str) -> Result<Option<Value>> {
        let mem = self.shared_memory.lock().await;
        Ok(mem.get(key).cloned())
    }

    pub async fn clear_memory(&self) -> Result<()> {
        let mut mem = self.shared_memory.lock().await;
        mem.clear();
        Ok(())
    }
}

impl Default for MultiAgentOrchestrator {
    fn default() -> Self {
        Self::new(
            Arc::new(ToolRegistry::new()),
            "qwen3-8b",
            "http://localhost:8080/v1",
            None,
            false,
        )
    }
}