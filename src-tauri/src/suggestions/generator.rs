use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::azure::{chat_completion, ChatMessage, AzureOpenAIConfig};

// ── Public output type ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSuggestion {
    pub id: String,
    pub concept: String,
    pub why_it_helps: String,
    pub related_dimension: String,
    pub priority: String, // "high" | "medium" | "low"
    pub example_conversation_id: Option<String>,
    pub generated_at: String, // ISO 8601
    pub is_dismissed: bool,
}

// ── Internal data-transfer types ──────────────────────────────────────────────

pub struct WeakDimensionInfo {
    pub display_name: String,   // camelCase, e.g. "taskCompletion"
    pub average: f64,
    pub examples: Vec<ConversationExample>,
}

pub struct ConversationExample {
    pub conversation_id: i64,
    pub score: f64,
    pub snippet: String, // first 200 chars of explanation
}

/// Parsed suggestion from OpenAI before it gets a DB id / timestamps.
pub struct SuggestionDraft {
    pub concept: String,
    pub why_it_helps: String,
    pub related_dimension: String,
    pub priority: String,
    pub example_conversation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSuggestion {
    concept: String,
    why_it_helps: String,
    related_dimension: String,
    priority: String,
    example_conversation_id: Option<String>,
}

#[derive(Deserialize)]
struct SuggestionsPayload {
    suggestions: Vec<RawSuggestion>,
}

// ── Dimension metadata ────────────────────────────────────────────────────────

static DIMENSIONS: &[(&str, &str)] = &[
    ("taskCompletion", "task_completion"),
    ("technicalCorrectness", "technical_correctness"),
    ("workflowQuality", "workflow_quality"),
    ("toolUseAndContext", "tool_use_and_context"),
    ("communicationClarity", "communication_clarity"),
    ("learningLeverage", "learning_leverage"),
];

// ── Step 1: synchronous DB query ──────────────────────────────────────────────

/// Return the 3 weakest rubric dimensions over the last 30 days, each with
/// 2 representative (lowest-scoring) conversations as evidence.
pub fn collect_weak_dimensions(conn: &Connection) -> Result<Vec<WeakDimensionInfo>, String> {
    let cutoff = (Utc::now() - Duration::days(30)).to_rfc3339();

    let mut stmt = conn
        .prepare(
            "SELECT
                AVG(task_completion),
                AVG(technical_correctness),
                AVG(workflow_quality),
                AVG(tool_use_and_context),
                AVG(communication_clarity),
                AVG(learning_leverage)
             FROM scores
             WHERE scored_at >= ?1",
        )
        .map_err(|e| e.to_string())?;

    let averages: [Option<f64>; 6] = stmt
        .query_row([&cutoff], |row| {
            Ok([
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ])
        })
        .map_err(|e| e.to_string())?;

    // Sort by average ascending, keep worst 3
    let mut ranked: Vec<(usize, f64)> = averages
        .iter()
        .enumerate()
        .filter_map(|(i, avg)| avg.map(|v| (i, v)))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(3);

    let mut result = Vec::new();

    for (dim_idx, avg) in ranked {
        let (display_name, column_name) = DIMENSIONS[dim_idx];

        // Fetch 2 conversations with the lowest score for this dimension
        let sql = format!(
            "SELECT c.id, s.{column_name}, SUBSTR(COALESCE(s.explanation, ''), 1, 200)
             FROM scores s
             JOIN conversations c ON c.id = s.conversation_id
             WHERE s.scored_at >= ?1
             ORDER BY s.{column_name} ASC
             LIMIT 2"
        );

        let mut ex_stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let examples: Vec<ConversationExample> = ex_stmt
            .query_map([&cutoff], |row| {
                Ok(ConversationExample {
                    conversation_id: row.get(0)?,
                    score: row.get(1)?,
                    snippet: row.get::<_, String>(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        result.push(WeakDimensionInfo {
            display_name: display_name.to_string(),
            average: avg,
            examples,
        });
    }

    Ok(result)
}

// ── Step 2: async Azure OpenAI call ───────────────────────────────────────────

pub async fn call_openai_suggestions(
    config: &AzureOpenAIConfig,
    weak_dims: &[WeakDimensionInfo],
    deployment: &str,
) -> Result<Vec<SuggestionDraft>, String> {
    let dim_summary: String = weak_dims
        .iter()
        .map(|d| {
            let examples_text: String = d
                .examples
                .iter()
                .map(|ex| {
                    format!(
                        "  - Conversation {}: score {:.1}/5.0{}",
                        ex.conversation_id,
                        ex.score,
                        if ex.snippet.is_empty() {
                            String::new()
                        } else {
                            format!(" — \"{}\"", ex.snippet)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            if examples_text.is_empty() {
                format!("Dimension: {} (avg: {:.2}/5.0)", d.display_name, d.average)
            } else {
                format!(
                    "Dimension: {} (avg: {:.2}/5.0)\nExamples:\n{}",
                    d.display_name, d.average, examples_text
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let user_content = format!(
        "Weak areas identified in the last 30 days:\n\n{dim_summary}\n\n\
         Respond ONLY with a valid JSON object matching this exact schema:\n\
         {{\"suggestions\": [\
           {{\"concept\": \"string\", \"why_it_helps\": \"string\", \
             \"related_dimension\": \"string\", \"priority\": \"high|medium|low\", \
             \"example_conversation_id\": \"<id string or null>\"}}\
         ]}}\n\n\
         related_dimension must match one of the dimension names above. \
         For example_conversation_id use the conversation ID as a string, or null."
    );

    let content = chat_completion(
        config,
        deployment,
        vec![
            ChatMessage {
                role: "system",
                content: "You are a coding coach analyzing vibe coding sessions. \
                    Based on the weak areas identified, suggest 3-5 specific, actionable learning concepts. \
                    Each suggestion should name a specific concept, explain why it helps, and link it to the \
                    observed weakness. Return only valid JSON matching the requested schema, no markdown fences."
                    .to_string(),
            },
            ChatMessage {
                role: "user",
                content: user_content,
            },
        ],
        None,
    )
    .await?;

    let json_str = extract_json_object(&content);
    let payload: SuggestionsPayload = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse suggestions JSON: {e}\nRaw: {content}"))?;

    Ok(payload
        .suggestions
        .into_iter()
        .map(|s| SuggestionDraft {
            concept: s.concept,
            why_it_helps: s.why_it_helps,
            related_dimension: s.related_dimension,
            priority: s.priority,
            example_conversation_id: s.example_conversation_id,
        })
        .collect())
}

/// Extract the first JSON object from a string (handles markdown fences).
fn extract_json_object(text: &str) -> String {
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

// ── Step 3: synchronous DB store ──────────────────────────────────────────────

/// Upsert parsed suggestions into `learning_suggestions` table and return them.
/// Upsert key: (rubric_dimension, concept).
pub fn store_suggestions(
    conn: &Connection,
    drafts: Vec<SuggestionDraft>,
) -> Result<Vec<LearningSuggestion>, String> {
    let now = Utc::now().to_rfc3339();
    let mut result = Vec::new();

    for s in drafts {
        let priority = normalize_priority(&s.priority);

        // Check for existing row
        let existing_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM learning_suggestions
                 WHERE rubric_dimension = ?1 AND concept = ?2",
                params![s.related_dimension, s.concept],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        let row_id: i64 = if let Some(id) = existing_id {
            conn.execute(
                "UPDATE learning_suggestions
                 SET rationale = ?1,
                     priority = ?2,
                     example_conversation_id = ?3,
                     generated_at = ?4,
                     dismissed = 0
                 WHERE id = ?5",
                params![s.why_it_helps, priority, s.example_conversation_id, now, id],
            )
            .map_err(|e| e.to_string())?;
            id
        } else {
            conn.execute(
                "INSERT INTO learning_suggestions
                    (rubric_dimension, concept, rationale, evidence_conversation_ids,
                     priority, example_conversation_id, generated_at, dismissed, created_at)
                 VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, 0, ?7)",
                params![
                    s.related_dimension,
                    s.concept,
                    s.why_it_helps,
                    priority,
                    s.example_conversation_id,
                    now,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        };

        result.push(LearningSuggestion {
            id: row_id.to_string(),
            concept: s.concept,
            why_it_helps: s.why_it_helps,
            related_dimension: s.related_dimension,
            priority: priority.to_string(),
            example_conversation_id: s.example_conversation_id,
            generated_at: now.clone(),
            is_dismissed: false,
        });
    }

    Ok(result)
}

fn normalize_priority(raw: &str) -> &'static str {
    match raw.to_lowercase().as_str() {
        "high" => "high",
        "low" => "low",
        _ => "medium",
    }
}
