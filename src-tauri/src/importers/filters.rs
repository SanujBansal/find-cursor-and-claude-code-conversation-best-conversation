use super::types::Conversation;

/// Keep conversations that contain at least one user message.
pub fn has_user_message(conversation: &Conversation) -> bool {
    conversation
        .messages
        .iter()
        .any(|m| m.role == "user")
}

/// Remove conversations without user messages.
pub fn filter_with_user_messages(conversations: Vec<Conversation>) -> Vec<Conversation> {
    conversations
        .into_iter()
        .filter(has_user_message)
        .collect()
}

/// Apply standard import filters: require user messages.
pub fn apply_import_filters(conversations: Vec<Conversation>) -> Vec<Conversation> {
    filter_with_user_messages(conversations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importers::types::Message;

    fn conv(project: &str, ended_at: &str, user: bool) -> Conversation {
        let messages = if user {
            vec![Message {
                role: "user".to_string(),
                content: "hello".to_string(),
                timestamp: None,
                tool_calls: vec![],
            }]
        } else {
            vec![Message {
                role: "assistant".to_string(),
                content: "hi".to_string(),
                timestamp: None,
                tool_calls: vec![],
            }]
        };

        Conversation {
            id: format!("{project}-{ended_at}"),
            source_type: "cursor-local".to_string(),
            title: project.to_string(),
            project_path: Some(project.to_string()),
            started_at: None,
            ended_at: Some(ended_at.to_string()),
            messages,
        }
    }

    #[test]
    fn drops_conversations_without_user_messages() {
        let input = vec![conv("/a", "2026-01-02", false), conv("/a", "2026-01-01", true)];
        let filtered = apply_import_filters(input);
        assert_eq!(filtered.len(), 1);
        assert!(has_user_message(&filtered[0]));
    }

    #[test]
    fn keeps_all_conversations_with_user_messages() {
        let mut input = Vec::new();
        for i in 0..60 {
            input.push(conv(
                "/proj",
                &format!("2026-01-{:02}", (i % 28) + 1),
                true,
            ));
        }
        let filtered = apply_import_filters(input);
        assert_eq!(filtered.len(), 60);
    }
}
