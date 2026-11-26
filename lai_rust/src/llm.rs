//! llm.rs - HTTP client for llama.cpp / Ollama local endpoints.
//! Rule 4: only local (loopback/private) endpoints are allowed unless
//! --allow-remote is set.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message {
    pub role: String,
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
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub message: Message,
    #[serde(default)]
    pub finish_reason: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
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
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    base_url: String,
    model: String,
    api_key: Option<String>,
    allow_remote: bool,
    client: reqwest::Client,
}

impl LlmClient {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            base_url,
            model,
            api_key: std::env::var("LAI_API_KEY")
                .ok()
                .filter(|key| !key.is_empty()),
            allow_remote: false,
            client: reqwest::Client::new(),
        }
    }

    /// Regla 4: permite endpoints no locales con consentimiento explícito.
    pub fn with_allow_remote(mut self, allow_remote: bool) -> Self {
        self.allow_remote = allow_remote;
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
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
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
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
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
    ) -> Result<Message> {
        let payload = NativeChatRequest {
            model: self.model.clone(),
            messages,
            tools,
            tool_choice,
            stream: Some(false),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
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
            temperature: Some(0.7),
            max_tokens: Some(4096),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(false),
            )])),
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
    fn check_endpoint_enforces_rule_4() {
        let remote = LlmClient::new("https://api.example.com/v1".into(), "m".into());
        assert!(remote.check_endpoint().is_err());

        let remote_allowed = remote.with_allow_remote(true);
        assert!(remote_allowed.check_endpoint().is_ok());

        let local = LlmClient::new("http://127.0.0.1:11434/v1".into(), "m".into());
        assert!(local.check_endpoint().is_ok());
    }
}
