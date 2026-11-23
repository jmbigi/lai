use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message { pub role: String, pub content: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest { pub model: String, pub messages: Vec<Message>, pub tools: Option<Vec<serde_json::Value>>, pub tool_choice: Option<String>, pub stream: Option<bool>, pub temperature: Option<f64>, pub max_tokens: Option<i32>, pub top_p: Option<f64>, pub top_k: Option<i32>, pub chat_template_kwargs: Option<HashMap<String, serde_json::Value>> }
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse { pub choices: Vec<Choice>, #[serde(default)] pub usage: serde_json::Value }
#[derive(Debug, Clone, Deserialize)]
pub struct Choice { pub message: Message, #[serde(default)] pub finish_reason: Option<String> }
pub struct LlmClient { pub base_url: String, pub model: String, pub client: reqwest::Client }
impl LlmClient {
    pub fn new(base_url: String, model: String) -> Self { Self { base_url, model, client: Client::new() } }
    pub async fn chat(&self, prompt: &str) -> Result<String> {
        let payload = ChatRequest { model: self.model.clone(), messages: vec![Message { role: "user".to_string(), content: prompt.to_string() }], tools: None, tool_choice: None, stream: Some(false), temperature: Some(0.7), max_tokens: Some(4096), top_p: Some(0.95), top_k: Some(20), chat_template_kwargs: Some(HashMap::from([("enable_thinking".into(), serde_json::Value::Bool(false))])) };
        let resp = self.client.post(format!("{}/chat/completions", self.base_url)).json(&payload).send().await?;
        if !resp.status().is_success() { let s = resp.status(); let t = resp.text().await.unwrap_or_default(); anyhow::bail!("HTTP {}: {}", s, t); }
        let data: ChatResponse = resp.json().await?;
        let choice = data.choices.first().cloned().ok_or_else(|| anyhow::anyhow!("No choices"))?;
        Ok(choice.message.content)
    }
}
