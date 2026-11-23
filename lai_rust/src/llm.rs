//! llm.rs - HTTP client for llama.cpp / Ollama local endpoints.
//! Rule 4: only local (loopback/private) endpoints are allowed unless
//! --allow-remote is set.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Clone)]
pub struct LlmClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl LlmClient {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
        }
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
            tool_choice: None,
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

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let data: ChatResponse = resp.json().await?;
        let choice = data.choices.first().cloned().ok_or_else(|| {
            anyhow::anyhow!("No choices in response")
        })?;

        Ok(choice.message.content)
    }

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

        let resp = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        let data: ChatResponse = resp.json().await?;
        let choice = data.choices.first().cloned().ok_or_else(|| {
            anyhow::anyhow!("No choices in response")
        })?;

        Ok(choice.message)
    }
}