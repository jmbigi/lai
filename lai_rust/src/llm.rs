//! llm.rs - HTTP client for llama.cpp / Ollama local endpoints.
//! Rule 4: only local (loopback/private) endpoints are allowed unless
//! --allow-remote is set.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Regla 4: comprueba si una URL apunta a un endpoint local.
///
/// Se considera local cualquier host que sea loopback, una dirección IP
/// privada, `localhost`, un nombre `.local`/`.localhost`, el alias del host de
/// Docker (`host.docker.internal`/`gateway.docker.internal`), o el comodín de
/// escucha `0.0.0.0`. Cualquier otra cosa (incluidos nombres DNS públicos)
/// requiere consentimiento explícito vía `--allow-remote`.
///
/// Los alias de Docker se aceptan porque resuelven a la propia máquina: son
/// la forma de llegar a un Ollama local desde dentro de un contenedor, no una
/// puerta a la nube.
pub fn is_local_url(url: &str) -> bool {
    let rest = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };

    // Autoridad = todo hasta el primer '/', '?' o '#'.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();

    // Quita userinfo (user:pass@host).
    let authority = match authority.rsplit_once('@') {
        Some((_, host)) => host,
        None => authority,
    };

    // IPv6 entre corchetes: [::1]:8080
    let host = if let Some(inner) = authority.strip_prefix('[') {
        match inner.split_once(']') {
            Some((h, _)) => h,
            None => inner,
        }
    } else {
        match authority.rsplit_once(':') {
            Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
            _ => authority,
        }
    };

    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }

    if host == "localhost" || host == "0.0.0.0" || host == "::1" || host == "::" {
        return true;
    }
    // Alias del host en Docker Desktop / Docker Engine: resuelven a la máquina
    // local, así que siguen siendo inferencia local (Regla 0/4).
    if host == "host.docker.internal" || host == "gateway.docker.internal" {
        return true;
    }
    if host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }

    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        Ok(std::net::IpAddr::V6(v6)) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // fc00::/7 (unique local) y fe80::/10 (link-local)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
        Err(_) => false,
    }
}

fn null_to_empty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message {
    pub role: String,
    // OpenAI/llama.cpp envían `content: null` cuando el mensaje solo lleva
    // tool_calls. Sin esto, la respuesta entera falla al deserializar.
    #[serde(default, deserialize_with = "null_to_empty_string")]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default = "default_call_type")]
    pub call_type: String,
    pub function: ToolFunction,
}

fn default_call_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    // Algunos servidores locales devuelven los argumentos como objeto JSON en
    // vez de como string. Normalizarlos aquí evita que falle toda la respuesta.
    #[serde(default, deserialize_with = "string_or_json_string")]
    pub arguments: String,
}

fn string_or_json_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::String(text) => text,
        serde_json::Value::Null => "{}".to_string(),
        other => other.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_kwargs: Option<HashMap<String, serde_json::Value>>,
    /// Reutiliza el KV cache de la petición anterior cuando el prefijo coincide.
    ///
    /// En un bucle de agente el prompt de sistema y las herramientas son un
    /// prefijo estable de miles de tokens; sin esto se reprocesan en cada
    /// turno. llama.cpp y Ollama ignoran el campo si no lo soportan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_prompt: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: serde_json::Value,
    /// Métricas de llama.cpp (`prompt_n`, `cache_n`, `predicted_per_second`).
    /// Opcional: otros servidores no lo devuelven.
    #[serde(default)]
    pub timings: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub message: Message,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Resultado de una llamada con tool calling: el mensaje y por qué terminó.
///
/// `finish_reason == "length"` significa que la respuesta se truncó al agotar
/// `max_tokens`; una tool call en ese estado puede estar incompleta y no debe
/// ejecutarse (lección del análisis de gramáticas de llama.cpp).
#[derive(Debug, Clone)]
pub struct ChatOutcome {
    pub message: Message,
    pub finish_reason: Option<String>,
}

impl ChatOutcome {
    /// ¿La respuesta se cortó por longitud y hay que pedir una más corta?
    pub fn truncated(&self) -> bool {
        self.finish_reason.as_deref() == Some("length")
    }
}

/// Tool schema for native function calling (OpenAI-compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    pub schema_type: String, // "function"
    pub function: FunctionSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

#[derive(Debug, Clone)]
pub enum ToolChoice {
    None,
    Auto,
    Required,
    Specific { function: ToolChoiceFunction },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolChoiceFunction {
    pub name: String,
}

// `#[serde(untagged)]` serializa las variantes unitarias como `null`, pero la
// API de OpenAI/llama.cpp espera las cadenas "none"/"auto"/"required" y, para
// una función concreta, {"type":"function","function":{"name":...}}. Un
// `tool_choice: null` funcionaba por accidente con algunos servidores, pero no
// es el contrato que anunciamos.
impl Serialize for ToolChoice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        match self {
            ToolChoice::None => serializer.serialize_str("none"),
            ToolChoice::Auto => serializer.serialize_str("auto"),
            ToolChoice::Required => serializer.serialize_str("required"),
            ToolChoice::Specific { function } => {
                let mut state = serializer.serialize_struct("ToolChoice", 2)?;
                state.serialize_field("type", "function")?;
                state.serialize_field("function", function)?;
                state.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ToolChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Named(String),
            Specific {
                #[serde(rename = "type")]
                kind: String,
                function: ToolChoiceFunction,
            },
        }

        match Repr::deserialize(deserializer)? {
            Repr::Named(name) => match name.as_str() {
                "none" => Ok(ToolChoice::None),
                "auto" => Ok(ToolChoice::Auto),
                "required" => Ok(ToolChoice::Required),
                other => Err(serde::de::Error::unknown_variant(
                    other,
                    &["none", "auto", "required", "function"],
                )),
            },
            Repr::Specific { kind, function } if kind == "function" => {
                Ok(ToolChoice::Specific { function })
            }
            Repr::Specific { .. } => Err(serde::de::Error::custom(
                "tool_choice type must be 'function'",
            )),
        }
    }
}

/// Native chat request with OpenAI-compatible function calling
#[derive(Debug, Clone, Serialize)]
pub struct NativeChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_kwargs: Option<HashMap<String, serde_json::Value>>,
    /// Reutiliza el KV cache de la petición anterior cuando el prefijo coincide.
    /// Ver `ChatRequest::cache_prompt`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_prompt: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    base_url: String,
    model: String,
    api_key: Option<String>,
    allow_remote: bool,
    /// Temperatura de muestreo. Los agentes de código necesitan determinismo:
    /// con 0.7 el mismo issue resolvía o no según la tirada (medido el
    /// 2026-11-27 en pytest-5809). `auto` usa 0.2 por defecto.
    temperature: f64,
    /// Compartido con la política de redirecciones de reqwest: una redirección
    /// también es una petición saliente y debe respetar la Regla 4.
    allow_remote_redirects: Arc<AtomicBool>,
    client: reqwest::Client,
}

impl LlmClient {
    pub fn new(base_url: String, model: String) -> Self {
        let allow_remote_redirects = Arc::new(AtomicBool::new(false));
        let redirect_flag = Arc::clone(&allow_remote_redirects);
        let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            if redirect_flag.load(Ordering::Relaxed) || is_local_url(attempt.url().as_str()) {
                attempt.follow()
            } else {
                attempt.error(
                    "Regla 4: redirección a un endpoint no local bloqueada; usa \
                     --allow-remote para permitirla explícitamente",
                )
            }
        });
        let client = reqwest::Client::builder()
            .redirect(redirect_policy)
            .build()
            .expect("failed to build HTTP client");

        Self {
            base_url,
            model,
            api_key: std::env::var("LAI_API_KEY")
                .ok()
                .filter(|key| !key.is_empty()),
            allow_remote: false,
            temperature: 0.7,
            allow_remote_redirects,
            client,
        }
    }

    /// Ajusta la temperatura de muestreo (0.0 = determinista).
    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    /// ¿Se pide reutilizar el KV cache entre peticiones?
    ///
    /// En un bucle de agente el prompt de sistema y los esquemas de herramientas
    /// son un prefijo estable de miles de tokens. `cache_prompt` hace que
    /// llama.cpp lo procese una vez y reutilice el KV cache en los turnos
    /// siguientes; sin él se recalcula en cada iteración. Los servidores que no
    /// lo soportan ignoran el campo. Se puede desactivar con
    /// `LAI_CACHE_PROMPT=0` (por si un backend rechaza campos desconocidos).
    fn cache_prompt(&self) -> bool {
        !matches!(
            std::env::var("LAI_CACHE_PROMPT")
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "0" | "false" | "no" | "off"
        )
    }

    /// Regla 4: permite endpoints no locales con consentimiento explícito.
    pub fn with_allow_remote(mut self, allow_remote: bool) -> Self {
        self.allow_remote = allow_remote;
        self.allow_remote_redirects
            .store(allow_remote, Ordering::Relaxed);
        self
    }

    /// Devuelve `Err` si la URL no es local y no se autorizó `--allow-remote`.
    pub fn check_endpoint(&self) -> Result<()> {
        if self.allow_remote || is_local_url(&self.base_url) {
            return Ok(());
        }
        anyhow::bail!(
            "Regla 4: el endpoint '{}' no es local. lai solo usa modelos locales \
             por defecto. Usa --allow-remote para aceptar explícitamente un \
             endpoint remoto.",
            self.base_url
        )
    }

    fn chat_request(&self) -> Result<reqwest::RequestBuilder> {
        self.check_endpoint()?;
        let request = self
            .client
            .post(format!("{}/chat/completions", self.base_url));
        Ok(match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
        })
    }

    pub async fn chat(&self, prompt: &str) -> Result<String> {
        let payload = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt.to_string(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }],
            tools: None,
            tool_choice: Some("none".to_string()),
            stream: Some(false),
            temperature: Some(self.temperature),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
            cache_prompt: Some(self.cache_prompt()),
        };

        let resp = self.chat_request()?.json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let data: ChatResponse = resp.json().await?;
        let choice = data
            .choices
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        Ok(choice.message.content)
    }

    /// Chat with simulated tool calling (JSON in code blocks) - legacy mode
    pub async fn chat_with_tools(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
        tool_choice: Option<String>,
    ) -> Result<Message> {
        let payload = ChatRequest {
            model: self.model.clone(),
            messages,
            tools,
            tool_choice,
            stream: Some(false),
            temperature: Some(self.temperature),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
            cache_prompt: Some(self.cache_prompt()),
        };

        let resp = self.chat_request()?.json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let data: ChatResponse = resp.json().await?;
        let choice = data
            .choices
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        Ok(choice.message)
    }

    /// Chat with native tool calling (llama.cpp native function calling API)
    /// Uses OpenAI-compatible function calling format with native tool call responses
    pub async fn chat_with_native_tools(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolSchema>>,
        tool_choice: Option<ToolChoice>,
    ) -> Result<ChatOutcome> {
        let payload = NativeChatRequest {
            model: self.model.clone(),
            messages,
            tools,
            tool_choice,
            stream: Some(false),
            temperature: Some(self.temperature),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
            cache_prompt: Some(self.cache_prompt()),
        };

        let resp = self.chat_request()?.json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let data: ChatResponse = resp.json().await?;
        if let Some(timings) = &data.timings {
            log_timings(timings);
        }
        let choice = data
            .choices
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

        Ok(ChatOutcome {
            message: choice.message,
            finish_reason: choice.finish_reason,
        })
    }

    /// Streaming chat with native tool calling
    pub async fn chat_with_native_tools_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolSchema>>,
        tool_choice: Option<ToolChoice>,
    ) -> Result<reqwest::Response> {
        let payload = NativeChatRequest {
            model: self.model.clone(),
            messages,
            tools,
            tool_choice,
            stream: Some(true),
            temperature: Some(self.temperature),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
            cache_prompt: Some(self.cache_prompt()),
        };

        let resp = self.chat_request()?.json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        Ok(resp)
    }
}

/// Imprime métricas de llama.cpp cuando `LAI_DEBUG_TIMINGS=1`.
///
/// Permite medir el efecto de `cache_prompt` (Regla 11: una variable a la vez):
/// `cache_n` son los tokens de prompt reutilizados del KV cache y `prompt_n`
/// los realmente procesados.
fn log_timings(timings: &serde_json::Value) {
    let enabled = matches!(
        std::env::var("LAI_DEBUG_TIMINGS")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    );
    if !enabled {
        return;
    }
    let get = |key: &str| timings.get(key).cloned().unwrap_or(serde_json::Value::Null);
    eprintln!(
        "  ⏱ prompt_n={} cache_n={} predicted_n={} gen_t/s={}",
        get("prompt_n"),
        get("cache_n"),
        get("predicted_n"),
        get("predicted_per_second"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_loopback_endpoints() {
        for url in [
            "http://127.0.0.1:11434/v1",
            "http://127.0.0.1:8080",
            "http://localhost:11434/v1",
            "http://localhost",
            "http://[::1]:11434/v1",
            "http://0.0.0.0:8082/v1",
            "https://LOCALHOST:1234/v1",
            "http://localhost.:11434/v1",
        ] {
            assert!(is_local_url(url), "deberia ser local: {url}");
        }
    }

    #[test]
    fn accepts_docker_host_aliases() {
        for url in [
            "http://host.docker.internal:11434/v1",
            "http://gateway.docker.internal:11434/v1",
            "http://HOST.DOCKER.INTERNAL:11434/v1",
        ] {
            assert!(is_local_url(url), "deberia ser local: {url}");
        }
    }

    #[test]
    fn accepts_private_ranges() {
        for url in [
            "http://10.0.0.5:11434/v1",
            "http://192.168.1.10:8080",
            "http://172.16.4.2:11434",
            "http://172.31.255.1:11434",
            "http://169.254.10.1:11434",
            "http://[fd00::1]:11434",
            "http://[fe80::1]:11434",
            "http://mi-servidor.local:11434/v1",
        ] {
            assert!(is_local_url(url), "deberia ser local: {url}");
        }
    }

    #[test]
    fn rejects_public_endpoints() {
        for url in [
            "https://api.openai.com/v1",
            "https://api.anthropic.com",
            "http://8.8.8.8:11434/v1",
            "http://172.32.0.1:11434",
            "http://example.com/v1",
            "http://[2001:4860:4860::8888]:443",
            "",
        ] {
            assert!(!is_local_url(url), "no deberia ser local: {url}");
        }
    }

    #[test]
    fn strips_userinfo_and_query() {
        assert!(is_local_url("http://user:pass@127.0.0.1:11434/v1"));
        assert!(!is_local_url("http://user:pass@evil.example.com/v1"));
        assert!(is_local_url("http://localhost:11434/v1?x=1"));
    }

    #[test]
    fn tool_call_accepts_object_arguments_and_missing_fields() {
        let call: ToolCall = serde_json::from_str(
            r#"{"function":{"name":"read_file","arguments":{"path":"a.rs"}}}"#,
        )
        .unwrap();
        assert_eq!(call.id, "");
        assert_eq!(call.call_type, "function");
        let args: serde_json::Value = serde_json::from_str(&call.function.arguments).unwrap();
        assert_eq!(args["path"], "a.rs");
    }

    #[test]
    fn null_message_content_deserializes_as_empty() {
        let message: Message =
            serde_json::from_str(r#"{"role":"assistant","content":null}"#).unwrap();
        assert_eq!(message.content, "");

        let message: Message = serde_json::from_str(r#"{"role":"assistant"}"#).unwrap();
        assert_eq!(message.content, "");
    }

    #[test]
    fn tool_choice_serializes_to_openai_strings() {
        assert_eq!(
            serde_json::to_string(&ToolChoice::None).unwrap(),
            "\"none\""
        );
        assert_eq!(
            serde_json::to_string(&ToolChoice::Auto).unwrap(),
            "\"auto\""
        );
        assert_eq!(
            serde_json::to_string(&ToolChoice::Required).unwrap(),
            "\"required\""
        );
    }

    #[test]
    fn check_endpoint_enforces_rule_4() {
        let remote = LlmClient::new("https://api.example.com/v1".into(), "m".into());
        assert!(remote.check_endpoint().is_err());

        let remote_allowed = remote.with_allow_remote(true);
        assert!(remote_allowed.check_endpoint().is_ok());

        let local = LlmClient::new("http://127.0.0.1:11434/v1".into(), "m".into());
        assert!(local.check_endpoint().is_ok());
    }

    #[test]
    fn chat_outcome_detects_truncation() {
        let message = Message {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        };
        let truncated = ChatOutcome {
            message: message.clone(),
            finish_reason: Some("length".into()),
        };
        assert!(truncated.truncated());
        let stopped = ChatOutcome {
            message,
            finish_reason: Some("stop".into()),
        };
        assert!(!stopped.truncated());
        let unknown = ChatOutcome {
            message: Message {
                role: "assistant".into(),
                content: String::new(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            finish_reason: None,
        };
        assert!(!unknown.truncated());
    }

    #[test]
    fn native_request_serializes_cache_prompt() {
        let payload = NativeChatRequest {
            model: "m".into(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: Some(false),
            temperature: Some(0.2),
            max_tokens: Some(64),
            top_p: None,
            top_k: None,
            chat_template_kwargs: None,
            cache_prompt: Some(true),
        };
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["cache_prompt"], true);

        let without = NativeChatRequest {
            model: "m".into(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: Some(false),
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            chat_template_kwargs: None,
            cache_prompt: None,
        };
        let json = serde_json::to_value(&without).unwrap();
        assert!(
            json.get("cache_prompt").is_none(),
            "no debe enviarse si es None"
        );
    }

    #[test]
    fn cache_prompt_env_parsing_is_pure() {
        for value in ["0", "false", "NO", "off"] {
            assert!(matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            ));
        }
        for value in ["1", "true", "", "on"] {
            assert!(!matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            ));
        }
    }
}
