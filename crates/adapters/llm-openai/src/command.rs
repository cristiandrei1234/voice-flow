//! Command Mode: transforma textul selectat dupa instructiunea rostita, cu acelasi server local
//! compatibil OpenAI ca la corecturi (url, model si timeout din `settings.cleanup`).

use anyhow::{anyhow, Context};
use serde_json::json;
use std::time::Duration;
use vf_application::command::TextTransformer;
use vf_domain::{command, CleanupSettings};

pub struct OpenAiTransformer {
    agent: ureq::Agent,
    url: String,
    model: String,
}

impl OpenAiTransformer {
    pub fn new(s: &CleanupSettings) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(s.timeout_secs))
            .build();
        Self {
            agent,
            url: s.url.clone(),
            model: s.model.clone(),
        }
    }
}

/// Corpul cererii catre server.
pub fn request_body(model: &str, system_prompt: &str, selection: &str, instruction: &str) -> serde_json::Value {
    json!({
        "model": model,
        "temperature": 0.2,
        "max_tokens": command::max_tokens(selection),
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": command::user_message(selection, instruction)},
        ],
    })
}

/// Textul raspunsului, fara blocurile de gandire.
pub fn answer_text(resp: &serde_json::Value) -> anyhow::Result<String> {
    let content = resp["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("Unexpected response from the server: {resp}"))?;
    Ok(crate::strip_thinking(content))
}

impl TextTransformer for OpenAiTransformer {
    fn transform(&self, system_prompt: &str, selection: &str, instruction: &str) -> anyhow::Result<String> {
        let resp: serde_json::Value = self
            .agent
            .post(&self.url)
            .send_json(request_body(&self.model, system_prompt, selection, instruction))
            .with_context(|| {
                format!("The corrections model is not responding at {} (start it from the Corrections section)", self.url)
            })?
            .into_json()?;
        answer_text(&resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_request() {
        let body = request_body("m", "PROMPT", "Salut", "tradu în spaniolă");
        assert_eq!(body["model"], "m");
        assert_eq!(body["max_tokens"], 512);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "PROMPT");
        let user = body["messages"][1]["content"].as_str().unwrap();
        assert!(user.contains("<<<\nSalut\n>>>"));
        assert!(user.starts_with("Selected text:\n<<<\nSalut\n>>>"));
        assert!(user.ends_with("Instruction: tradu în spaniolă\n\nResult:"));
    }

    #[test]
    fn reads_answer() {
        let resp = json!({"choices": [{"message": {"content": "<think>hm</think>\nHola."}}]});
        assert_eq!(answer_text(&resp).unwrap(), "Hola.");
        assert!(answer_text(&json!({"error": "x"})).is_err());
    }
}
