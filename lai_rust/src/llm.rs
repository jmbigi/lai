//! LLM client: HTTP client for llama.cpp / Ollama local endpoints.
//! Rule 4: only local (loopback/private) endpoints are allowed unless
//! --allow-remote is set.

use futures::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::net::IpAddr;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Delta {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    #[serde(default)]
    pub delta: Delta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    pub choices: Vec<StreamChoice>,
}

/// Result of a chat call.
#[derive(Debug, Clone)]
pub struct ChatResult {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub error: Option<String>,
}

/// LLM client that talks to a local OpenAI-compatible endpoint.
pub struct LlmClient {
    pub base_url: String,
    pub model: String,
    pub client: reqwest::Client,
    pub allow_remote: bool,
    pub temperature: f64,
    pub max_tokens: i32,
    pub thinking: bool,
}

impl LlmClient {
    pub fn new(base_url: &str, model: &str, allow_remote: bool) -> Self {
        Self {
            base_url: base_url.to_string(),
            model: model.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            allow_remote,
            temperature: 0.7,
            max_tokens: 4096,
            thinking: true,
        }
    }

    /// Rule 4 guard: only local endpoints allowed unless --allow-remote.
    pub fn is_local_url(url: &str) -> bool {
        let host = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .unwrap_or(url)
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .to_lowercase();

        let local_hosts = ["localhost", "127.0.0.1", "::1", "0.0.0.0"];
        if local_hosts.contains(&host.as_str()) || host.ends_with(".local") {
            return true;
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            match ip {
                IpAddr::V4(v4) => return v4.is_loopback() || v4.is_private(),
                IpAddr::V6(v6) => return v6.is_loopback() || v6.is_unicast_link_local(),
            }
        }
        false
    }

    pub async fn chat(&self, prompt: &str, stream: bool, thinking: bool) -> ChatResult {
        if !Self::is_local_url(&self.base_url) && !self.allow_remote {
            return ChatResult {
                content: format!(
                    "Error: LAI requires a local model endpoint (Rule 4). Refusing non-local base_url: {}",
                    self.base_url
                ),
                tool_calls: vec![],
                finish_reason: None,
                error: Some("non-local endpoint".to_string()),
            };
        }

        let mut full_prompt = prompt.to_string();
        if thinking {
            full_prompt = format!(
                "<thinking>\n{}\n</thinking>\n\nProvide the solution.",
                prompt
            );
        }

        let payload = ChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".into(),
                content: full_prompt,
            }],
            tools: None,
            tool_choice: None,
            stream: Some(stream),
            temperature: Some(self.temperature),
            max_tokens: Some(self.max_tokens),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(thinking),
            )])),
        };

        let url = format!("{}/chat/completions", self.base_url);

        if stream {
            return self.chat_stream(&url, &payload).await;
        }

        match self.client.post(&url).json(&payload).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return ChatResult {
                        content: format!("Error: HTTP {} {}", status, text),
                        tool_calls: vec![],
                        finish_reason: None,
                        error: Some(format!("HTTP {}", status)),
                    };
                }
                match resp.json::<ChatResponse>().await {
                    Ok(data) => {
                        let choice = data.choices.first().cloned();
                        match choice {
                            Some(c) => ChatResult {
                                content: c.message.content,
                                tool_calls: vec![],
                                finish_reason: c.finish_reason,
                                error: None,
                            },
                            None => ChatResult {
                                content: "".into(),
                                tool_calls: vec![],
                                finish_reason: None,
                                error: Some("no choices in response".to_string()),
                            },
                        }
                    }
                    Err(e) => ChatResult {
                        content: format!("Error parsing response: {}", e),
                        tool_calls: vec![],
                        finish_reason: None,
                        error: Some(e.to_string()),
                    },
                }
            }
            Err(e) => ChatResult {
                content: format!("Error: {}", e),
                tool_calls: vec![],
                finish_reason: None,
                error: Some(e.to_string()),
            },
        }
    }

    async fn chat_stream(&self, url: &str, payload: &ChatRequest) -> ChatResult {
        match self.client.post(url).json(payload).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return ChatResult {
                        content: format!("Error: HTTP {} {}", status, text),
                        tool_calls: vec![],
                        finish_reason: None,
                        error: Some(format!("HTTP {}", status)),
                    };
                }

                let mut stream = resp.bytes_stream();
                let mut content_buffer = String::new();

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            for line in text.lines() {
                                let line = line.trim();
                                if line.starts_with("data: ") {
                                    let data = &line[6..];
                                    if data == "[DONE]" {
                                        break;
                                    }
                                    if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                                        if let Some(choice) = chunk.choices.first() {
                                            if let Some(content) = &choice.delta.content {
                                                print!("{}", content);
                                                let _ = std::io::stdout().flush();
                                                content_buffer.push_str(content);
                                            }
                                        }
                                        if chunk
                                            .choices
                                            .first()
                                            .and_then(|c| c.finish_reason.as_deref())
                                            .is_some()
                                        {
                                            println!();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            return ChatResult {
                                content: content_buffer,
                                tool_calls: vec![],
                                finish_reason: None,
                                error: Some(e.to_string()),
                            };
                        }
                    }
                }

                ChatResult {
                    content: content_buffer,
                    tool_calls: vec![],
                    finish_reason: Some("stop".to_string()),
                    error: None,
                }
            }
            Err(e) => ChatResult {
                content: format!("Error: {}", e),
                tool_calls: vec![],
                finish_reason: None,
                error: Some(e.to_string()),
            },
        }
    }

    /// Send a full message list (used by agent loop). Non-streaming.
    pub async fn chat_messages(
        &self,
        messages: Vec<Message>,
        _tools: Option<Vec<serde_json::Value>>,
        stream: bool,
        thinking: bool,
    ) -> ChatResult {
        if !Self::is_local_url(&self.base_url) && !self.allow_remote {
            return ChatResult {
                content: "".into(),
                tool_calls: vec![],
                finish_reason: None,
                error: Some(format!(
                    "Rule 4: local endpoint required, got {}",
                    self.base_url
                )),
            };
        }

        let mut msgs = messages.clone();
        if thinking {
            if let Some(first) = msgs.first_mut() {
                first.content = format!("<thinking>\n{}\n</thinking>", first.content);
            }
        }

        let payload = ChatRequest {
            model: self.model.clone(),
            messages: msgs,
            tools: None,
            tool_choice: Some("auto".to_string()),
            stream: Some(stream),
            temperature: Some(self.temperature),
            max_tokens: Some(2048),
            top_p: Some(0.95),
            top_k: Some(20),
            chat_template_kwargs: Some(HashMap::from([(
                "enable_thinking".into(),
                serde_json::Value::Bool(thinking),
            )])),
        };

        let url = format!("{}/chat/completions", self.base_url);

        if stream {
            return self.chat_stream(&url, &payload).await;
        }

        match self.client.post(&url).json(&payload).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return ChatResult {
                        content: format!("Error: HTTP {} {}", status, text),
                        tool_calls: vec![],
                        finish_reason: None,
                        error: Some(format!("HTTP {}", status)),
                    };
                }
                match resp.json::<ChatResponse>().await {
                    Ok(data) => {
                        let choice = data.choices.first().cloned();
                        match choice {
                            Some(c) => {
                                let content = strip_thinking_tags(&c.message.content);
                                ChatResult {
                                    content,
                                    tool_calls: vec![],
                                    finish_reason: c.finish_reason,
                                    error: None,
                                }
                            }
                            None => ChatResult {
                                content: "".into(),
                                tool_calls: vec![],
                                finish_reason: None,
                                error: Some("no choices in response".to_string()),
                            },
                        }
                    }
                    Err(e) => ChatResult {
                        content: format!("Error parsing response: {}", e),
                        tool_calls: vec![],
                        finish_reason: None,
                        error: Some(e.to_string()),
                    },
                }
            }
            Err(e) => ChatResult {
                content: format!("Error: {}", e),
                tool_calls: vec![],
                finish_reason: None,
                error: Some(e.to_string()),
            },
        }
    }
}

/// Strip <thinking>...</thinking> tags from response text.
pub fn strip_thinking_tags(text: &str) -> String {
    let re = Regex::new(r"(?s)<think(?:ing)?>.*?</think(?:ing)>").unwrap();
    re.replace_all(text, "").to_string().trim().to_string()
}
