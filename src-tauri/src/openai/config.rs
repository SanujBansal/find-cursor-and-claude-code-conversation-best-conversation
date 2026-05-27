use crate::azure::{load_env_file, optional_env};

pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4.1-mini";

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
}

impl OpenAiConfig {
    pub fn from_env() -> Result<Self, String> {
        load_env_file().ok();
        let api_key = optional_env(&["OPENAI_API_KEY"])
            .ok_or_else(|| "Missing OPENAI_API_KEY".to_string())?;
        let model = optional_env(&["OPENAI_MODEL"])
            .unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string());
        Self::new(api_key, model)
    }

    pub fn new(api_key: String, model: String) -> Result<Self, String> {
        let config = Self { api_key, model };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.api_key.trim().is_empty() {
            return Err("OpenAI API key is empty".to_string());
        }
        if self.model.trim().is_empty() {
            return Err("OpenAI model is empty".to_string());
        }
        Ok(())
    }

    pub fn is_configured() -> bool {
        load_env_file().ok();
        optional_env(&["OPENAI_API_KEY"]).is_some()
    }
}
