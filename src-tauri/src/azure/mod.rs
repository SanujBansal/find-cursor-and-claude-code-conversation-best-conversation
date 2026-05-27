mod client;
mod config;

pub use client::{chat_completion, ChatMessage};
pub use config::{
    normalize_endpoint, AzureOpenAIConfig, DEFAULT_API_VERSION, DEFAULT_CHAT_DEPLOYMENT,
};
