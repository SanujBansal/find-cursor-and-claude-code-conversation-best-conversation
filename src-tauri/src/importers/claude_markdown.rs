use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    filters::apply_import_filters,
    normalizer::{conversation_id, normalize_text},
    types::{Conversation, Message},
};

const SOURCE_TYPE: &str = "claude-web-markdown";

/// Import all Claude web Markdown exports from the given folder.
pub fn import(folder_path: &str) -> (Vec<Conversation>, Vec<String>) {
    let dir = PathBuf::from(folder_path);
    let mut conversations = Vec::new();
    let mut errors = Vec::new();

    let entries = match fs::read_dir(&dir) {
        Ok(d) => d,
        Err(e) => {
            errors.push(format!("Cannot read {}: {e}", dir.display()));
            return (conversations, errors);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        match import_markdown_file(&path) {
            Ok(Some(conv)) => conversations.push(conv),
            Ok(None) => {}
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }

    (apply_import_filters(conversations), errors)
}

fn import_markdown_file(path: &Path) -> Result<Option<Conversation>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let path_str = path.to_string_lossy().to_string();

    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&path_str)
        .to_string();

    let title = session_id
        .replace('-', " ")
        .replace('_', " ");

    let messages = parse_messages(&text);
    if messages.is_empty() || !messages.iter().any(|m| m.role == "user") {
        return Ok(None);
    }

    let project_path = path
        .parent()
        .map(|p| p.to_string_lossy().to_string());

    let id = conversation_id(SOURCE_TYPE, &path_str, &session_id);
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

/// Parse a Claude web Markdown export from raw content and a logical filename.
/// Returns a vec of at most one Conversation; empty when no messages are found.
/// Exposed for testing and direct use without touching the filesystem.
#[cfg(test)]
pub fn parse_markdown_content(filename: &str, text: &str) -> Vec<Conversation> {
    let session_id = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename)
        .to_string();
    let title = session_id.replace('-', " ").replace('_', " ");
    let messages = parse_messages(text);
    if messages.is_empty() {
        return vec![];
    }
    let id = conversation_id(SOURCE_TYPE, filename, &session_id);
    let started_at = messages.first().and_then(|m| m.timestamp.clone());
    let ended_at = messages.last().and_then(|m| m.timestamp.clone());
    vec![Conversation {
        id,
        source_type: SOURCE_TYPE.to_string(),
        title,
        project_path: None,
        started_at,
        ended_at,
        messages,
    }]
}

/// Parse a Claude web Markdown export into a list of messages.
///
/// Expected format:
/// ```markdown
/// ## Human (2024-01-15T12:00:00Z):
/// message content
///
/// ---
///
/// ## Claude:
/// response content
/// ```
fn parse_messages(text: &str) -> Vec<Message> {
    let mut messages = Vec::new();

    // Split on `---` separators between turns
    let blocks: Vec<&str> = text.split("\n---\n").collect();

    for block in blocks {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }

        let Some(msg) = parse_block(block) else {
            continue;
        };

        if !msg.content.is_empty() {
            messages.push(msg);
        }
    }

    messages
}

fn parse_block(block: &str) -> Option<Message> {
    let mut lines = block.lines().peekable();

    // Skip leading lines that are not role headers (e.g. a document title).
    let (role, timestamp) = loop {
        let line = lines.next()?.trim();
        if let Some(parsed) = parse_header(line) {
            break parsed;
        }
    };

    let content_lines: Vec<&str> = lines.collect();
    let content = normalize_text(&content_lines.join("\n"));

    Some(Message {
        role,
        content,
        timestamp,
        tool_calls: vec![],
    })
}

/// Parse `## Human (2024-01-15T12:00:00Z):` or `## Claude:` headers.
fn parse_header(header: &str) -> Option<(String, Option<String>)> {
    // Strip markdown heading prefix
    let stripped = header.trim_start_matches('#').trim();

    if let Some(rest) = stripped
        .to_lowercase()
        .strip_prefix("human")
        .map(|_| &stripped["Human".len()..])
    {
        let rest = rest.trim();
        let timestamp = if rest.starts_with('(') {
            rest.strip_prefix('(')
                .and_then(|s| s.split(')').next())
                .map(|ts| ts.trim().to_string())
                .filter(|ts| !ts.is_empty())
        } else {
            None
        };
        return Some(("user".to_string(), timestamp));
    }

    let lower = stripped.to_lowercase();
    if lower.starts_with("claude") || lower.starts_with("assistant") {
        return Some(("assistant".to_string(), None));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_and_claude_sections() {
        let md = r#"# Conversation with Claude

## Human (May 27, 2026, 10:00 AM):

How do I sort a Vec in Rust?

---

## Claude:

Use `.sort()` for in-place or `.sort_by()` for custom ordering.

---

## Human:

Thanks!

---

## Claude:

You're welcome!
"#;
        let convs = parse_markdown_content("test-file.md", md);
        assert_eq!(convs.len(), 1);
        let messages = &convs[0].messages;
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
        assert!(messages[0].content.contains("sort a Vec"));
    }
}
