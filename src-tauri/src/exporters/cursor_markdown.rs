use chrono::{DateTime, Local, Utc};

#[derive(Debug, Clone)]
pub struct ExportMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ConversationMarkdownExport {
    pub markdown: String,
    pub suggested_filename: String,
    pub provider: String,
}

/// Build a Cursor-style Markdown export from conversation metadata and messages.
pub fn build_conversation_markdown_export(
    title: &str,
    provider: &str,
    completed_at: Option<&str>,
    messages: &[ExportMessage],
) -> ConversationMarkdownExport {
    let blocks = merge_blocks(messages, provider);
    let markdown = render_markdown(title, provider, completed_at, &blocks);
    let suggested_filename = suggest_filename(title, provider);

    ConversationMarkdownExport {
        markdown,
        suggested_filename,
        provider: provider.to_string(),
    }
}

fn merge_blocks(messages: &[ExportMessage], provider: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();

    for message in messages {
        if message.content.trim().is_empty() {
            continue;
        }

        let label = role_label(&message.role, provider);
        if let Some((last_label, last_content)) = blocks.last_mut() {
            if *last_label == label {
                last_content.push_str("\n\n");
                last_content.push_str(&message.content);
                continue;
            }
        }
        blocks.push((label, message.content.clone()));
    }

    blocks
}

fn role_label(role: &str, provider: &str) -> String {
    match role {
        "user" => "User".to_string(),
        "assistant" => match provider {
            "cursor-local" => "Cursor".to_string(),
            "claude-code-local" | "claude-web-markdown" => "Claude".to_string(),
            _ => "Assistant".to_string(),
        },
        "tool" => "Tool".to_string(),
        other => {
            if other.is_empty() {
                "Unknown".to_string()
            } else {
                let mut chars = other.chars();
                let Some(first) = chars.next() else {
                    return other.to_string();
                };
                first.to_uppercase().collect::<String>() + chars.as_str()
            }
        }
    }
}

fn render_markdown(
    title: &str,
    provider: &str,
    completed_at: Option<&str>,
    blocks: &[(String, String)],
) -> String {
    let mut out = String::new();
    out.push_str(title.trim());
    out.push_str("\nExported on ");
    out.push_str(&format_export_timestamp(completed_at));
    out.push_str(" from ");
    out.push_str(source_name(provider));
    out.push_str(" via Vibe Score\n\n");

    for (label, content) in blocks {
        out.push_str(label);
        out.push_str("\n\n");
        out.push_str(content.trim());
        out.push_str("\n\n");
    }

    out.trim_end().to_string()
}

fn format_export_timestamp(completed_at: Option<&str>) -> String {
    if let Some(raw) = completed_at {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
            return parsed.with_timezone(&Local).format("%-m/%-d/%Y at %-I:%M:%S %p %Z").to_string();
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
            return parsed.format("%-m/%-d/%Y at %-I:%M:%S").to_string();
        }
    }

    Utc::now()
        .with_timezone(&Local)
        .format("%-m/%-d/%Y at %-I:%M:%S %p %Z")
        .to_string()
}

fn source_name(provider: &str) -> &'static str {
    match provider {
        "cursor-local" => "Cursor",
        "claude-code-local" => "Claude Code",
        "claude-web-markdown" => "Claude",
        _ => "Vibe Score",
    }
}

fn suggest_filename(title: &str, provider: &str) -> String {
    let mut slug: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                '-'
            }
        })
        .collect();

    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    slug = slug.trim_matches('-').to_string();

    if slug.is_empty() {
        slug = match provider {
            "cursor-local" => "cursor-conversation".to_string(),
            "claude-code-local" => "claude-code-conversation".to_string(),
            _ => "conversation".to_string(),
        };
    }

    format!("{slug}.md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_cursor_style_markdown() {
        let export = build_conversation_markdown_export(
            "Conversation rating system",
            "cursor-local",
            Some("2026-05-27T10:09:14.517Z"),
            &[
                ExportMessage {
                    role: "user".to_string(),
                    content: "How do I score transcripts?".to_string(),
                },
                ExportMessage {
                    role: "assistant".to_string(),
                    content: "Use a deterministic pipeline.".to_string(),
                },
            ],
        );

        assert!(export.markdown.starts_with("Conversation rating system\nExported on"));
        assert!(export.markdown.contains("from Cursor via Vibe Score"));
        assert!(export.markdown.contains("User\n\nHow do I score transcripts?"));
        assert!(export.markdown.contains("Cursor\n\nUse a deterministic pipeline."));
        assert_eq!(export.suggested_filename, "conversation-rating-system.md");
    }

    #[test]
    fn merges_consecutive_assistant_messages() {
        let export = build_conversation_markdown_export(
            "Test",
            "cursor-local",
            None,
            &[
                ExportMessage {
                    role: "assistant".to_string(),
                    content: "Step one.".to_string(),
                },
                ExportMessage {
                    role: "assistant".to_string(),
                    content: "Step two.".to_string(),
                },
            ],
        );

        assert_eq!(export.markdown.matches("Cursor\n\n").count(), 1);
        assert!(export.markdown.contains("Step one.\n\nStep two."));
    }
}
