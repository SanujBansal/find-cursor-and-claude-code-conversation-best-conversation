use chrono::Utc;

use crate::{
    llm::{self, LlmConfig},
    scoring::{
        prompt,
        rubric::{self, RubricDimensions, RUBRIC_VERSION},
    },
};
use crate::azure::ChatMessage;

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
    #[serde(rename = "conceptualKnowledge")]
    conceptual_knowledge: i64,
    #[serde(rename = "attentionToDetail")]
    attention_to_detail: i64,
    #[serde(rename = "problemDecomposition")]
    problem_decomposition: i64,
    #[serde(rename = "criticalEvaluation")]
    critical_evaluation: i64,
    #[serde(rename = "robustnessAwareness")]
    robustness_awareness: i64,
    #[serde(rename = "debuggingSkill")]
    debugging_skill: i64,
    #[serde(rename = "promptSpecificity")]
    prompt_specificity: i64,
    #[serde(rename = "scopeDiscipline")]
    scope_discipline: i64,
    explanation: String,
}

/// Score a batch of up to 5 conversations sequentially. Retries each failed
/// call once before returning an error for that transcript.
pub async fn score_batch(
    conversations: &[ConversationForScoring],
    config: &LlmConfig,
    model: &str,
) -> Result<Vec<ScoringResult>, String> {
    let mut results = Vec::new();

    for conv in conversations {
        match score_one(conv, config, model).await {
            Ok(result) => results.push(result),
            Err(first_err) => match score_one(conv, config, model).await {
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
    config: &LlmConfig,
    model: &str,
) -> Result<ScoringResult, String> {
    let prompt_out = prompt::build_prompt(
        &prompt::PromptInput {
            content_hash: conv.content_hash.clone(),
            messages: conv.messages.clone(),
        },
        model,
    );

    let json_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "conceptualKnowledge":   { "type": "integer", "minimum": 0, "maximum": 5 },
            "attentionToDetail":     { "type": "integer", "minimum": 0, "maximum": 5 },
            "problemDecomposition":  { "type": "integer", "minimum": 0, "maximum": 5 },
            "criticalEvaluation":    { "type": "integer", "minimum": 0, "maximum": 5 },
            "robustnessAwareness":   { "type": "integer", "minimum": 0, "maximum": 5 },
            "debuggingSkill":        { "type": "integer", "minimum": 0, "maximum": 5 },
            "promptSpecificity":     { "type": "integer", "minimum": 0, "maximum": 5 },
            "scopeDiscipline":       { "type": "integer", "minimum": 0, "maximum": 5 },
            "explanation":           { "type": "string" }
        },
        "required": [
            "conceptualKnowledge", "attentionToDetail", "problemDecomposition",
            "criticalEvaluation", "robustnessAwareness", "debuggingSkill",
            "promptSpecificity", "scopeDiscipline", "explanation"
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

    let content = llm::chat_completion(
        config,
        vec![ChatMessage {
            role: "user",
            content: prompt_out.prompt,
        }],
        Some(response_format),
        Some(0.0),
    )
    .await?;

    let payload: ScorePayload =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse score JSON: {e}"))?;

    for (name, val) in [
        ("conceptualKnowledge", payload.conceptual_knowledge),
        ("attentionToDetail", payload.attention_to_detail),
        ("problemDecomposition", payload.problem_decomposition),
        ("criticalEvaluation", payload.critical_evaluation),
        ("robustnessAwareness", payload.robustness_awareness),
        ("debuggingSkill", payload.debugging_skill),
        ("promptSpecificity", payload.prompt_specificity),
        ("scopeDiscipline", payload.scope_discipline),
    ] {
        if !(0..=5).contains(&val) {
            return Err(format!("Dimension '{name}' value {val} is out of range 0-5"));
        }
    }

    let dimensions = RubricDimensions {
        conceptual_knowledge: payload.conceptual_knowledge as f64,
        attention_to_detail: payload.attention_to_detail as f64,
        problem_decomposition: payload.problem_decomposition as f64,
        critical_evaluation: payload.critical_evaluation as f64,
        robustness_awareness: payload.robustness_awareness as f64,
        debugging_skill: payload.debugging_skill as f64,
        prompt_specificity: payload.prompt_specificity as f64,
        scope_discipline: payload.scope_discipline as f64,
    };

    let final_score = rubric::compute_final_score(&dimensions);

    Ok(ScoringResult {
        conversation_id: conv.id.to_string(),
        content_hash: conv.content_hash.clone(),
        dimensions,
        final_score,
        explanation: payload.explanation,
        model_id: model.to_string(),
        rubric_version: RUBRIC_VERSION.to_string(),
        prompt_version: prompt::PROMPT_VERSION.to_string(),
        cache_key: prompt_out.cache_key,
        scored_at: Utc::now().to_rfc3339(),
    })
}
