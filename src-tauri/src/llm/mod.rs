use crate::{
    azure::{self, AzureOpenAIConfig, ChatMessage, DEFAULT_CHAT_DEPLOYMENT},
    openai::{self, OpenAiConfig, DEFAULT_OPENAI_MODEL},
};

#[derive(Debug, Clone)]
pub struct LlmSettings {
    pub azure_endpoint: String,
    pub azure_api_key: String,
    pub open_ai_api_key: String,
    pub scoring_model: String,
}

#[derive(Debug, Clone)]
pub enum LlmConfig {
    OpenAi(OpenAiConfig),
    Azure(AzureOpenAIConfig),
}

impl LlmConfig {
    pub fn model(&self) -> String {
        match self {
            LlmConfig::OpenAi(config) => config.model.clone(),
            LlmConfig::Azure(config) => config.chat_deployment.clone(),
        }
    }

    pub fn provider_label(&self) -> &'static str {
        match self {
            LlmConfig::OpenAi(_) => "OpenAI",
            LlmConfig::Azure(_) => "Azure OpenAI",
        }
    }
}

pub async fn chat_completion(
    config: &LlmConfig,
    messages: Vec<ChatMessage<'_>>,
    response_format: Option<serde_json::Value>,
) -> Result<String, String> {
    match config {
        LlmConfig::OpenAi(openai_config) => {
            openai::chat_completion(
                openai_config,
                &openai_config.model,
                messages,
                response_format,
            )
            .await
        }
        LlmConfig::Azure(azure_config) => {
            azure::chat_completion(
                azure_config,
                &azure_config.chat_deployment,
                messages,
                response_format,
            )
            .await
        }
    }
}

pub fn resolve_llm_config(
    settings: &LlmSettings,
    api_key_override: &str,
    model_override: Option<String>,
) -> Result<LlmConfig, String> {
    if let Some(key) = resolve_openai_key(settings, api_key_override) {
        let model = model_override
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                let from_settings = settings.scoring_model.trim();
                (!from_settings.is_empty()).then(|| from_settings.to_string())
            })
            .unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string());

        return Ok(LlmConfig::OpenAi(OpenAiConfig::new(key, model)?));
    }

    let mut config = AzureOpenAIConfig::load().unwrap_or_else(|_| AzureOpenAIConfig {
        endpoint: String::new(),
        api_key: String::new(),
        api_version: azure::DEFAULT_API_VERSION.to_string(),
        chat_deployment: DEFAULT_CHAT_DEPLOYMENT.to_string(),
    });

    if !settings.azure_endpoint.trim().is_empty() {
        config.endpoint = azure::normalize_endpoint(&settings.azure_endpoint);
    }

    let api_key = if !api_key_override.trim().is_empty() {
        api_key_override.trim().to_string()
    } else if !settings.azure_api_key.trim().is_empty() {
        settings.azure_api_key.trim().to_string()
    } else {
        config.api_key
    };
    config.api_key = api_key;

    let env_chat_deployment = config.chat_deployment.clone();
    config.chat_deployment = model_override
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let from_settings = settings.scoring_model.trim();
            (!from_settings.is_empty()).then(|| from_settings.to_string())
        })
        .unwrap_or(env_chat_deployment);

    config.validate()?;
    Ok(LlmConfig::Azure(config))
}

fn resolve_openai_key(settings: &LlmSettings, api_key_override: &str) -> Option<String> {
    if !settings.open_ai_api_key.trim().is_empty() {
        if !api_key_override.trim().is_empty() {
            return Some(api_key_override.trim().to_string());
        }
        return Some(settings.open_ai_api_key.trim().to_string());
    }

    if OpenAiConfig::is_configured() {
        if !api_key_override.trim().is_empty() {
            return Some(api_key_override.trim().to_string());
        }
        return OpenAiConfig::from_env().ok().map(|config| config.api_key);
    }

    None
}

pub fn openai_credentials_available(settings: &LlmSettings) -> bool {
    if !settings.open_ai_api_key.trim().is_empty() {
        return true;
    }
    OpenAiConfig::is_configured()
}

pub fn azure_credentials_available(
    settings: &LlmSettings,
    env_config: Option<&AzureOpenAIConfig>,
) -> bool {
    let settings_endpoint = settings.azure_endpoint.trim();
    let settings_key = settings.azure_api_key.trim();

    if !settings_endpoint.is_empty() && !settings_key.is_empty() {
        return true;
    }

    env_config.is_some() || AzureOpenAIConfig::is_configured()
}

pub fn ai_credentials_available(
    settings: &LlmSettings,
    env_config: Option<&AzureOpenAIConfig>,
) -> bool {
    openai_credentials_available(settings) || azure_credentials_available(settings, env_config)
}

pub fn ai_provider(settings: &LlmSettings) -> &'static str {
    if openai_credentials_available(settings) {
        "openai"
    } else {
        "azure"
    }
}
