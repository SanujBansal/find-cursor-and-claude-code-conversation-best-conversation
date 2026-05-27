use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Deterministic: sha256 of source_type + path + session_id
    pub id: String,
    /// "cursor-local" | "claude-code-local" | "claude-web-markdown"
    pub source_type: String,
    pub title: String,
    pub project_path: Option<String>,
    /// ISO 8601
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// "user" | "assistant" | "tool"
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
    /// Tool names invoked
    pub tool_calls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub source_type: String,
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub id: i64,
    pub job_type: String,
    pub status: String,
    pub progress: Option<f64>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
