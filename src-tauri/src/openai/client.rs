use reqwest::Client;
use serde::Deserialize;

use super::config::OpenAiConfig;
use crate::azure::ChatMessage;

const OPENAI_CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";

pub async fn chat_completion(
    config: &OpenAiConfig,
    model: &str,
    messages: Vec<ChatMessage<'_>>,
    response_format: Option<serde_json::Value>,
) -> Result<String, String> {
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages.iter().map(|message| {
            serde_json::json!({
                "role": message.role,
                "content": message.content,
            })
        }).collect::<Vec<_>>(),
    });

    if let Some(format) = response_format {
        body["response_format"] = format;
    }

    let client = Client::new();
    let response = client
        .post(OPENAI_CHAT_URL)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("OpenAI chat request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI chat API error {status}: {body}"));
    }

    #[derive(Deserialize)]
    struct ChatResponse {
        choices: Vec<ChatChoice>,
    }

    #[derive(Deserialize)]
    struct ChatChoice {
        message: ChatMsg,
    }

    #[derive(Deserialize)]
    struct ChatMsg {
        content: String,
    }

    let parsed: ChatResponse = response
        .json()
        .await
        .map_err(|error| format!("Failed to parse OpenAI chat response: {error}"))?;

    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| "Empty choices from OpenAI".to_string())
}
