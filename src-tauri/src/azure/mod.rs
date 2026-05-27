mod client;
mod config;

pub use client::{chat_completion, ChatMessage};
pub use config::{
    load_env_file, normalize_endpoint, optional_env, AzureOpenAIConfig, DEFAULT_API_VERSION,
    DEFAULT_CHAT_DEPLOYMENT,
};
