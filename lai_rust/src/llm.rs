//! llm.rs - HTTP client for llama.cpp / Ollama local endpoints.
//! Rule 4: only local (loopback/private) endpoints are allowed unless
//! --allow-remote is set.

use anyhow::Result;
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
            client: reqwest::Client::new(),
        }
    }

    fn chat_request(&self) -> reqwest::RequestBuilder {
        let request = self
            .client
            .post(format!("{}/chat/completions", self.base_url));
        match &self.api_key {
            Some(key) => request.bearer_auth(key),
            None => request,
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

        let resp = self.chat_request().json(&payload).send().await?;

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

        let resp = self.chat_request().json(&payload).send().await?;

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

        let resp = self.chat_request().json(&payload).send().await?;

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

        let resp = self.chat_request().json(&payload).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {}: {}", status, text);
        }

        Ok(resp)
    }
}
