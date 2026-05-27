use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::{
    filters::apply_import_filters,
    normalizer::{conversation_id, normalize_text, sort_messages},
    types::{Conversation, Message},
};

const SOURCE_TYPE: &str = "claude-code-local";

/// Candidate base directories where Claude Code stores project transcripts.
fn candidate_bases() -> Vec<PathBuf> {
    let mut bases = Vec::new();
    if let Some(home) = dirs::home_dir() {
        bases.push(home.join(".claude").join("projects"));
        bases.push(home.join(".config").join("claude").join("projects"));
    }
    bases
}

/// Return the first base directory that exists, or None.
fn resolve_base(override_path: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = override_path {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
        return None;
    }
    candidate_bases().into_iter().find(|p| p.exists())
}

/// Import all Claude Code conversations from JSONL files.
pub fn import(override_path: Option<&str>) -> (Vec<Conversation>, Vec<String>) {
    let Some(base) = resolve_base(override_path) else {
        return (vec![], vec![]);
    };

    let mut conversations = Vec::new();
    let mut errors = Vec::new();
    collect_jsonl_files(&base, &mut |path| {
        match import_jsonl(path) {
            Ok(conv) => {
                if let Some(c) = conv {
                    conversations.push(c);
                }
            }
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    });

    (apply_import_filters(conversations), errors)
}

/// Recursively collect `.jsonl` files under `dir`.
fn collect_jsonl_files(dir: &Path, visit: &mut dyn FnMut(&Path)) {
    let entries = match fs::read_dir(dir) {
        Ok(d) => d,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, visit);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            visit(&path);
        }
    }
}

fn import_jsonl(path: &Path) -> Result<Option<Conversation>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let path_str = path.to_string_lossy().to_string();

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&path_str)
        .to_string();

    let mut messages: Vec<Message> = Vec::new();
    let mut project_path: Option<String> = None;
    let mut custom_title: Option<String> = None;

    for (line_idx, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line).map_err(|e| {
            format!("line {}: invalid JSON: {e}", line_idx + 1)
        })?;

        if let Some(title) = extract_custom_title(&value) {
            custom_title = Some(title);
        }

        if project_path.is_none() {
            project_path = value
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }

        if let Some(message) = parse_entry(&value) {
            messages.push(message);
        }
    }

    if messages.is_empty() || !messages.iter().any(|m| m.role == "user") {
        return Ok(None);
    }

    if project_path.is_none() {
        project_path = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .and_then(decode_project_slug);
    }

    sort_messages(&mut Conversation {
        id: String::new(),
        source_type: String::new(),
        title: String::new(),
        project_path: None,
        started_at: None,
        ended_at: None,
        messages: messages.clone(),
    });

    let id = conversation_id(SOURCE_TYPE, &path_str, &session_id);

    let title = derive_title(
        custom_title.as_deref(),
        messages
            .iter()
            .find(|m| m.role == "user" && !is_local_command_caveat(&m.content))
            .or_else(|| messages.iter().find(|m| m.role == "user")),
        &session_id,
    );

    let started_at = messages.first().and_then(|m| m.timestamp.clone());
    let ended_at = messages.last().and_then(|m| m.timestamp.clone());

    Ok(Some(Conversation {
        id,
        source_type: SOURCE_TYPE.to_string(),
        title,
        project_path,
        started_at,
        ended_at,
        messages,
    }))
}

/// Parse a single JSONL entry into a chat message, if applicable.
fn parse_entry(value: &Value) -> Option<Message> {
    // Current Claude Code format: { type: "user"|"assistant", message: { role, content } }
    if let Some(entry_type) = value.get("type").and_then(|v| v.as_str()) {
        if entry_type == "user" || entry_type == "assistant" {
            let msg_obj = value.get("message")?;
            let role = msg_obj
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or(entry_type)
                .to_string();
            let content = extract_content(msg_obj);
            let tool_calls = extract_tool_calls(msg_obj);
            let timestamp = value
                .get("timestamp")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if content.is_empty() && tool_calls.is_empty() {
                return None;
            }

            return Some(Message {
                role,
                content,
                timestamp,
                tool_calls,
            });
        }
        return None;
    }

    // Legacy flat format used in early fixtures: { role, content }
    if value.get("role").is_some() {
        let role = value
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("user")
            .to_string();
        let content = extract_content(value);
        if content.is_empty() {
            return None;
        }
        let timestamp = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let tool_calls = extract_tool_calls(value);

        return Some(Message {
            role,
            content,
            timestamp,
            tool_calls,
        });
    }

    None
}

fn decode_project_slug(slug: &str) -> Option<String> {
    if slug.is_empty() || !slug.starts_with('-') {
        return None;
    }
    Some(slug.replace('-', "/"))
}

fn extract_custom_title(value: &Value) -> Option<String> {
    if value.get("type").and_then(|t| t.as_str()) != Some("custom-title") {
        return None;
    }

    value
        .get("title")
        .or_else(|| value.get("customTitle"))
        .or_else(|| value.get("sessionTitle"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn derive_title(
    custom_title: Option<&str>,
    first_user_message: Option<&Message>,
    session_id: &str,
) -> String {
    if let Some(title) = custom_title.filter(|t| !t.trim().is_empty()) {
        return truncate_title(title, 80);
    }

    if let Some(message) = first_user_message.filter(|m| !m.content.trim().is_empty()) {
        let cleaned = strip_local_command_caveat(&message.content);
        if !cleaned.is_empty() {
            return truncate_title(&cleaned, 80);
        }
    }

    if !looks_like_uuid(session_id) {
        return session_id.to_string();
    }

    "Claude Code Session".to_string()
}

fn looks_like_uuid(value: &str) -> bool {
    value.len() == 36 && value.chars().filter(|c| *c == '-').count() == 4
}

fn is_local_command_caveat(content: &str) -> bool {
    content.starts_with("<local-command-caveat>")
}

fn strip_local_command_caveat(content: &str) -> String {
    if !is_local_command_caveat(content) {
        return content.trim().to_string();
    }

    content
        .split_once("</local-command-caveat>")
        .map(|(_, rest)| rest.trim().to_string())
        .unwrap_or_default()
}

fn truncate_title(text: &str, max_len: usize) -> String {
    let single_line = text.lines().next().unwrap_or(text).trim();
    if single_line.len() <= max_len {
        single_line.to_string()
    } else {
        format!("{}…", &single_line[..max_len.saturating_sub(1)])
    }
}

/// Extract textual content from a message value.
/// Claude Code may store content as a string, or as an array of content blocks.
fn extract_content(value: &Value) -> String {
    match value.get("content") {
        Some(Value::String(s)) => normalize_text(s),
        Some(Value::Array(blocks)) => {
            let text = blocks
                .iter()
                .filter_map(|block| match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .map(|t| t.to_string()),
                    Some("thinking") => None,
                    Some("tool_use") => block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|name| format!("[Tool: {name}]")),
                    Some("tool_result") => block
                        .get("content")
                        .and_then(|c| c.as_str())
                        .map(|c| format!("[Tool result]\n{c}")),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            normalize_text(&text)
        }
        _ => String::new(),
    }
}

fn extract_tool_calls(value: &Value) -> Vec<String> {
    let mut names = Vec::new();

    if let Some(Value::Array(blocks)) = value.get("content") {
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                    names.push(name.to_string());
                }
            }
        }
    }

    if names.is_empty() {
        if let Some(arr) = value
            .get("toolUse")
            .and_then(|v| v.as_array())
            .or_else(|| value.get("tool_calls").and_then(|v| v.as_array()))
        {
            for tc in arr {
                if let Some(name) = tc.get("name").and_then(|n| n.as_str()) {
                    names.push(name.to_string());
                }
            }
        }
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_flat_format() {
        let text = include_str!("../../tests/fixtures/claude_code_session.jsonl");
        let mut messages = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(line).unwrap();
            if let Some(msg) = parse_entry(&value) {
                messages.push(msg);
            }
        }
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    #[test]
    fn parses_current_claude_code_format() {
        let line = r#"{"parentUuid":null,"type":"user","message":{"role":"user","content":"Add auth middleware"},"uuid":"abc","timestamp":"2026-05-19T16:12:56.336Z","cwd":"/Users/dev/my-app"}"#;
        let value: Value = serde_json::from_str(line).unwrap();
        let msg = parse_entry(&value).expect("user message");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Add auth middleware");
        assert_eq!(msg.timestamp.as_deref(), Some("2026-05-19T16:12:56.336Z"));

        let assistant_line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'll add middleware."},{"type":"tool_use","id":"t1","name":"Read","input":{}}]},"timestamp":"2026-05-19T16:13:00.000Z"}"#;
        let value: Value = serde_json::from_str(assistant_line).unwrap();
        let msg = parse_entry(&value).expect("assistant message");
        assert_eq!(msg.role, "assistant");
        assert!(msg.content.contains("I'll add middleware."));
        assert_eq!(msg.tool_calls, vec!["Read".to_string()]);
    }

    #[test]
    fn skips_non_message_entry_types() {
        let line = r#"{"type":"queue-operation","content":"Say OK"}"#;
        let value: Value = serde_json::from_str(line).unwrap();
        assert!(parse_entry(&value).is_none());
    }
}
