use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::config::AzureOpenAIConfig;

#[derive(Serialize)]
pub struct ChatMessage<'a> {
    pub role: &'a str,
    pub content: String,
}

pub async fn chat_completion(
    config: &AzureOpenAIConfig,
    deployment: &str,
    messages: Vec<ChatMessage<'_>>,
    response_format: Option<serde_json::Value>,
    temperature: Option<f32>,
) -> Result<String, String> {
    let mut body = serde_json::json!({
        "messages": messages.iter().map(|message| {
            serde_json::json!({
                "role": message.role,
                "content": message.content,
            })
        }).collect::<Vec<_>>(),
    });

    if let Some(temp) = temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    if let Some(format) = response_format {
        body["response_format"] = format;
    }

    let client = Client::new();
    let response = client
        .post(config.chat_completions_url(deployment))
        .header("api-key", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("Azure chat request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Azure chat API error {status}: {body}"));
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
        .map_err(|error| format!("Failed to parse Azure chat response: {error}"))?;

    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .ok_or_else(|| "Empty choices from Azure OpenAI".to_string())
}
