use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use log::{debug, info, warn};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;

use super::{
    filters::apply_import_filters,
    normalizer::{conversation_id, normalize_text},
    types::{Conversation, Message},
};

const SOURCE_TYPE: &str = "cursor-local";
const SESSION_NAMESPACE: &str = "composer-session";
const LOG_EVERY_N_COMPOSERS: usize = 50;
const LOG_EVERY_N_UPSERTS: usize = 100;

/// Return `state.vscdb` paths to scan for Cursor composer data.
/// Composer sessions and bubbles live in global storage; workspace copies are empty.
pub fn discover_vscdb_paths(override_data_path: Option<&str>) -> Vec<PathBuf> {
    let base = match override_data_path {
        Some(p) => PathBuf::from(p),
        None => {
            let Some(home) = dirs::home_dir() else {
                warn!("[cursor-import] Could not resolve home directory");
                return vec![];
            };
            home.join("Library")
                .join("Application Support")
                .join("Cursor")
                .join("User")
        }
    };

    info!("[cursor-import] Using Cursor data root {}", base.display());

    let global = base.join("globalStorage").join("state.vscdb");
    if global.exists() {
        info!(
            "[cursor-import] Will import composer data from global storage: {}",
            global.display()
        );
        vec![global]
    } else {
        warn!(
            "[cursor-import] Global state.vscdb not found at {} — no Cursor chats to import",
            global.display()
        );
        vec![]
    }
}

/// Return the default Cursor data path (the `User/` directory).
pub fn default_cursor_data_path() -> Option<String> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("Application Support")
            .join("Cursor")
            .join("User")
            .to_string_lossy()
            .to_string()
    })
}

/// Extract the real workspace path from `workspace.json` in the same directory as the vscdb.
fn workspace_path_for(vscdb: &Path) -> Option<String> {
    let workspace_json = vscdb.parent()?.join("workspace.json");
    let text = fs::read_to_string(&workspace_json).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("folder")
        .or_else(|| value.get("workspace"))
        .and_then(|v| v.as_str())
        .map(|s| s.strip_prefix("file://").unwrap_or(s).to_string())
}

/// Import all Cursor conversations reachable from the given data path (or auto-detected default).
pub fn import(override_data_path: Option<&str>) -> (Vec<Conversation>, Vec<String>) {
    let started = Instant::now();
    let vscdb_paths = discover_vscdb_paths(override_data_path);
    let mut conversations_by_session: HashMap<String, Conversation> = HashMap::new();
    let mut errors = Vec::new();
    let mut seen_paths = 0usize;

    for vscdb_path in &vscdb_paths {
        seen_paths += 1;
        info!(
            "[cursor-import] Reading vscdb {}/{}: {}",
            seen_paths,
            vscdb_paths.len(),
            vscdb_path.display()
        );

        match import_from_vscdb(vscdb_path) {
            Ok(convs) => {
                info!(
                    "[cursor-import] Parsed {} conversation(s) from {}",
                    convs.len(),
                    vscdb_path.display()
                );

                for conv in convs {
                    conversations_by_session
                        .entry(conv.id.clone())
                        .and_modify(|existing| {
                            if existing.project_path.is_none() && conv.project_path.is_some() {
                                existing.project_path = conv.project_path.clone();
                            }
                            if conv.messages.len() > existing.messages.len() {
                                *existing = conv.clone();
                            }
                        })
                        .or_insert(conv);
                }
            }
            Err(e) => {
                warn!("[cursor-import] Failed to read {}: {e}", vscdb_path.display());
                errors.push(format!("{}: {e}", vscdb_path.display()));
            }
        }
    }

    let conversations: Vec<Conversation> = apply_import_filters(conversations_by_session.into_values().collect());
    info!(
        "[cursor-import] Finished scan in {:.1}s — {} conversation(s) after filters, {} error(s)",
        started.elapsed().as_secs_f64(),
        conversations.len(),
        errors.len()
    );

    (conversations, errors)
}

fn import_from_vscdb(vscdb_path: &Path) -> Result<Vec<Conversation>, String> {
    let started = Instant::now();
    let conn = Connection::open(vscdb_path)
        .map_err(|e| format!("Cannot open {}: {e}", vscdb_path.display()))?;

    let table_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='cursorDiskKV'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or(false);

    if !table_exists {
        info!(
            "[cursor-import] Skipping {} — no cursorDiskKV table",
            vscdb_path.display()
        );
        return Ok(vec![]);
    }

    let project_path = workspace_path_for(vscdb_path);
    if let Some(path) = &project_path {
        debug!("[cursor-import] Workspace path for {}: {path}", vscdb_path.display());
    }

    let mut stmt = conn
        .prepare(
            "SELECT key, value FROM cursorDiskKV
             WHERE key LIKE 'composerData:%'",
        )
        .map_err(|e| e.to_string())?;

    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        info!(
            "[cursor-import] No composer sessions in {} — nothing to import",
            vscdb_path.display()
        );
        return Ok(vec![]);
    }

    info!(
        "[cursor-import] Found {} composer session(s) in {}",
        rows.len(),
        vscdb_path.display()
    );

    let bubble_index_started = Instant::now();
    let bubbles_by_composer = load_all_bubbles(&conn)?;
    info!(
        "[cursor-import] Indexed {} bubble row(s) across {} composer(s) in {:.1}s ({})",
        bubbles_by_composer
            .values()
            .map(|rows| rows.len())
            .sum::<usize>(),
        bubbles_by_composer.len(),
        bubble_index_started.elapsed().as_secs_f64(),
        vscdb_path.display()
    );

    let total_composers = rows.len();
    let mut conversations = Vec::new();
    let mut skipped_empty = 0usize;

    for (index, (key, value_str)) in rows.into_iter().enumerate() {
        if index > 0 && index % LOG_EVERY_N_COMPOSERS == 0 {
            info!(
                "[cursor-import] Progress {index}/{total_composers} composer(s) in {} ({:.1}s elapsed)",
                vscdb_path.display(),
                started.elapsed().as_secs_f64()
            );
        }

        let composer_id = key
            .strip_prefix("composerData:")
            .unwrap_or(&key)
            .to_string();

        let session_value: Value = match serde_json::from_str(&value_str) {
            Ok(v) => v,
            Err(e) => {
                debug!(
                    "[cursor-import] Skipping composer {} — invalid JSON: {e}",
                    composer_id
                );
                continue;
            }
        };

        if session_value.is_null() {
            skipped_empty += 1;
            continue;
        }

        let title = session_value
            .get("name")
            .or_else(|| session_value.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled Cursor Session")
            .to_string();

        let bubble_rows = bubbles_by_composer
            .get(&composer_id)
            .cloned()
            .unwrap_or_default();

        if bubble_rows.is_empty() {
            skipped_empty += 1;
            continue;
        }

        let messages = parse_messages_from_bubbles(&bubble_rows);
        if messages.is_empty() || !messages.iter().any(|m| m.role == "user") {
            skipped_empty += 1;
            continue;
        }

        let resolved_project_path = project_path
            .clone()
            .or_else(|| extract_project_path_from_bubbles(&bubble_rows));

        let id = conversation_id(SOURCE_TYPE, SESSION_NAMESPACE, &composer_id);
        conversations.push(Conversation {
            id,
            source_type: SOURCE_TYPE.to_string(),
            title,
            project_path: resolved_project_path,
            started_at: messages.first().and_then(|m| m.timestamp.clone()),
            ended_at: messages.last().and_then(|m| m.timestamp.clone()),
            messages,
        });
    }

    info!(
        "[cursor-import] Parsed {} conversation(s) from {} in {:.1}s (skipped {} empty composer(s))",
        conversations.len(),
        vscdb_path.display(),
        started.elapsed().as_secs_f64(),
        skipped_empty
    );

    Ok(conversations)
}

fn load_all_bubbles(
    conn: &Connection,
) -> Result<HashMap<String, Vec<(String, String)>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT key, value FROM cursorDiskKV
             WHERE key LIKE 'bubbleId:%'
             ORDER BY rowid ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    let mut by_composer: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut unparsed = 0usize;

    for (key, value) in rows {
        let Some(composer_id) = parse_composer_id_from_bubble_key(&key) else {
            unparsed += 1;
            continue;
        };
        by_composer
            .entry(composer_id)
            .or_default()
            .push((key, value));
    }

    if unparsed > 0 {
        debug!("[cursor-import] Skipped {unparsed} bubble key(s) with unexpected format");
    }

    Ok(by_composer)
}

fn parse_composer_id_from_bubble_key(key: &str) -> Option<String> {
    let rest = key.strip_prefix("bubbleId:")?;
    let (composer_id, _bubble_id) = rest.split_once(':')?;
    Some(composer_id.to_string())
}

fn parse_messages_from_bubbles(bubble_rows: &[(String, String)]) -> Vec<Message> {
    let mut messages = Vec::with_capacity(bubble_rows.len());

    for (_key, bubble_str) in bubble_rows {
        if bubble_str.is_empty() {
            continue;
        }

        let bubble: Value = match serde_json::from_str(bubble_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let role = role_from_bubble_type(&bubble);

        let content = bubble
            .get("text")
            .or_else(|| bubble.get("rawText"))
            .or_else(|| bubble.get("content"))
            .or_else(|| bubble.get("message"))
            .or_else(|| bubble.get("richText").and_then(|rt| rt.get("text")))
            .and_then(|v| v.as_str())
            .map(normalize_text)
            .unwrap_or_default();

        if content.is_empty() {
            continue;
        }

        let timestamp = bubble
            .get("createdAt")
            .or_else(|| bubble.get("timestamp"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let tool_calls: Vec<String> = bubble
            .get("toolCalls")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        tc.get("name")
                            .or_else(|| tc.get("function").and_then(|f| f.get("name")))
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        messages.push(Message {
            role,
            content,
            timestamp,
            tool_calls,
        });
    }

    messages
}

/// Walk bubble JSON for `file://` paths and pick the most referenced directory.
fn extract_project_path_from_bubbles(bubble_rows: &[(String, String)]) -> Option<String> {
    use std::collections::HashMap;

    let mut dir_counts: HashMap<String, usize> = HashMap::new();

    for (_key, bubble_str) in bubble_rows {
        let bubble: Value = match serde_json::from_str(bubble_str) {
            Ok(v) => v,
            Err(_) => continue,
        };
        collect_file_paths(&bubble, &mut dir_counts);
    }

    dir_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(path, _)| path)
}

fn collect_file_paths(value: &Value, dir_counts: &mut std::collections::HashMap<String, usize>) {
    match value {
        Value::String(s) => {
            if let Some(path) = normalize_file_uri(s) {
                if let Some(parent) = Path::new(&path).parent() {
                    let project = infer_project_root(parent);
                    *dir_counts.entry(project).or_default() += 1;
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_file_paths(item, dir_counts);
            }
        }
        Value::Object(map) => {
            for val in map.values() {
                collect_file_paths(val, dir_counts);
            }
        }
        _ => {}
    }
}

fn normalize_file_uri(raw: &str) -> Option<String> {
    let path = raw.strip_prefix("file://").unwrap_or(raw);
    if path.starts_with('/') && !path.contains("..") {
        Some(path.to_string())
    } else {
        None
    }
}

/// Climb from a file directory toward a likely project root.
fn infer_project_root(start: &Path) -> String {
    let markers = [
        "package.json",
        "Cargo.toml",
        "pyproject.toml",
        "go.mod",
        ".git",
        "pnpm-workspace.yaml",
    ];

    let mut current = start.to_path_buf();
    for _ in 0..6 {
        for marker in markers {
            if current.join(marker).exists() {
                return current.to_string_lossy().to_string();
            }
        }
        if !current.pop() {
            break;
        }
    }

    start.to_string_lossy().to_string()
}

/// Cursor stores bubble `type` as a number (1 = user, 2 = assistant) or legacy strings.
fn role_from_bubble_type(bubble: &Value) -> String {
    let Some(type_value) = bubble.get("type") else {
        return "user".to_string();
    };

    match type_value {
        Value::Number(n) => {
            if n.as_i64() == Some(2) {
                "assistant".to_string()
            } else {
                "user".to_string()
            }
        }
        Value::String(s) => match s.as_str() {
            "2" | "ai" | "assistant" => "assistant".to_string(),
            "1" | "human" | "user" => "user".to_string(),
            "tool" => "tool".to_string(),
            _ => "user".to_string(),
        },
        _ => "user".to_string(),
    }
}

/// Log upsert progress for large Cursor imports.
pub fn log_upsert_progress(processed: usize, total: usize, imported: usize, skipped: usize) {
    if processed == total || processed % LOG_EVERY_N_UPSERTS == 0 {
        info!(
            "[cursor-import] Upsert progress {processed}/{total} — imported {imported}, skipped {skipped}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_numeric_bubble_types() {
        assert_eq!(role_from_bubble_type(&json!({"type": 1})), "user");
        assert_eq!(role_from_bubble_type(&json!({"type": 2})), "assistant");
    }

    #[test]
    fn maps_string_bubble_types() {
        assert_eq!(role_from_bubble_type(&json!({"type": "1"})), "user");
        assert_eq!(role_from_bubble_type(&json!({"type": "2"})), "assistant");
        assert_eq!(role_from_bubble_type(&json!({"type": "assistant"})), "assistant");
        assert_eq!(role_from_bubble_type(&json!({"type": "tool"})), "tool");
    }

    #[test]
    fn defaults_missing_type_to_user() {
        assert_eq!(role_from_bubble_type(&json!({})), "user");
    }

    #[test]
    fn parse_messages_assigns_roles_from_numeric_type() {
        let bubbles = vec![
            (
                "bubbleId:composer:1".to_string(),
                json!({"type": 1, "text": "Hello from user"}).to_string(),
            ),
            (
                "bubbleId:composer:2".to_string(),
                json!({"type": 2, "text": "Hello from assistant"}).to_string(),
            ),
        ];

        let messages = parse_messages_from_bubbles(&bubbles);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }
}
