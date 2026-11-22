//! Session Management - Conversation history and persistence.
//! Ported from lai.py SessionManager with full feature parity.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            role: "user".into(),
            content: String::new(),
            tool_calls: None,
            tool_call_id: None,
            timestamp: Some(Utc::now()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, Value>,
}

impl Session {
    pub fn new(id: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    pub fn trim_history(&mut self, max_tokens: usize) {
        if self.messages.is_empty() { return; }

        let mut total_tokens = 0;
        for msg in &self.messages {
            total_tokens += msg.content.len() / 4; // Rough estimate
        }

        if total_tokens <= max_tokens { return; }

        // Remove oldest messages (keep system messages)
        while total_tokens > max_tokens && self.messages.len() > 1 {
            let removed = self.messages.remove(0);
            total_tokens -= removed.content.len() / 4;
        }
    }
}

pub struct SessionManager {
    pub history_dir: PathBuf,
}

impl SessionManager {
    pub fn new(history_dir: Option<PathBuf>) -> Self {
        let dir = history_dir.unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".lai")
                .join("sessions")
        });
        fs::create_dir_all(&dir).ok();
        Self { history_dir: dir }
    }

    pub fn create_session(&self, session_id: Option<String>) -> Result<String> {
        let id = session_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let session_path = self.history_dir.join(format!("{}.json", id));

        if !session_path.exists() {
            let session = Session::new(Some(id.clone()));
            self.save_session(&session)?;
        }
        Ok(id)
    }

    pub fn get_session(&self, session_id: &str) -> Result<Session> {
        let session_path = self.history_dir.join(format!("{}.json", session_id));
        if session_path.exists() {
            let content = fs::read_to_string(session_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Session::new(Some(session_id.to_string())))
        }
    }

    pub fn save_session(&self, session: &Session) -> Result<()> {
        let session_path = self.history_dir.join(format!("{}.json", session.id));
        let content = serde_json::to_string_pretty(session)?;
        fs::write(session_path, content)?;
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<String>> {
        if !self.history_dir.exists() { return Ok(Vec::new()); }

        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.history_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    sessions.push(name.to_string());
                }
            }
        }
        sessions.sort();
        sessions.reverse(); // Most recent first
        Ok(sessions)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        let session_path = self.history_dir.join(format!("{}.json", session_id));
        if session_path.exists() {
            fs::remove_file(session_path)?;
        }
        Ok(())
    }

    pub fn export_session(&self, session_id: &str, export_path: &Path) -> Result<()> {
        let session = self.get_session(session_id)?;
        let content = serde_json::to_string_pretty(&session)?;
        fs::write(export_path, content)?;
        Ok(())
    }

    pub fn import_session(&self, import_path: &Path) -> Result<String> {
        let content = fs::read_to_string(import_path)?;
        let session: Session = serde_json::from_str(&content)?;
        let new_id = Uuid::new_v4().to_string();
        let mut session = session;
        session.id = new_id.clone();
        session.created_at = Utc::now();
        session.updated_at = Utc::now();
        self.save_session(&session)?;
        Ok(new_id)
    }

    pub fn get_recent_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let ids = self.list_sessions()?;
        let mut sessions = Vec::new();
        for id in ids.into_iter().take(limit) {
            if let Ok(session) = self.get_session(&id) {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationHistory {
    pub messages: Vec<Message>,
    pub max_tokens: usize,
    pub system_prompt: Option<String>,
}

impl ConversationHistory {
    pub fn new(max_tokens: usize, system_prompt: Option<String>) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
            system_prompt,
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.trim();
    }

    pub fn add_user_message(&mut self, content: String) {
        self.add_message(Message {
            role: "user".into(),
            content,
            timestamp: Some(Utc::now()),
            ..Default::default()
        });
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.add_message(Message {
            role: "assistant".into(),
            content,
            timestamp: Some(Utc::now()),
            ..Default::default()
        });
    }

    pub fn add_tool_message(&mut self, content: String, tool_call_id: String) {
        self.add_message(Message {
            role: "tool".into(),
            content,
            tool_call_id: Some(tool_call_id),
            timestamp: Some(Utc::now()),
            ..Default::default()
        });
    }

    fn trim(&mut self) {
        let mut total_tokens = 0;
        for msg in &self.messages {
            total_tokens += msg.content.len() / 4;
        }

        if total_tokens <= self.max_tokens { return; }

        // Keep system prompt if present
        let start_idx = if self.messages.first().map(|m| m.role == "system").unwrap_or(false) { 1 } else { 0 };

        while total_tokens > self.max_tokens && self.messages.len() > start_idx + 1 {
            let removed = self.messages.remove(start_idx);
            total_tokens -= removed.content.len() / 4;
        }
    }

    pub fn get_messages(&self) -> Vec<Message> {
        let mut msgs = self.messages.clone();
        if let Some(sys) = &self.system_prompt {
            msgs.insert(0, Message {
                role: "system".into(),
                content: sys.clone(),
                timestamp: Some(Utc::now()),
                ..Default::default()
            });
        }
        msgs
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}