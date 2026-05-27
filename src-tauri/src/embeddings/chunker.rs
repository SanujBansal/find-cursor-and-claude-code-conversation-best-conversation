use hex;
use sha2::{Digest, Sha256};

/// Approximate token limit in characters (1 token ≈ 4 chars, target ~400 tokens)
const CHUNK_CHAR_LIMIT: usize = 1600;

pub struct ConversationChunk {
    /// sha256(conversation_id + chunk_index)
    pub id: String,
    pub chunk_index: usize,
    /// Formatted "User: …\nAssistant: …"
    pub text: String,
}

/// Split a conversation's messages into ~400-token chunks.
/// Each chunk contains complete messages; no mid-message splits.
pub fn chunk_messages(
    conversation_id: &str,
    messages: &[(String, String)], // (role, content)
) -> Vec<ConversationChunk> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut chunks: Vec<ConversationChunk> = Vec::new();
    let mut current_chars: usize = 0;
    let mut current_lines: Vec<String> = Vec::new();

    for (role, content) in messages.iter() {
        let line = format!("{}: {}", format_role(role), content);
        let line_chars = line.chars().count();

        if current_chars + line_chars > CHUNK_CHAR_LIMIT && !current_lines.is_empty() {
            let chunk_index = chunks.len();
            chunks.push(ConversationChunk {
                id: derive_chunk_id(conversation_id, chunk_index),
                chunk_index,
                text: current_lines.join("\n"),
            });
            current_lines = vec![line];
            current_chars = line_chars;
        } else {
            current_chars += line_chars + 1; // +1 for the newline separator
            current_lines.push(line);
        }
    }

    // Flush the final chunk
    if !current_lines.is_empty() {
        let chunk_index = chunks.len();
        chunks.push(ConversationChunk {
            id: derive_chunk_id(conversation_id, chunk_index),
            chunk_index,
            text: current_lines.join("\n"),
        });
    }

    chunks
}

fn derive_chunk_id(conversation_id: &str, chunk_index: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(conversation_id.as_bytes());
    hasher.update(chunk_index.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn format_role(role: &str) -> &str {
    match role {
        "user" => "User",
        "assistant" => "Assistant",
        "tool" => "Tool",
        other => other,
    }
}
