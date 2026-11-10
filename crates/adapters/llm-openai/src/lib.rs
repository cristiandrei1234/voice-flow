//! Curatare cu un server local compatibil OpenAI: mlx_lm.server, Ollama (/v1), LM Studio.

pub mod command;

use anyhow::{anyhow, Context};
use serde_json::json;
use std::time::Duration;
use vf_application::{Cleaned, TextCleaner};
use vf_domain::{styles, CleanupSettings};

pub struct OpenAiCleaner {
    agent: ureq::Agent,
    url: String,
    model: String,
    prompt: String,
    level: styles::CleanupLevel,
}

impl OpenAiCleaner {
    pub fn new(s: &CleanupSettings) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(s.timeout_secs))
            .build();
        Self {
            agent,
            url: s.url.clone(),
            model: s.model.clone(),
            prompt: s.prompt.clone(),
            level: s.level,
        }
    }

    /// Trimite textul la model cu promptul de sistem dat (asamblat pentru dictarea asta).
    pub fn clean_with_prompt(&self, system_prompt: &str, text: &str) -> anyhow::Result<String> {
        let body = json!({
            "model": self.model,
            "temperature": 0.1,
            "max_tokens": 2048,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": text},
            ],
        });
        let resp: serde_json::Value = self
            .agent
            .post(&self.url)
            .send_json(body)
            .with_context(|| format!("The corrections server is not responding at {}", self.url))?
            .into_json()?;
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("Unexpected response from the server: {resp}"))?;
        Ok(strip_thinking(content))
    }
}

/// Fara stil pe aplicatie: regulile de baza din setari + nivelul ales la pornire.
impl TextCleaner for OpenAiCleaner {
    fn clean(&self, text: &str, terms: &[&str]) -> anyhow::Result<Cleaned> {
        let prompt = styles::build_prompt(&self.prompt, self.level, "", terms);
        self.clean_with_prompt(&prompt, text).map(Cleaned::new)
    }
}

/// Scoate blocurile <think>...</think> pe care unele modele le pun inainte de raspuns.
pub fn strip_thinking(s: &str) -> String {
    let mut out = s.to_string();
    while let (Some(a), Some(b)) = (out.find("<think>"), out.find("</think>")) {
        if b < a {
            break;
        }
        out.replace_range(a..b + "</think>".len(), "");
    }
    out.trim().trim_matches('"').trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_thinking() {
        assert_eq!(strip_thinking("<think>hmm</think>\n Salut."), "Salut.");
        assert_eq!(strip_thinking("\"Salut.\""), "Salut.");
    }
}
