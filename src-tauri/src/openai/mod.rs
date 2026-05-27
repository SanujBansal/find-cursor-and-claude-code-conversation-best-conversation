mod client;
mod config;

pub use client::chat_completion;
pub use config::{OpenAiConfig, DEFAULT_OPENAI_MODEL};
