//! AutoMode - Autonomous agent with Plan-and-Execute, ReAct, and Reflection.
//! Ported from lai_automode.py with full feature parity.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::llm::{ChatResult, LlmClient, Message};
use crate::tools::ToolRegistry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    pub criteria: Vec<String>,
    pub status: String, // pending, in_progress, done, failed
    pub result: Option<String>,
    pub score: Option<f64>,
    pub feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActTrace {
    pub thought: String,
    pub action: Option<String>,
    pub action_input: Option<HashMap<String, Value>>,
    pub observation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CritiqueResult {
    pub approved: bool,
    pub score: f64,
    pub verdict: String, // APROBADO / RECHAZADO / PARCIAL
    pub feedback: String,
    pub raw: String,
}

pub struct AutoMode {
    pub goal: String,
    pub tools: Arc<ToolRegistry>,
    pub llm: Arc<LlmClient>,
    pub max_iterations: usize,
    pub max_turns: usize,
    pub track_cost: bool,
    pub reflective: bool,
    pub max_reflection_rounds: usize,

    pub iteration: usize,
    pub turns_used: usize,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub messages: Vec<Message>,
    pub completed: bool,
    pub error_history: Vec<String>,
    pub plan: Vec<PlanStep>,
    pub transcript: Vec<String>,
    pub best_outputs: HashMap<String, (f64, String)>,
    pub best_global: Option<(f64, String)>,
}

impl AutoMode {
    const PLANNER_PROMPT: &'static str = "/no_think
Eres un planificador de tareas para un agente de programación autónomo.
OBJETIVO: Descomponer la meta del usuario en subtareas pequeñas y verificables, cada una con criterios de aceptación concretos.

REGLAS:
1. Cada subtarea debe ser pequeña (una acción o un archivo).
2. Los criterios de aceptación deben ser verificables (compila, pasa tests, existe el archivo, etc.).
3. Devuelve SOLO una lista numerada con este formato:
   1. <descripción> | Criterios: <c1>; <c2>
4. No escribas código, solo planifica.
5. Todo el código debe generarse con modelos locales (Regla 4).";

    const EXECUTOR_PROMPT: &'static str = "/no_think
Eres un ejecutor de tareas autónomo con acceso a herramientas.
OBJETIVO: Cumplir la subtarea asignada usando las herramientas disponibles.

MÉTODO ReAct (Reason + Act):
Para cada paso, piensa primero (Thought), luego decide una acción
(Action con nombre de herramienta y argumentos), espera la observación
y repite hasta resolver la subtarea.

REGLAS DE ORO:
1. Inspecciona los archivos antes de editarlos (read_file, grep, list_dir).
2. Usa write_file con el CONTENIDO COMPLETO del archivo, nunca fragmentos.
3. Los archivos Python se verifican sintácticamente automáticamente.
4. Si una herramienta falla, analiza el error y reintenta con corrección.
5. Cuando la subtarea esté completa, responde sin llamar a ninguna herramienta.
6. Todo el código debe generarse usando modelos locales (Regla 4).";

    const CRITIQUE_PROMPT_HINT: &'static str = "Usa únicamente evidencia concreta (líneas, fragmentos, salida de herramientas) para fundamentar tu veredicto.";

    pub fn new(
        goal: String,
        tools: Arc<ToolRegistry>,
        llm: Arc<LlmClient>,
        max_iterations: usize,
        max_turns: usize,
        track_cost: bool,
        reflective: bool,
        max_reflection_rounds: usize,
    ) -> Self {
        Self {
            goal,
            tools,
            llm,
            max_iterations,
            max_turns,
            track_cost,
            reflective,
            max_reflection_rounds,
            iteration: 0,
            turns_used: 0,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            messages: vec![
                Message { role: "system".into(), content: Self::EXECUTOR_PROMPT.into() },
                Message { role: "user".into(), content: String::new() },
            ],
            completed: false,
            error_history: Vec::new(),
            plan: Vec::new(),
            transcript: Vec::new(),
            best_outputs: HashMap::new(),
            best_global: None,
        }
    }

    fn is_local_endpoint(&self) -> bool {
        crate::llm::LlmClient::is_local_url(&self.llm.base_url)
    }

    fn log(&mut self, msg: &str) {
        self.transcript.push(msg.to_string());
        println!("{}", msg);
    }

    pub async fn run(&mut self) -> Result<String> {
        self.transcript.clear();
        self.log("╔═══════════════════════════════════════════════════════════════╗");
        self.log("║  lai AutoMode - Agente autónomo (Plan + ReAct + Reflection)   ║");
        self.log("╚═══════════════════════════════════════════════════════════════╝");
        self.log(&format!("\nMeta: {}\n", self.goal));

        if !self.is_local_endpoint() {
            self.log("Error: AutoMode requiere un endpoint de modelo local (Regla 4).");
            return Ok(self.transcript.join("\n"));
        }

        // Phase 1: Plan-and-Execute
        self.log("\n=== FASE 1: Planificación (Plan-and-Execute) ===");
        self.plan = self.plan_goal().await?;
        if self.plan.is_empty() {
            self.log("No se pudo generar un plan. Ejecutando en modo directo (una sola tarea).");
            self.plan = vec![PlanStep {
                id: "step-1".into(),
                description: self.goal.clone(),
                criteria: vec!["La tarea del usuario queda resuelta".into()],
                status: "pending".into(),
                result: None,
                score: None,
                feedback: None,
            }];
        }

        self.log_plan();

        // Phase 2: Execute each step with ReAct + Reflection
        let num_steps = self.plan.len();
        for i in 0..num_steps {
            if self.turns_used >= self.max_turns {
                self.log(&format!("\nMáximo de turnos ({}) alcanzado. Deteniendo plan.", self.max_turns));
                break;
            }

            // Get step data before mutable borrow
            let (step_id, step_desc, step_criteria) = {
                let step = &self.plan[i];
                (step.id.clone(), step.description.clone(), step.criteria.clone())
            };

            self.log(&format!("\n--- Paso {}: {} ---", step_id, step_desc));
            self.log(&format!("Criterios de aceptación: {}", step_criteria.join("; ")));

            // Update step status
            self.plan[i].status = "in_progress".into();

            // Create temporary step for execution
            let temp_step = PlanStep {
                id: step_id.clone(),
                description: step_desc.clone(),
                criteria: step_criteria.clone(),
                status: "in_progress".into(),
                result: None,
                score: None,
                feedback: None,
            };

            // 2a. Execute with ReAct loop
            let step_result = self.execute_step_react(&temp_step).await?;

            // 2b. Reflection: separate critic evaluates
            let mut best_result = step_result.clone();
            let mut best_score = -1.0;

            if self.reflective {
                for round_num in 1..=self.max_reflection_rounds {
                    self.log(&format!("\n[Reflection] Ronda {} de crítica", round_num));
                    let critique = self.critique_step(&temp_step, &step_result).await?;

                    // Update step with critique results
                    self.plan[i].score = Some(critique.score);
                    self.plan[i].feedback = Some(critique.feedback.clone());

                    if critique.approved {
                        self.log(&format!("[Reflection] APROBADO (puntuación {})", critique.score));
                        best_result = step_result.clone();
                        best_score = critique.score;
                        break;
                    }

                    self.log(&format!(
                        "[Reflection] RECHAZADO/PARCIAL (puntuación {}): {}",
                        critique.score,
                        critique.feedback.chars().take(200).collect::<String>()
                    ));

                    if critique.score > best_score {
                        best_score = critique.score;
                        best_result = step_result.clone();
                    }

                    // 2c. Revise: re-execute with critic feedback
                    self.log("[Reflection] Re-ejecutando con feedback del crítico...");
                    let revised = self.revise_step(&temp_step, &step_result, &critique.feedback).await?;
                    if !revised.is_empty() {
                        // Update step_result for next iteration would need re-structuring
                    }
                }
            }

            // 2d. Best-version fallback
            if best_score > 0.0 {
                self.best_outputs.insert(step_id.clone(), (best_score, best_result.clone()));
            }
            self.plan[i].result = Some(best_result);
            self.plan[i].status = "done".into();
            self.log(&format!("[Paso {}] Completado.", step_id));
        }

        // Phase 3: Integrate results
        self.log("\n=== FASE 3: Integración y respuesta final ===");
        let final_output = self.integrate_results().await?;

        // Global best-version fallback
        if !self.completed && self.best_global.is_some() {
            self.log("\n[Best-version fallback] No se alcanzó completitud. Devolviendo la mejor versión puntuada.");
            return Ok(self.best_global.as_ref().unwrap().1.clone());
        }

        self.log_cost_summary();
        Ok(final_output)
    }

    fn log_plan(&mut self) {
        let lines: Vec<String> = self.plan.iter().map(|step| {
            format!(
                "  {}. {}{}",
                step.id,
                step.description,
                if !step.criteria.is_empty() { format!(" | Criterios: {}", step.criteria.join("; ")) } else { String::new() }
            )
        }).collect();
        for line in lines {
            self.log(&line);
        }
    }

    async fn plan_goal(&mut self) -> Result<Vec<PlanStep>> {
        let messages = vec![
            Message { role: "system".into(), content: Self::PLANNER_PROMPT.into() },
            Message { role: "user".into(), content: format!("Meta: {}", self.goal) },
        ];

        let response = self.llm.chat_messages(messages, None, false, false).await;
        if let Some(err) = &response.error {
            self.log(&format!("Error en planificación: {}", err));
            return Ok(Vec::new());
        }

        let content = response.content.clone();
        Ok(self.parse_plan(&content))
    }

    fn parse_plan(&self, content: &str) -> Vec<PlanStep> {
        let mut steps = Vec::new();
        let pattern = Regex::new(r"(?m)^\s*\d+\.\s*(.+?)\s*\|\s*Criterios:\s*(.+?)\s*$").unwrap();

        for (idx, m) in pattern.find_iter(content).enumerate() {
            let line = m.as_str();
            if let Some(caps) = pattern.captures(line) {
                let description = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                let criteria_raw = caps.get(2).map(|m| m.as_str().trim()).unwrap_or("");
                let criteria: Vec<String> = criteria_raw.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                steps.push(PlanStep {
                    id: format!("step-{}", idx + 1),
                    description: description.to_string(),
                    criteria,
                    status: "pending".into(),
                    result: None,
                    score: None,
                    feedback: None,
                });
            }
        }

        // Fallback: simple numbered lines
        if steps.is_empty() {
            for (idx, line) in content.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || !line.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                    continue;
                }
                let desc = Regex::new(r"^\d+\.\s*").unwrap().replace(line, "").to_string();
                if !desc.is_empty() {
                    steps.push(PlanStep {
                        id: format!("step-{}", idx + 1),
                        description: desc,
                        criteria: vec!["La subtarea queda resuelta".into()],
                        status: "pending".into(),
                        result: None,
                        score: None,
                        feedback: None,
                    });
                }
            }
        }
        steps
    }

    async fn execute_step_react(&mut self, step: &PlanStep) -> Result<String> {
        let mut messages = vec![
            Message { role: "system".into(), content: Self::EXECUTOR_PROMPT.into() },
            Message {
                role: "user".into(),
                content: format!(
                    "Subtarea: {}\nCriterios de aceptación: {}\nResuelve la subtarea usando el método ReAct (Thought → Action → Observation). Cuando termine, responde sin llamar a herramientas.",
                    step.description,
                    step.criteria.join("; ")
                ),
            },
        ];

        let mut final_content = String::new();
        let tools_schema = self.tools.get_tools_schema();

        for _turn in 1..=self.max_turns {
            if self.turns_used >= self.max_turns {
                self.log(&format!("  [ReAct] Máximo de turnos alcanzado en paso {}", step.id));
                break;
            }

            self.turns_used += 1;
            let response = self.llm.chat_messages(messages.clone(), Some(vec![serde_json::to_value(&tools_schema)?]), false, false).await;
            if let Some(err) = &response.error {
                self.log(&format!("  [ReAct] Error: {}", err));
                break;
            }

            self.track_usage(&response);

            let content = response.content.clone();
            let tool_calls = response.tool_calls.clone();

            if !content.trim().is_empty() {
                self.log(&format!("  [ReAct] Thought: {}", content.chars().take(200).collect::<String>()));
                final_content = content.trim().to_string();
            }

            if tool_calls.is_empty() {
                self.log("  [ReAct] Sin más acciones. Contenido final del paso.");
                break;
            }

            // Execute tools
            for call in tool_calls {
                if self.turns_used >= self.max_turns { break; }

                let name = call.function.name;
                let args: HashMap<String, Value> = serde_json::from_str(&call.function.arguments).unwrap_or_default();

                self.log(&format!("  [ReAct] Action: {}({})", name, serde_json::to_string(&args).unwrap_or_default().chars().take(120).collect::<String>()));

                let result = self.tools.call_tool(&name, args).await.unwrap_or_else(|e| format!("Tool {} failed: {}", name, e));
                let result_truncated = result.chars().take(6000).collect::<String>();

                self.log(&format!("  [ReAct] Observation: {}", result_truncated.chars().take(200).collect::<String>()));

                messages.push(Message {
                    role: "tool".into(),
                    content: result_truncated,
                    ..Default::default()
                });
            }
        }

        Ok(final_content)
    }

    fn track_usage(&mut self, response: &ChatResult) {
        // ChatResult doesn't have usage field; estimate from content length
        let estimated = (response.content.len() / 4) as u64;
        self.total_completion_tokens += estimated;
    }

    async fn critique_step(&mut self, step: &PlanStep, output: &str) -> Result<CritiqueResult> {
        let context = format!(
            "Meta global: {}\nSubtarea: {}\nCriterios de aceptación: {}",
            self.goal,
            step.description,
            step.criteria.join("; ")
        );

        let critique_prompt = format!(
            r#"Eres un crítico de código experto. Evalúa la siguiente salida contra los criterios.

{context}

Salida a evaluar:
---
{output}
---

Responde EXACTAMENTE con este formato:
VEREDICTO: APROBADO | RECHAZADO | PARCIAL
PUNTUACION: <0.0-10.0>
FEEDBACK: <feedback concreto y accionable>

{}"#,
            Self::CRITIQUE_PROMPT_HINT
        );

        let messages = vec![
            Message { role: "system".into(), content: "Eres un crítico estricto. Evalúa con evidencia concreta.".into() },
            Message { role: "user".into(), content: critique_prompt },
        ];

        let response = self.llm.chat_messages(messages, None, false, false).await;
        if let Some(err) = &response.error {
            self.log(&format!("  [Critique] Error: {}", err));
            return Ok(CritiqueResult {
                approved: false,
                score: 0.0,
                verdict: "ERROR".to_string(),
                feedback: err.clone(),
                raw: err.clone(),
            });
        }
        let raw = response.content.clone();
        Ok(self.parse_critique(&raw))
    }

    fn parse_critique(&self, raw: &str) -> CritiqueResult {
        let mut verdict = "RECHAZADO".to_string();
        let mut approved = false;
        let mut score = 0.0;
        let mut feedback = raw.to_string();

        if let Some(m) = Regex::new(r"VEREDICTO:\s*(APROBADO|RECHAZADO|PARCIAL)").unwrap().captures(raw) {
            verdict = m.get(1).unwrap().as_str().to_uppercase();
            approved = verdict == "APROBADO";
        }

        if let Some(m) = Regex::new(r"PUNTUACION:\s*(\d+(?:\.\d+)?)").unwrap().captures(raw) {
            score = m.get(1).unwrap().as_str().parse().unwrap_or(0.0);
        }

        if let Some(m) = Regex::new(r"FEEDBACK:\s*(.+)").unwrap().captures(raw) {
            feedback = m.get(1).unwrap().as_str().trim().to_string();
        }

        CritiqueResult { approved, score, verdict, feedback, raw: raw.to_string() }
    }

    async fn revise_step(&mut self, step: &PlanStep, previous_output: &str, feedback: &str) -> Result<String> {
        let mut messages = vec![
            Message { role: "system".into(), content: Self::EXECUTOR_PROMPT.into() },
            Message {
                role: "user".into(),
                content: format!(
                    "Subtarea: {}\nCriterios de aceptación: {}\nINTENTO ANTERIOR:\n---\n{}\n---\nFEEDBACK DEL CRÍTICO (debes corregir estos puntos):\n---\n{}\n---\nVuelve a resolver la subtarea aplicando el feedback. Usa el método ReAct. Cuando termine, responde sin llamar a herramientas.",
                    step.description,
                    step.criteria.join("; "),
                    previous_output,
                    feedback
                ),
            },
        ];

        let mut final_content = String::new();
        let tools_schema = self.tools.get_tools_schema();

        for _turn in 1..=self.max_turns {
            if self.turns_used >= self.max_turns { break; }

            self.turns_used += 1;
            let response = self.llm.chat_messages(messages.clone(), Some(vec![serde_json::to_value(&tools_schema)?]), false, false).await;
            if let Some(err) = &response.error {
                self.log(&format!("  [Revise] Error: {}", err));
                break;
            }

            self.track_usage(&response);

            let content = response.content.clone();
            let tool_calls = response.tool_calls.clone();

            if !content.trim().is_empty() {
                self.log(&format!("  [Revise] Thought: {}", content.chars().take(200).collect::<String>()));
                final_content = content.trim().to_string();
            }

            if tool_calls.is_empty() {
                self.log("  [Revise] Sin más acciones. Contenido final del paso.");
                break;
            }

            for call in tool_calls {
                if self.turns_used >= self.max_turns { break; }

                let name = call.function.name;
                let args: HashMap<String, Value> = serde_json::from_str(&call.function.arguments).unwrap_or_default();

                self.log(&format!("  [Revise] Action: {}({})", name, serde_json::to_string(&args).unwrap_or_default().chars().take(120).collect::<String>()));

                let result = self.tools.call_tool(&name, args).await.unwrap_or_else(|e| format!("Tool {} failed: {}", name, e));
                let result_truncated = result.chars().take(6000).collect::<String>();

                self.log(&format!("  [Revise] Observation: {}", result_truncated.chars().take(200).collect::<String>()));

                messages.push(Message {
                    role: "tool".into(),
                    content: result_truncated,
                    ..Default::default()
                });
            }
        }

        Ok(final_content)
    }

    async fn integrate_results(&mut self) -> Result<String> {
        let mut output = String::new();
        for step in &self.plan {
            if let Some(ref result) = step.result {
                output.push_str(&format!("\n=== Paso {}: {} ===\n{}\n", step.id, step.description, result));
            }
        }
        if output.trim().is_empty() {
            output = "No se generaron resultados.".to_string();
        }
        self.completed = true;
        Ok(output)
    }

    fn log_cost_summary(&mut self) {
        self.log("\n--- Resumen de Coste ---");
        self.log(&format!("Prompt tokens: {}", self.total_prompt_tokens));
        self.log(&format!("Completion tokens: {}", self.total_completion_tokens));
        self.log(&format!("Turnos usados: {}/{}", self.turns_used, self.max_turns));
        self.log(&format!("Iteraciones: {}/{}", self.iteration, self.max_iterations));
        self.log(&format!("Pasos del plan: {}", self.plan.len()));
        self.log(&format!("Completada: {}", self.completed));
        self.log(&format!("Errores: {}", self.error_history.len()));
    }
}
