use chrono::Utc;

use crate::{
    azure::{chat_completion, ChatMessage, AzureOpenAIConfig},
    scoring::{
        prompt,
        rubric::{self, RubricDimensions, RUBRIC_VERSION},
    },
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoringResult {
    pub conversation_id: String,
    pub content_hash: String,
    pub dimensions: RubricDimensions,
    pub final_score: f64,
    pub explanation: String,
    pub model_id: String,
    pub rubric_version: String,
    pub prompt_version: String,
    pub cache_key: String,
    pub scored_at: String, // ISO 8601
}

/// Minimal view of a conversation the scorer needs to build the prompt.
pub struct ConversationForScoring {
    pub id: i64,
    pub content_hash: String,
    pub messages: Vec<(String, String)>, // (role, content) ordered by sequence_num
}

#[derive(serde::Deserialize)]
struct ScorePayload {
    #[serde(rename = "taskCompletion")]
    task_completion: i64,
    #[serde(rename = "technicalCorrectness")]
    technical_correctness: i64,
    #[serde(rename = "workflowQuality")]
    workflow_quality: i64,
    #[serde(rename = "toolUseAndContext")]
    tool_use_and_context: i64,
    #[serde(rename = "communicationClarity")]
    communication_clarity: i64,
    #[serde(rename = "learningLeverage")]
    learning_leverage: i64,
    explanation: String,
}

/// Score a batch of up to 5 conversations sequentially. Retries each failed
/// call once before returning an error for that transcript.
pub async fn score_batch(
    conversations: &[ConversationForScoring],
    config: &AzureOpenAIConfig,
    deployment: &str,
) -> Result<Vec<ScoringResult>, String> {
    let mut results = Vec::new();

    for conv in conversations {
        match score_one(conv, config, deployment).await {
            Ok(result) => results.push(result),
            Err(first_err) => match score_one(conv, config, deployment).await {
                Ok(result) => results.push(result),
                Err(retry_err) => {
                    return Err(format!(
                        "Failed to score conversation {}: {}; retry: {}",
                        conv.id, first_err, retry_err
                    ))
                }
            },
        }
    }

    Ok(results)
}

async fn score_one(
    conv: &ConversationForScoring,
    config: &AzureOpenAIConfig,
    deployment: &str,
) -> Result<ScoringResult, String> {
    let prompt_out = prompt::build_prompt(
        &prompt::PromptInput {
            content_hash: conv.content_hash.clone(),
            messages: conv.messages.clone(),
        },
        deployment,
    );

    let json_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "taskCompletion":         { "type": "integer", "minimum": 0, "maximum": 5 },
            "technicalCorrectness":   { "type": "integer", "minimum": 0, "maximum": 5 },
            "workflowQuality":        { "type": "integer", "minimum": 0, "maximum": 5 },
            "toolUseAndContext":      { "type": "integer", "minimum": 0, "maximum": 5 },
            "communicationClarity":   { "type": "integer", "minimum": 0, "maximum": 5 },
            "learningLeverage":       { "type": "integer", "minimum": 0, "maximum": 5 },
            "explanation":            { "type": "string" }
        },
        "required": [
            "taskCompletion", "technicalCorrectness", "workflowQuality",
            "toolUseAndContext", "communicationClarity", "learningLeverage",
            "explanation"
        ],
        "additionalProperties": false
    });

    let response_format = serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "transcript_score",
            "strict": true,
            "schema": json_schema
        }
    });

    let content = chat_completion(
        config,
        deployment,
        vec![ChatMessage {
            role: "user",
            content: prompt_out.prompt,
        }],
        Some(response_format),
    )
    .await?;

    let payload: ScorePayload =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse score JSON: {e}"))?;

    for (name, val) in [
        ("taskCompletion", payload.task_completion),
        ("technicalCorrectness", payload.technical_correctness),
        ("workflowQuality", payload.workflow_quality),
        ("toolUseAndContext", payload.tool_use_and_context),
        ("communicationClarity", payload.communication_clarity),
        ("learningLeverage", payload.learning_leverage),
    ] {
        if !(0..=5).contains(&val) {
            return Err(format!("Dimension '{name}' value {val} is out of range 0-5"));
        }
    }

    let dimensions = RubricDimensions {
        task_completion: payload.task_completion as f64,
        technical_correctness: payload.technical_correctness as f64,
        workflow_quality: payload.workflow_quality as f64,
        tool_use_and_context: payload.tool_use_and_context as f64,
        communication_clarity: payload.communication_clarity as f64,
        learning_leverage: payload.learning_leverage as f64,
    };

    let final_score = rubric::compute_final_score(&dimensions);

    Ok(ScoringResult {
        conversation_id: conv.id.to_string(),
        content_hash: conv.content_hash.clone(),
        dimensions,
        final_score,
        explanation: payload.explanation,
        model_id: deployment.to_string(),
        rubric_version: RUBRIC_VERSION.to_string(),
        prompt_version: prompt::PROMPT_VERSION.to_string(),
        cache_key: prompt_out.cache_key,
        scored_at: Utc::now().to_rfc3339(),
    })
}
