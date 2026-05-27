use std::path::{Path, PathBuf};

pub const DEFAULT_API_VERSION: &str = "2024-02-15-preview";
pub const DEFAULT_CHAT_DEPLOYMENT: &str = "gpt-4.1-mini";
pub const DEFAULT_EMBEDDING_DEPLOYMENT: &str = "text-embedding-3-small";

#[derive(Debug, Clone)]
pub struct AzureOpenAIConfig {
    pub endpoint: String,
    pub api_key: String,
    pub api_version: String,
    pub chat_deployment: String,
    pub embedding_deployment: String,
}

impl AzureOpenAIConfig {
    pub fn load() -> Result<Self, String> {
        load_env_file()?;

        let endpoint = required_env(&[
            "AZURE_OPENAI_ENDPOINT",
            "AZURE_OPENAI_API_ENDPOINT",
        ])?;
        let api_key = required_env(&["AZURE_OPENAI_API_KEY"])?;
        let api_version = optional_env(&[
            "AZURE_OPENAI_API_VERSION",
            "OPENAI_API_VERSION",
        ])
        .unwrap_or_else(|| DEFAULT_API_VERSION.to_string());
        let chat_deployment = optional_env(&[
            "AZURE_OPENAI_DEPLOYMENT_NAME",
            "AZURE_OPENAI_DEPLOYMENT",
            "AZURE_OPENAI_CHAT_DEPLOYMENT_NAME",
        ])
        .unwrap_or_else(|| DEFAULT_CHAT_DEPLOYMENT.to_string());
        let embedding_deployment = optional_env(&[
            "AZURE_OPENAI_EMBEDDING_DEPLOYMENT_NAME",
            "AZURE_OPENAI_EMBEDDING_DEPLOYMENT",
        ])
        .unwrap_or_else(|| DEFAULT_EMBEDDING_DEPLOYMENT.to_string());

        let config = Self {
            endpoint: normalize_endpoint(&endpoint),
            api_key,
            api_version,
            chat_deployment,
            embedding_deployment,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.is_empty() {
            return Err("Azure OpenAI endpoint is empty".to_string());
        }
        if self.api_key.is_empty() {
            return Err("Azure OpenAI API key is empty".to_string());
        }
        Ok(())
    }

    pub fn embeddings_url(&self, deployment: &str) -> String {
        format!(
            "{}/openai/deployments/{}/embeddings?api-version={}",
            self.endpoint, deployment, self.api_version
        )
    }

    pub fn chat_completions_url(&self, deployment: &str) -> String {
        format!(
            "{}/openai/deployments/{}/chat/completions?api-version={}",
            self.endpoint, deployment, self.api_version
        )
    }

    pub fn is_configured() -> bool {
        load_env_file().ok();
        optional_env(&["AZURE_OPENAI_ENDPOINT", "AZURE_OPENAI_API_ENDPOINT"]).is_some()
            && optional_env(&["AZURE_OPENAI_API_KEY"]).is_some()
    }

    pub fn masked_endpoint() -> Option<String> {
        load_env_file().ok();
        optional_env(&["AZURE_OPENAI_ENDPOINT", "AZURE_OPENAI_API_ENDPOINT"])
    }
}

fn load_env_file() -> Result<(), String> {
    for path in env_file_candidates() {
        if path.exists() {
            dotenvy::from_path(&path).map_err(|error| {
                format!(
                    "Failed to load Azure env file {}: {error}",
                    path.display()
                )
            })?;
            return Ok(());
        }
    }

    Err(format!(
        "Azure env file not found. Expected one of: {}",
        env_file_candidates()
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn env_file_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let project_root = PathBuf::from(&manifest_dir).join("..");
        candidates.push(project_root.join(".env"));
        candidates.push(
            project_root
                .join("..")
                .join("expense-tracking-application")
                .join(".env"),
        );
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(".env"));
        candidates.push(resolve_env_from_base(&cwd));
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.join(".env"));
            candidates.push(resolve_env_from_base(parent));
        }
    }

    candidates.dedup();
    candidates
}

fn resolve_env_from_base(base: &Path) -> PathBuf {
    base.join("..")
        .join("expense-tracking-application")
        .join(".env")
}

pub fn normalize_endpoint(endpoint: &str) -> String {
    endpoint.trim().trim_end_matches('/').to_string()
}

fn required_env(keys: &[&str]) -> Result<String, String> {
    optional_env(keys).ok_or_else(|| {
        format!(
            "Missing required Azure env var (set one of: {})",
            keys.join(", ")
        )
    })
}

fn optional_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|value| !value.is_empty())
}
