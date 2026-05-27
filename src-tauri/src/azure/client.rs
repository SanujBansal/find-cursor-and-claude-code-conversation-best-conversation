use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::config::AzureOpenAIConfig;

const EMBEDDING_BATCH_SIZE: usize = 20;

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Serialize)]
pub struct ChatMessage<'a> {
    pub role: &'a str,
    pub content: String,
}

pub async fn create_embeddings(
    config: &AzureOpenAIConfig,
    deployment: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let client = Client::new();
    let url = config.embeddings_url(deployment);
    let mut all_embeddings = Vec::with_capacity(texts.len());

    for batch in texts.chunks(EMBEDDING_BATCH_SIZE) {
        let request = EmbeddingRequest { input: batch };

        let response = client
            .post(&url)
            .header("api-key", &config.api_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|error| format!("Azure embedding request failed: {error}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Azure embedding API error {status}: {body}"));
        }

        let parsed: EmbeddingResponse = response
            .json()
            .await
            .map_err(|error| format!("Failed to parse Azure embedding response: {error}"))?;

        for datum in parsed.data {
            all_embeddings.push(datum.embedding);
        }
    }

    Ok(all_embeddings)
}

pub async fn chat_completion(
    config: &AzureOpenAIConfig,
    deployment: &str,
    messages: Vec<ChatMessage<'_>>,
    response_format: Option<serde_json::Value>,
) -> Result<String, String> {
    let mut body = serde_json::json!({
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
