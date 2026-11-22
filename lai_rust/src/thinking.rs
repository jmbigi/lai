//! Thinking Management - Extract and display reasoning from model responses.
//! Ported from lai.py ThinkingManager with full feature parity.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
    pub content: String,
    pub start_time: f64,
    pub end_time: Option<f64>,
    pub tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingSummary {
    pub blocks: usize,
    pub total_tokens: usize,
    pub blocks_detail: Vec<ThinkingBlockDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlockDetail {
    pub tokens: usize,
    pub preview: String,
}

pub struct ThinkingManager {
    pub enabled: bool,
    pub blocks: Vec<ThinkingBlock>,
    patterns: Vec<Regex>,
}

impl ThinkingManager {
    pub fn new(enabled: bool) -> Self {
        let patterns = vec![
            Regex::new(r"<thinking>\s*(.*?)\s*</thinking>").unwrap(),           // Qwen3 standard
            Regex::new(r"questions\s*(.*?)\s*回答").unwrap(),                    // DeepSeek style
            Regex::new(r"questions\s*(.*?)\s*</thinking>").unwrap(),             // questions with </thinking>
            Regex::new(r"think\s*(.*?)\s*</thinking>").unwrap(),                 // think with </thinking>
            Regex::new(r"\|begin_thinking\|(.*?)\|end_thinking\|").unwrap(),     // Some models
            Regex::new(r"<think>\s*(.*?)\s*</think>").unwrap(),                  // Generic think tags
        ];

        Self {
            enabled,
            blocks: Vec::new(),
            patterns,
        }
    }

    pub fn extract_reasoning(&mut self, content: &str) -> (String, String) {
        if !self.enabled {
            return (String::new(), content.to_string());
        }

        for pattern in &self.patterns {
            if let Some(m) = pattern.find(content) {
                let thinking = m.as_str()
                    .trim_start_matches(|c: char| !c.is_alphabetic() && c != '<' && c != '|')
                    .trim_end_matches(|c: char| !c.is_alphabetic() && c != '>' && c != '|')
                    .trim()
                    .to_string();

                // Extract just the inner content
                let thinking = if let Some(caps) = pattern.captures(content) {
                    caps.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or(thinking)
                } else {
                    thinking
                };

                let clean_content = pattern.replace_all(content, "").to_string().trim().to_string();

                let block = ThinkingBlock {
                    content: thinking.clone(),
                    start_time: Instant::now().elapsed().as_secs_f64() - 1.0,
                    end_time: Some(Instant::now().elapsed().as_secs_f64()),
                    tokens: thinking.len() / 4,
                };
                self.blocks.push(block);

                return (thinking, clean_content);
            }
        }

        (String::new(), content.to_string())
    }

    pub fn format_for_display(&self, thinking: &str, content: &str) -> String {
        if self.enabled && !thinking.is_empty() {
            format!("\x1b[93m<thinking>\n{}\n</thinking>\x1b[0m\n\n{}", thinking, content)
        } else {
            content.to_string()
        }
    }

    pub fn get_summary(&self) -> ThinkingSummary {
        let total_tokens: usize = self.blocks.iter().map(|b| b.tokens).sum();
        let blocks_detail = self.blocks.iter().map(|b| ThinkingBlockDetail {
            tokens: b.tokens,
            preview: b.content.chars().take(100).collect(),
        }).collect();

        ThinkingSummary {
            blocks: self.blocks.len(),
            total_tokens,
            blocks_detail,
        }
    }

    pub fn clear(&mut self) {
        self.blocks.clear();
    }

    pub fn strip_thinking_artifacts(text: &str) -> String {
        let mut result = text.to_string();

        // Remove <thinking>...</thinking> tags
        result = Regex::new(r"(?s)<think(?:ing)?>.*?</think(?:ing)>").unwrap()
            .replace_all(&result, "").to_string();

        // Remove 思考...回答 patterns
        result = Regex::new(r"(?s)思考.*?回答").unwrap()
            .replace_all(&result, "").to_string();

        // Remove thinking...回答 patterns
        result = Regex::new(r"(?si)thinking.*?回答").unwrap()
            .replace_all(&result, "").to_string();

        // Remove question...回答 patterns
        result = Regex::new(r"(?si)question.*?回答").unwrap()
            .replace_all(&result, "").to_string();

        // Remove 回答 prefix
        if result.contains("回答") {
            let parts: Vec<&str> = result.split("回答").collect();
            if parts.len() > 1 {
                result = parts.last().unwrap().trim().to_string();
            }
        }

        // Remove markdown code fences if present
        result = result.trim().to_string();
        if result.contains("```python") {
            if let Some(m) = Regex::new(r"```python\n(.*?)\n```").unwrap().find(&result) {
                return m.as_str().trim_start_matches("```python").trim_end_matches("```").trim().to_string();
            }
        } else if result.contains("```") {
            if let Some(m) = Regex::new(r"```\n(.*?)\n```").unwrap().find(&result) {
                return m.as_str().trim_start_matches("```").trim_end_matches("```").trim().to_string();
            }
        }

        result.trim().to_string()
    }
}

impl Default for ThinkingManager {
    fn default() -> Self {
        Self::new(true)
    }
}

/// Extract reasoning content from streaming response chunks
pub fn extract_streaming_reasoning(chunk: &str) -> Option<String> {
    // Try to extract from delta.reasoning_content
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(chunk) {
        if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
            if let Some(delta) = choices.first().and_then(|c| c.get("delta")) {
                if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str()) {
                    if !reasoning.is_empty() {
                        return Some(reasoning.to_string());
                    }
                }
                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                    if !content.is_empty() {
                        return Some(content.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Process streaming response and separate thinking from content
pub struct StreamingThinkingProcessor {
    thinking_buffer: String,
    content_buffer: String,
    in_thinking: bool,
}

impl StreamingThinkingProcessor {
    pub fn new() -> Self {
        Self {
            thinking_buffer: String::new(),
            content_buffer: String::new(),
            in_thinking: false,
        }
    }

    pub fn process_chunk(&mut self, chunk: &str) -> (Option<String>, Option<String>) {
        let mut new_thinking = None;
        let mut new_content = None;

        // Check for thinking tags in chunk
        if chunk.contains("<thinking>") || chunk.contains("<think>") {
            self.in_thinking = true;
        }

        if self.in_thinking {
            self.thinking_buffer.push_str(chunk);
            if chunk.contains("</thinking>") || chunk.contains("</think>") {
                self.in_thinking = false;
                new_thinking = Some(self.thinking_buffer.clone());
                self.thinking_buffer.clear();
            }
        } else {
            self.content_buffer.push_str(chunk);
            new_content = Some(chunk.to_string());
        }

        (new_thinking, new_content)
    }

    pub fn get_accumulated(&self) -> (String, String) {
        (self.thinking_buffer.clone(), self.content_buffer.clone())
    }

    pub fn reset(&mut self) {
        self.thinking_buffer.clear();
        self.content_buffer.clear();
        self.in_thinking = false;
    }
}

impl Default for StreamingThinkingProcessor {
    fn default() -> Self {
        Self::new()
    }
}