use sha2::{Digest, Sha256};

use crate::scoring::rubric::{RUBRIC_DESCRIPTION, RUBRIC_VERSION};

pub const PROMPT_VERSION: &str = "v2";

pub struct PromptInput {
    pub content_hash: String,
    pub messages: Vec<(String, String)>, // (role, content)
}

pub struct PromptOutput {
    pub cache_key: String,
    pub prompt: String,
}

/// Build a deterministic cache key from a content hash and model id.
/// Stable across restarts: sha256(content_hash || RUBRIC_VERSION || PROMPT_VERSION || model_id).
pub fn build_cache_key(content_hash: &str, model_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content_hash.as_bytes());
    hasher.update(RUBRIC_VERSION.as_bytes());
    hasher.update(PROMPT_VERSION.as_bytes());
    hasher.update(model_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// Build a deterministic scoring prompt for the given conversation.
/// Transcript is truncated to at most 20 messages (first 10 + last 10 when
/// the conversation is longer) to stay within ~4 000 tokens.
pub fn build_prompt(input: &PromptInput, model_id: &str) -> PromptOutput {
    let messages = truncate_messages(&input.messages);

    let transcript = messages
        .iter()
        .map(|(role, content)| format!("[{}]: {}", role.to_uppercase(), content.trim()))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "You are a strict, evidence-driven reviewer. Default each dimension to 2 \
         and only raise a score when the transcript contains concrete, specific \
         evidence to justify it. A 5 is reserved for exceptional work and should \
         be rare. Do not be polite — be accurate.\n\n\
         ## Rubric\n{}\n\n## Transcript to Score\n\n{}\n\n\
         Before answering, silently ask yourself for EACH dimension:\n\
         - What specific evidence in the transcript supports this score?\n\
         - What weaknesses am I overlooking?\n\
         - Would a senior engineer reviewing this work agree?\n\
         If you can't cite evidence, the score is too high.\n\n\
         Return JSON only. Score this transcript independently of any others.",
        RUBRIC_DESCRIPTION, transcript
    );

    let cache_key = build_cache_key(&input.content_hash, model_id);

    PromptOutput { cache_key, prompt }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_changes_with_content_hash() {
        let k1 = build_cache_key("hash1", "gpt-4o-mini");
        let k2 = build_cache_key("hash2", "gpt-4o-mini");
        assert_ne!(k1, k2);
    }

    #[test]
    fn cache_key_changes_with_model() {
        let k1 = build_cache_key("hash1", "gpt-4o-mini");
        let k2 = build_cache_key("hash1", "gpt-4o");
        assert_ne!(k1, k2);
    }
}

/// Keep the first 10 and last 10 messages when transcript exceeds 20 messages.
fn truncate_messages(messages: &[(String, String)]) -> Vec<(String, String)> {
    const MAX: usize = 20;
    const HALF: usize = MAX / 2;

    if messages.len() <= MAX {
        return messages.to_vec();
    }

    let mut result = Vec::with_capacity(MAX + 1);
    result.extend_from_slice(&messages[..HALF]);
    result.push((
        "system".to_string(),
        "[... middle of conversation omitted for brevity ...]".to_string(),
    ));
    result.extend_from_slice(&messages[messages.len() - HALF..]);
    result
}
