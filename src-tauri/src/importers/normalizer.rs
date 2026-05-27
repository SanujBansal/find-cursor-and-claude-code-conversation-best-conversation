use sha2::{Digest, Sha256};

use super::types::Conversation;

/// Trim whitespace and collapse multiple consecutive blank lines into one.
pub fn normalize_text(text: &str) -> String {
    let trimmed = text.trim();
    let mut result = String::with_capacity(trimmed.len());
    let mut blank_run = 0usize;

    for line in trimmed.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                result.push('\n');
            }
        } else {
            blank_run = 0;
            result.push_str(line);
            result.push('\n');
        }
    }

    result.trim_end().to_string()
}

/// Sort messages in place by timestamp (when available), preserving insertion order for ties.
pub fn sort_messages(conversation: &mut Conversation) {
    conversation
        .messages
        .sort_by(|a, b| match (&a.timestamp, &b.timestamp) {
            (Some(ta), Some(tb)) => ta.cmp(tb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
}

/// Generate a stable sha256 content hash used as the cache key for scoring.
/// Format: sha256(source_type | "|" | conversation_id | "|" | all_message_content_joined)
pub fn content_hash(conversation: &Conversation) -> String {
    let mut hasher = Sha256::new();
    hasher.update(conversation.source_type.as_bytes());
    hasher.update(b"|");
    hasher.update(conversation.id.as_bytes());
    hasher.update(b"|");
    for msg in &conversation.messages {
        hasher.update(msg.role.as_bytes());
        hasher.update(b":");
        hasher.update(msg.content.as_bytes());
        hasher.update(b"\x00");
    }
    hex::encode(hasher.finalize())
}

/// Generate a deterministic conversation id from its source coordinates.
pub fn conversation_id(source_type: &str, path: &str, session_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_type.as_bytes());
    hasher.update(b"|");
    hasher.update(path.as_bytes());
    hasher.update(b"|");
    hasher.update(session_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// Used for testing and direct cache-key computation without a full Conversation struct.
#[cfg(test)]
pub fn compute_content_hash(source_type: &str, conv_id: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_type.as_bytes());
    hasher.update(b"|");
    hasher.update(conv_id.as_bytes());
    hasher.update(b"|");
    hasher.update(content.as_bytes());
    hasher.update(b"\x00");
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_stable() {
        let text = "  Hello\n\n\n  World  \n";
        let once = normalize_text(text);
        let twice = normalize_text(&once);
        assert_eq!(once, twice, "normalization must be idempotent");
    }

    #[test]
    fn content_hash_changes_with_content() {
        let h1 = compute_content_hash("cursor-local", "conv1", "hello world");
        let h2 = compute_content_hash("cursor-local", "conv1", "hello world modified");
        assert_ne!(h1, h2);
    }

    #[test]
    fn content_hash_changes_with_source_type() {
        let h1 = compute_content_hash("cursor-local", "conv1", "hello");
        let h2 = compute_content_hash("claude-code-local", "conv1", "hello");
        assert_ne!(h1, h2);
    }
}
