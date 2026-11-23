//! llm.rs - HTTP client for llama.cpp / Ollama local endpoints.
//! Rule 4: only local (loopback/private) endpoints are allowed unless
//! --allow-remote is set.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message {
    pub role: String,
    pub content: String,
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
    #[allow(dead_code)]
    pub usage: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub message: Message,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChatResult {
    pub content: String,
    #[allow(dead_code)]
    pub tool_calls: Vec<ToolCall>,
    #[allow(dead_code)]
    pub finish_reason: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    pub base_url: String,
    pub model: String,
    pub client: Client,
    pub allow_remote: bool,
    pub temperature: f64,
    pub max_tokens: i32,
    #[allow(dead_code)]
    pub thinking: bool,
}

impl LlmClient {
    pub fn new(base_url: &str, model: &str, allow_remote: bool) -> Self {
        Self {
            base_url: base_url.to_string(),
            model: model.to_string(),
            client: Client::new(),
            allow_remote,
            temperature: 0.7,
            max_tokens: 4096,
            thinking: true,
        }
    }

    pub fn is_local_url(url: &str) -> bool {
        url.starts_with("http://127.0.0.1")
            || url.starts_with("http://localhost")
            || url.starts_with("http://[::1]")
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

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&payload)
            .send()
            .await;

        match resp {
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

                let data: ChatResponse = resp.json().await.unwrap_or_else(|_| ChatResponse {
                    choices: vec![],
                    usage: serde_json::Value::Null,
                });

                let choice = data.choices.first().cloned().unwrap_or(Choice {
                    message: Message {
                        role: "assistant".into(),
                        content: String::new(),
                    },
                    finish_reason: None,
                });

                ChatResult {
                    content: choice.message.content,
                    tool_calls: vec![],
                    finish_reason: choice.finish_reason,
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
}
