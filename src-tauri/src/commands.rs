use crate::{
    analytics::aggregates::{compute_weekly_score, effort_weight, weighted_average},
    azure::AzureOpenAIConfig,
    db::Database,
    importers::{
        self,
        types::{ImportResult, JobStatus},
    },
    llm::{self, ai_provider, LlmConfig, LlmSettings},
    rules::{
        scan_project_rules as scanner_scan, score_project_rules_with_llm, ProjectRulesReport,
        ProjectRulesScore, PROJECT_RULES_RUBRIC_VERSION,
    },
    scoring::{
        prompt::PROMPT_VERSION,
        rubric::RUBRIC_VERSION,
        scorer::{ConversationForScoring, ScoringResult},
    },
};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Azure OpenAI endpoint URL override (falls back to `.env`).
    #[serde(default)]
    pub azure_endpoint: String,
    /// Azure OpenAI API key override (falls back to `.env`).
    #[serde(default, alias = "openaiApiKey")]
    pub azure_api_key: String,
    /// Direct OpenAI API key override (falls back to OPENAI_API_KEY from `.env`).
    #[serde(default)]
    pub open_ai_api_key: String,
    pub scoring_model: String,
    pub cursor_data_path: String,
    pub claude_code_path: String,
    pub claude_markdown_path: String,
    #[serde(default)]
    pub azure_configured: bool,
    #[serde(default)]
    pub open_ai_configured: bool,
    #[serde(default)]
    pub ai_provider: String,
    #[serde(default)]
    pub azure_env_path: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            azure_endpoint: String::new(),
            azure_api_key: String::new(),
            open_ai_api_key: String::new(),
            scoring_model: "gpt-4.1-mini".to_string(),
            cursor_data_path: String::new(),
            claude_code_path: String::new(),
            claude_markdown_path: String::new(),
            azure_configured: false,
            open_ai_configured: false,
            ai_provider: String::new(),
            azure_env_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: i64,
    pub title: String,
    pub provider: String,
    pub project_name: Option<String>,
    pub source_path: Option<String>,
    pub final_score: Option<f64>,
    pub completed_at: Option<String>,
    pub user_message_count: i64,
    pub tool_call_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyTrendPoint {
    pub week_label: String, // e.g. "2026-W21"
    pub score: f64,
    pub active_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakRubric {
    pub dimension: String,  // e.g. "conceptualKnowledge"
    pub average_score: f64,
    pub label: String,      // human-readable, e.g. "Conceptual Knowledge"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub date: String, // "YYYY-MM-DD"
    pub score: f64,
    pub conversation_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    pub today_score: Option<f64>,
    pub daily_delta: Option<f64>,     // today vs previous active day
    pub week_score: Option<f64>,
    pub weekly_delta: Option<f64>,    // this week vs last week
    pub rolling_7d: Option<f64>,
    pub weekly_trend: Vec<WeeklyTrendPoint>,
    pub top_conversations: Vec<ConversationWithScore>,
    pub weakest_rubrics: Vec<WeakRubric>,
    pub total_conversations: i64,
    pub total_scored: i64,
}

const SETTINGS_KEY: &str = "app_settings";

#[tauri::command]
pub async fn get_dashboard(db: tauri::State<'_, Database>) -> Result<DashboardData, String> {
    let conn = db.raw();
    tokio::task::spawn_blocking(move || Database::run_with(conn, |conn| {
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        let week_start = start_of_week(Utc::now().date_naive())
            .format("%Y-%m-%d")
            .to_string();

        // Today's score from daily_scores
        let today_score: Option<f64> = conn
            .query_row(
                "SELECT average_score FROM daily_scores WHERE score_date = ?1",
                [&today],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        // Previous active day's score
        let previous_day: Option<f64> = conn
            .query_row(
                "SELECT average_score FROM daily_scores
                 WHERE score_date < ?1
                 ORDER BY score_date DESC
                 LIMIT 1",
                [&today],
                |row| row.get::<_, f64>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        // This week's score
        let week_score: Option<f64> = conn
            .query_row(
                "SELECT average_score FROM weekly_scores WHERE week_start = ?1",
                [&week_start],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        // Previous week's score
        let previous_week: Option<f64> = conn
            .query_row(
                "SELECT average_score FROM weekly_scores
                 WHERE week_start < ?1
                 ORDER BY week_start DESC
                 LIMIT 1",
                [&week_start],
                |row| row.get::<_, f64>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        // Rolling 7-day average
        let rolling_7d: Option<f64> = conn
            .query_row(
                "SELECT AVG(average_score) FROM (
                     SELECT average_score FROM daily_scores
                     ORDER BY score_date DESC
                     LIMIT 7
                 )",
                [],
                |row| row.get::<_, Option<f64>>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();

        // Last 8 weeks of weekly trend (oldest → newest)
        let mut trend_stmt = conn
            .prepare(
                "SELECT week_start, average_score, active_days
                 FROM weekly_scores
                 ORDER BY week_start DESC
                 LIMIT 8",
            )
            .map_err(|e| e.to_string())?;

        let mut weekly_trend: Vec<WeeklyTrendPoint> = trend_stmt
            .query_map([], |row| {
                let ws: String = row.get(0)?;
                let score: f64 = row.get(1)?;
                let active_days: i64 = row.get(2)?;
                Ok((ws, score, active_days))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(ws, score, active_days)| WeeklyTrendPoint {
                week_label: week_label_from_start(&ws),
                score,
                active_days,
            })
            .collect();
        weekly_trend.reverse(); // chronological order

        // Top 3 conversations by score
        let mut top_stmt = conn
            .prepare(
                "SELECT
                    c.id, c.title, c.provider, p.name, c.source_path,
                    s.final_score, c.completed_at, c.message_count, c.tool_call_count,
                    s.conceptual_knowledge, s.attention_to_detail, s.problem_decomposition,
                    s.critical_evaluation, s.robustness_awareness, s.debugging_skill,
                    s.prompt_specificity, s.scope_discipline,
                    s.explanation, s.model_id, s.scored_at
                 FROM conversations c
                 LEFT JOIN projects p ON p.id = c.project_id
                 LEFT JOIN scores s ON s.conversation_id = c.id
                 ORDER BY s.final_score DESC, c.completed_at DESC, c.id ASC
                 LIMIT 3",
            )
            .map_err(|e| e.to_string())?;

        let top_conversations = top_stmt
            .query_map([], |row| {
                let project_name: Option<String> = row.get(3)?;
                let source_path: Option<String> = row.get(4)?;
                Ok(ConversationWithScore {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    provider: row.get(2)?,
                    project_name: display_project_name(project_name, source_path.clone()),
                    source_path,
                    final_score: row.get(5)?,
                    completed_at: row.get(6)?,
                    message_count: row.get(7)?,
                    tool_call_count: row.get(8)?,
                    conceptual_knowledge: row.get(9)?,
                    attention_to_detail: row.get(10)?,
                    problem_decomposition: row.get(11)?,
                    critical_evaluation: row.get(12)?,
                    robustness_awareness: row.get(13)?,
                    debugging_skill: row.get(14)?,
                    prompt_specificity: row.get(15)?,
                    scope_discipline: row.get(16)?,
                    explanation: row.get(17)?,
                    model_id: row.get(18)?,
                    scored_at: row.get(19)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        // Weakest rubric dimensions (last 30 days)
        let weakest_rubrics = compute_weak_rubrics(conn)?;

        // Total counts
        let total_conversations: i64 = conn
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;

        let total_scored: i64 = conn
            .query_row("SELECT COUNT(*) FROM scores", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;

        Ok(DashboardData {
            today_score,
            daily_delta: match (today_score, previous_day) {
                (Some(curr), Some(prev)) => Some(curr - prev),
                _ => None,
            },
            week_score,
            weekly_delta: match (week_score, previous_week) {
                (Some(curr), Some(prev)) => Some(curr - prev),
                _ => None,
            },
            rolling_7d,
            weekly_trend,
            top_conversations,
            weakest_rubrics,
            total_conversations,
            total_scored,
        })
    }))
    .await
    .map_err(|e| format!("get_dashboard panicked: {e}"))?
}

#[tauri::command]
pub async fn list_conversations(db: tauri::State<'_, Database>) -> Result<Vec<ConversationSummary>, String> {
    let conn = db.raw();
    tokio::task::spawn_blocking(move || Database::run_with(conn, |connection| {
        // Only surface final_score when the stored score matches the current
        // rubric/prompt versions AND the current conversation content. Stale
        // scores show up as `null` so the UI's "unscored" treatment kicks in
        // and the user is invited to re-score them.
        let mut stmt = connection
            .prepare(
                "SELECT
                    c.id,
                    c.title,
                    c.provider,
                    p.name,
                    c.source_path,
                    CASE
                        WHEN s.id IS NULL THEN NULL
                        WHEN s.rubric_version != ?1 THEN NULL
                        WHEN s.prompt_version != ?2 THEN NULL
                        WHEN s.content_hash  != c.content_hash THEN NULL
                        ELSE s.final_score
                    END AS final_score,
                    c.completed_at,
                    (SELECT COUNT(*) FROM messages m
                     WHERE m.conversation_id = c.id AND m.role = 'user'),
                    c.tool_call_count
                 FROM conversations c
                 LEFT JOIN projects p ON p.id = c.project_id
                 LEFT JOIN scores s ON s.conversation_id = c.id
                 ORDER BY c.completed_at DESC, c.id ASC",
            )
            .map_err(|error| error.to_string())?;

        let conversations = stmt
            .query_map(params![RUBRIC_VERSION, PROMPT_VERSION], |row| {
                let project_name: Option<String> = row.get(3)?;
                let source_path: Option<String> = row.get(4)?;
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    provider: row.get(2)?,
                    project_name: display_project_name(project_name, source_path.clone()),
                    source_path,
                    final_score: row.get(5)?,
                    completed_at: row.get(6)?,
                    user_message_count: row.get(7)?,
                    tool_call_count: row.get(8)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;

        Ok(conversations)
    }))
    .await
    .map_err(|e| format!("list_conversations panicked: {e}"))?
}

#[tauri::command]
pub fn get_settings(db: tauri::State<'_, Database>) -> Result<AppSettings, String> {
    let settings = read_settings(&db)?;
    Ok(enrich_settings_from_azure(settings))
}

#[tauri::command]
pub fn save_settings(
    db: tauri::State<'_, Database>,
    settings: AppSettings,
) -> Result<(), String> {
    db.with_connection(|connection| {
        let json = serde_json::to_string(&settings).map_err(|error| error.to_string())?;
        let updated_at = Utc::now().to_rfc3339();

        connection
            .execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = excluded.updated_at",
                params![SETTINGS_KEY, json, updated_at],
            )
            .map_err(|error| error.to_string())?;

        Ok(())
    })
}

fn read_settings(db: &Database) -> Result<AppSettings, String> {
    db.with_connection(|connection| {
        let stored: Option<String> = connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [SETTINGS_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        match stored {
            Some(json) => serde_json::from_str(&json).map_err(|error| error.to_string()),
            None => Ok(AppSettings::default()),
        }
    })
}

fn enrich_settings_from_azure(mut settings: AppSettings) -> AppSettings {
    settings.azure_env_path = ".env".to_string();

    let env_config = AzureOpenAIConfig::load().ok();
    let llm_settings = llm_settings_from_app(&settings);

    if settings.azure_endpoint.trim().is_empty() {
        if let Some(ref config) = env_config {
            settings.azure_endpoint = config.endpoint.clone();
        }
    }

    if let Some(ref config) = env_config {
        if settings.scoring_model.is_empty() {
            settings.scoring_model = config.chat_deployment.clone();
        }
    }

    settings.azure_configured =
        llm::azure_credentials_available(&llm_settings, env_config.as_ref());
    settings.open_ai_configured = llm::openai_credentials_available(&llm_settings);
    settings.ai_provider = ai_provider(&llm_settings).to_string();
    settings
}

fn llm_settings_from_app(settings: &AppSettings) -> LlmSettings {
    LlmSettings {
        azure_endpoint: settings.azure_endpoint.clone(),
        azure_api_key: settings.azure_api_key.clone(),
        open_ai_api_key: settings.open_ai_api_key.clone(),
        scoring_model: settings.scoring_model.clone(),
    }
}

fn resolve_llm_config(
    db: &Database,
    api_key_override: &str,
    model_override: Option<String>,
) -> Result<LlmConfig, String> {
    let settings = read_settings(db)?;
    llm::resolve_llm_config(&llm_settings_from_app(&settings), api_key_override, model_override)
}

/// Returns bottom-3 WeakRubric entries averaged over the last 30 days (for dashboard).
fn compute_weak_rubrics(conn: &rusqlite::Connection) -> Result<Vec<WeakRubric>, String> {
    let cutoff = (Utc::now().date_naive() - Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();

    let mut stmt = conn
        .prepare(
            "SELECT
                AVG(s.conceptual_knowledge),
                AVG(s.attention_to_detail),
                AVG(s.problem_decomposition),
                AVG(s.critical_evaluation),
                AVG(s.robustness_awareness),
                AVG(s.debugging_skill),
                AVG(s.prompt_specificity),
                AVG(s.scope_discipline)
             FROM scores s
             JOIN conversations c ON c.id = s.conversation_id
             WHERE c.completed_at >= ?1",
        )
        .map_err(|e| e.to_string())?;

    type DimEntry = (&'static str, &'static str, Option<f64>);
    let averages: [DimEntry; 8] = stmt
        .query_row([&cutoff], |row| {
            Ok([
                ("conceptualKnowledge",   "Conceptual Knowledge",   row.get::<_, Option<f64>>(0)?),
                ("attentionToDetail",     "Attention to Detail",    row.get::<_, Option<f64>>(1)?),
                ("problemDecomposition",  "Problem Decomposition",  row.get::<_, Option<f64>>(2)?),
                ("criticalEvaluation",    "Critical Evaluation",    row.get::<_, Option<f64>>(3)?),
                ("robustnessAwareness",   "Robustness Awareness",   row.get::<_, Option<f64>>(4)?),
                ("debuggingSkill",        "Debugging Skill",        row.get::<_, Option<f64>>(5)?),
                ("promptSpecificity",     "Prompt Specificity",     row.get::<_, Option<f64>>(6)?),
                ("scopeDiscipline",       "Scope Discipline",       row.get::<_, Option<f64>>(7)?),
            ])
        })
        .map_err(|e| e.to_string())?;

    let mut rubrics: Vec<WeakRubric> = averages
        .into_iter()
        .filter_map(|(dim, label, avg)| {
            avg.map(|v| WeakRubric {
                dimension: dim.to_string(),
                average_score: v,
                label: label.to_string(),
            })
        })
        .collect();

    rubrics.sort_by(|a, b| {
        a.average_score
            .partial_cmp(&b.average_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rubrics.truncate(3);
    Ok(rubrics)
}

fn start_of_week(date: NaiveDate) -> NaiveDate {
    let weekday = date.weekday().num_days_from_monday();
    date - Duration::days(weekday as i64)
}

fn week_label_from_start(week_start: &str) -> String {
    NaiveDate::parse_from_str(week_start, "%Y-%m-%d")
        .map(|d| {
            let iso = d.iso_week();
            format!("{:04}-W{:02}", iso.year(), iso.week())
        })
        .unwrap_or_else(|_| week_start.to_string())
}

// ── Analytics commands ────────────────────────────────────────────────────────

/// Recompute daily and weekly aggregate scores from the scored conversations table.
/// Should be called after scoring new conversations.
#[tauri::command]
pub async fn refresh_analytics(db: tauri::State<'_, Database>) -> Result<(), String> {
    let conn_arc = db.raw();
    tokio::task::spawn_blocking(move || {
        Database::run_with(conn_arc, refresh_analytics_on_conn)
    })
    .await
    .map_err(|e| format!("refresh_analytics panicked: {e}"))?
}

fn refresh_analytics_on_conn(conn: &rusqlite::Connection) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();

        // 1. Load all scored conversations (with completion date)
        let mut stmt = conn
            .prepare(
                "SELECT
                    substr(c.completed_at, 1, 10),
                    c.message_count,
                    c.tool_call_count,
                    s.final_score
                 FROM conversations c
                 JOIN scores s ON s.conversation_id = c.id
                 WHERE c.completed_at IS NOT NULL
                 ORDER BY c.completed_at ASC",
            )
            .map_err(|e| e.to_string())?;

        // Group by date: date -> Vec<(score, effort_weight)>
        let mut by_date: HashMap<String, Vec<(f64, f64)>> = HashMap::new();

        let rows: Vec<(String, i64, i64, f64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        for (date, msg, tool, score) in &rows {
            let w = effort_weight(*msg, *tool);
            by_date.entry(date.clone()).or_default().push((*score, w));
        }

        // 2. Upsert daily scores
        for (date, pairs) in &by_date {
            let avg = weighted_average(pairs);
            let count = pairs.len() as i64;
            let total_weight: f64 = pairs.iter().map(|(_, w)| w).sum();

            conn.execute(
                "INSERT INTO daily_scores
                    (score_date, average_score, conversation_count, total_effort_weight, computed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(score_date) DO UPDATE SET
                    average_score = excluded.average_score,
                    conversation_count = excluded.conversation_count,
                    total_effort_weight = excluded.total_effort_weight,
                    computed_at = excluded.computed_at",
                params![date, avg, count, total_weight, now],
            )
            .map_err(|e| e.to_string())?;
        }

        // 3. Load all daily scores and group by ISO week
        let mut daily_stmt = conn
            .prepare("SELECT score_date, average_score FROM daily_scores ORDER BY score_date ASC")
            .map_err(|e| e.to_string())?;

        let daily_rows: Vec<(String, f64)> = daily_stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        // Group daily scores by the Monday of each ISO week
        let mut by_week: HashMap<String, Vec<f64>> = HashMap::new();
        for (date_str, score) in &daily_rows {
            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                let week_key = start_of_week(date).format("%Y-%m-%d").to_string();
                by_week.entry(week_key).or_default().push(*score);
            }
        }

        // 4. Upsert weekly scores
        for (week_start, daily_scores) in &by_week {
            let avg = compute_weekly_score(daily_scores);
            let active_days = daily_scores.len() as i64;

            conn.execute(
                "INSERT INTO weekly_scores (week_start, average_score, active_days, computed_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(week_start) DO UPDATE SET
                    average_score = excluded.average_score,
                    active_days = excluded.active_days,
                    computed_at = excluded.computed_at",
                params![week_start, avg, active_days, now],
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(())
}

/// Return daily scores for the given period as trend points.
/// `period` = "7d" | "30d" | "90d" | "all"
#[tauri::command]
pub async fn get_trend_data(
    db: tauri::State<'_, Database>,
    period: String,
) -> Result<Vec<TrendPoint>, String> {
    let conn_arc = db.raw();
    tokio::task::spawn_blocking(move || Database::run_with(conn_arc, |conn| {
        let cutoff: Option<String> = match period.as_str() {
            "7d" => Some(
                (Utc::now().date_naive() - Duration::days(7))
                    .format("%Y-%m-%d")
                    .to_string(),
            ),
            "30d" => Some(
                (Utc::now().date_naive() - Duration::days(30))
                    .format("%Y-%m-%d")
                    .to_string(),
            ),
            "90d" => Some(
                (Utc::now().date_naive() - Duration::days(90))
                    .format("%Y-%m-%d")
                    .to_string(),
            ),
            _ => None, // "all" or unknown → no cutoff
        };

        // Build a single query with an optional WHERE clause to avoid borrow-checker
        // issues that arise from having `stmt` and `?` in separate if/else arms.
        let sql = if cutoff.is_some() {
            "SELECT score_date, average_score, conversation_count
             FROM daily_scores
             WHERE score_date >= ?1
             ORDER BY score_date ASC"
        } else {
            "SELECT score_date, average_score, conversation_count
             FROM daily_scores
             ORDER BY score_date ASC"
        };

        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;

        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(TrendPoint {
                date: row.get(0)?,
                score: row.get(1)?,
                conversation_count: row.get(2)?,
            })
        };

        let rows: Vec<TrendPoint> = match cutoff {
            Some(cutoff_str) => {
                let x: Vec<TrendPoint> = stmt
                    .query_map([cutoff_str], map_row)
                    .map_err(|e| e.to_string())?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?;
                x
            }
            None => {
                let x: Vec<TrendPoint> = stmt
                    .query_map([], map_row)
                    .map_err(|e| e.to_string())?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?;
                x
            }
        };

        Ok(rows)
    }))
    .await
    .map_err(|e| format!("get_trend_data panicked: {e}"))?
}

// ── Import commands ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn import_cursor(
    db: tauri::State<'_, Database>,
    data_path: Option<String>,
) -> Result<ImportResult, String> {
    let conn = db.raw();
    let path_label = data_path.clone().unwrap_or_else(|| "default".to_string());
    tokio::task::spawn_blocking(move || {
        log::info!("[cursor-import] Starting Cursor import (path: {path_label})");
        let scan_started = std::time::Instant::now();
        let (conversations, errors) = importers::cursor::import(data_path.as_deref());
        log::info!(
            "[cursor-import] Scan complete in {:.1}s — upserting {} conversation(s)",
            scan_started.elapsed().as_secs_f64(),
            conversations.len()
        );
        let result = upsert_conversations(conn, conversations, "cursor-local", errors, true);
        if let Ok(ref import_result) = result {
            log::info!(
                "[cursor-import] Import finished — imported {}, skipped {}, {} error(s)",
                import_result.imported,
                import_result.skipped,
                import_result.errors.len()
            );
        }
        result
    })
    .await
    .map_err(|e| format!("import_cursor task panicked: {e}"))?
}

#[tauri::command]
pub async fn import_claude_code(
    db: tauri::State<'_, Database>,
    transcripts_path: Option<String>,
) -> Result<ImportResult, String> {
    let conn = db.raw();
    tokio::task::spawn_blocking(move || {
        let (conversations, errors) = importers::claude_code::import(transcripts_path.as_deref());
        upsert_conversations(conn, conversations, "claude-code-local", errors, false)
    })
    .await
    .map_err(|e| format!("import_claude_code task panicked: {e}"))?
}

#[tauri::command]
pub async fn import_claude_markdown(
    db: tauri::State<'_, Database>,
    folder_path: String,
) -> Result<ImportResult, String> {
    let conn = db.raw();
    tokio::task::spawn_blocking(move || {
        let (conversations, errors) = importers::claude_markdown::import(&folder_path);
        upsert_conversations(conn, conversations, "claude-web-markdown", errors, false)
    })
    .await
    .map_err(|e| format!("import_claude_markdown task panicked: {e}"))?
}

#[tauri::command]
pub fn get_import_status(
    db: tauri::State<'_, Database>,
) -> Result<Vec<JobStatus>, String> {
    db.with_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, job_type, status, progress, error_message, created_at, updated_at
                 FROM jobs
                 WHERE job_type = 'import'
                 ORDER BY created_at DESC
                 LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let jobs = stmt
            .query_map([], |row| {
                Ok(JobStatus {
                    id: row.get(0)?,
                    job_type: row.get(1)?,
                    status: row.get(2)?,
                    progress: row.get(3)?,
                    error_message: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(jobs)
    })
}

#[tauri::command]
pub fn get_default_cursor_path() -> Result<String, String> {
    importers::cursor::default_cursor_data_path()
        .ok_or_else(|| "Could not determine home directory".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroup {
    pub project_path: String,
    pub project_name: String,
    pub conversation_count: i64,
    pub scored_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAllResult {
    pub cleared: bool,
    pub cursor: ImportResult,
    pub claude_code: ImportResult,
    pub claude_markdown: ImportResult,
    pub scored: usize,
    pub scoring_errors: Vec<String>,
}

#[tauri::command]
pub async fn clear_all_transcripts(db: tauri::State<'_, Database>) -> Result<(), String> {
    let conn = db.raw();
    tokio::task::spawn_blocking(move || Database::run_with(conn, clear_transcript_tables))
        .await
        .map_err(|e| format!("clear_all_transcripts panicked: {e}"))?
}

fn clear_transcript_tables(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "DELETE FROM scores;
         DELETE FROM messages;
         DELETE FROM daily_scores;
         DELETE FROM weekly_scores;
         DELETE FROM jobs;
         DELETE FROM conversations;
         DELETE FROM projects;",
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_projects(db: tauri::State<'_, Database>) -> Result<Vec<ProjectGroup>, String> {
    let conn = db.raw();
    tokio::task::spawn_blocking(move || Database::run_with(conn, |connection| {
        // Count a conversation as "scored" only when its score matches the
        // current rubric/prompt versions and content hash. Scores produced
        // by an older rubric (e.g. v1) should no longer count, so the user
        // sees that re-scoring is needed after a rubric bump.
        let mut stmt = connection
            .prepare(
                "SELECT
                    COALESCE(c.source_path, 'Unassigned') AS project_path,
                    COUNT(*) AS conversation_count,
                    SUM(CASE
                            WHEN s.id IS NOT NULL
                             AND s.rubric_version = ?1
                             AND s.prompt_version = ?2
                             AND s.content_hash  = c.content_hash
                            THEN 1 ELSE 0
                        END) AS scored_count
                 FROM conversations c
                 LEFT JOIN scores s ON s.conversation_id = c.id
                 GROUP BY COALESCE(c.source_path, 'Unassigned')
                 ORDER BY conversation_count DESC, project_path ASC",
            )
            .map_err(|e| e.to_string())?;

        let groups: Vec<ProjectGroup> = stmt
            .query_map(params![RUBRIC_VERSION, PROMPT_VERSION], |row| {
                let project_path: String = row.get(0)?;
                let project_name = display_project_name(None, Some(project_path.clone()))
                    .unwrap_or_else(|| "Unassigned".to_string());
                Ok(ProjectGroup {
                    project_path,
                    project_name,
                    conversation_count: row.get(1)?,
                    scored_count: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(groups)
    }))
    .await
    .map_err(|e| format!("list_projects panicked: {e}"))?
}

fn run_import_all(
    conn: Arc<Mutex<rusqlite::Connection>>,
    cursor_data_path: Option<String>,
    claude_code_path: Option<String>,
    claude_markdown_path: Option<String>,
    should_clear: bool,
) -> Result<(bool, ImportResult, ImportResult, ImportResult), String> {
    if should_clear {
        Database::run_with(Arc::clone(&conn), clear_transcript_tables)?;
    }

    let cursor_path = cursor_data_path.filter(|p| !p.trim().is_empty());
    let (cursor_convs, cursor_errors) = importers::cursor::import(cursor_path.as_deref());
    let cursor = upsert_conversations(
        Arc::clone(&conn),
        cursor_convs,
        "cursor-local",
        cursor_errors,
        true,
    )?;

    let claude_path = claude_code_path.filter(|p| !p.trim().is_empty());
    let (claude_convs, claude_errors) = importers::claude_code::import(claude_path.as_deref());
    let claude_code = upsert_conversations(
        Arc::clone(&conn),
        claude_convs,
        "claude-code-local",
        claude_errors,
        false,
    )?;

    let mut claude_markdown = ImportResult {
        source_type: "claude-web-markdown".to_string(),
        imported: 0,
        skipped: 0,
        errors: vec![],
    };
    if let Some(folder) = claude_markdown_path.filter(|p| !p.trim().is_empty()) {
        let (md_convs, md_errors) = importers::claude_markdown::import(&folder);
        claude_markdown = upsert_conversations(
            Arc::clone(&conn),
            md_convs,
            "claude-web-markdown",
            md_errors,
            false,
        )?;
    }

    Ok((should_clear, cursor, claude_code, claude_markdown))
}

#[tauri::command]
pub async fn import_all(
    db: tauri::State<'_, Database>,
    cursor_data_path: Option<String>,
    claude_code_path: Option<String>,
    claude_markdown_path: Option<String>,
    clear_existing: Option<bool>,
) -> Result<ImportAllResult, String> {
    let should_clear = clear_existing.unwrap_or(true);
    let conn = db.raw();

    let import_results = tokio::task::spawn_blocking(move || {
        run_import_all(
            conn,
            cursor_data_path,
            claude_code_path,
            claude_markdown_path,
            should_clear,
        )
    })
    .await
    .map_err(|e| format!("import_all panicked: {e}"))??;

    let (cleared, cursor, claude_code, claude_markdown) = import_results;

    Ok(ImportAllResult {
        cleared,
        cursor,
        claude_code,
        claude_markdown,
        scored: 0,
        scoring_errors: vec![],
    })
}

#[tauri::command]
pub async fn import_all_and_score(
    db: tauri::State<'_, Database>,
    api_key: String,
    cursor_data_path: Option<String>,
    claude_code_path: Option<String>,
    claude_markdown_path: Option<String>,
    scoring_model: Option<String>,
    clear_existing: Option<bool>,
) -> Result<ImportAllResult, String> {
    let should_clear = clear_existing.unwrap_or(true);
    let conn = db.raw();

    let import_results = tokio::task::spawn_blocking(move || {
        run_import_all(
            conn,
            cursor_data_path,
            claude_code_path,
            claude_markdown_path,
            should_clear,
        )
    })
    .await
    .map_err(|e| format!("import_all_and_score import phase panicked: {e}"))??;

    let (cleared, cursor, claude_code, claude_markdown) = import_results;

    let config = resolve_llm_config(&db, &api_key, scoring_model)?;
    let model = config.model();
    let pending = fetch_conversations_for_scoring(&db, None, None, None)?;

    let mut scored = 0usize;
    let mut scoring_errors = Vec::new();

    if !pending.is_empty() {
        match score_and_persist(&db, &pending, &config, &model).await {
            Ok(results) => scored = results.len(),
            Err(e) => scoring_errors.push(e),
        }
    }

    let conn_arc = db.raw();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        Database::run_with(conn_arc, refresh_analytics_on_conn)
    })
    .await
    .map_err(|e| format!("analytics refresh panicked: {e}"))?
    {
        scoring_errors.push(format!("Analytics refresh failed: {e}"));
    }

    Ok(ImportAllResult {
        cleared,
        cursor,
        claude_code,
        claude_markdown,
        scored,
        scoring_errors,
    })
}

// ── Shared upsert helper ─────────────────────────────────────────────────────

fn upsert_conversations(
    conn_arc: Arc<Mutex<rusqlite::Connection>>,
    conversations: Vec<importers::types::Conversation>,
    source_type: &str,
    import_errors: Vec<String>,
    log_progress: bool,
) -> Result<ImportResult, String> {
    let started = std::time::Instant::now();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let errors = import_errors;
    let total = conversations.len();

    if log_progress {
        log::info!("[cursor-import] Upserting {total} conversation(s) into database");
    }

    let conn = conn_arc
        .lock()
        .map_err(|_| "Database lock poisoned".to_string())?;

    {
        let conn = &*conn;
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;

        let upsert_result = (|| -> Result<(usize, usize), String> {
            let now = Utc::now().to_rfc3339();
            let mut processed = 0usize;

            for conv in conversations {
                processed += 1;
            let content_hash = importers::normalizer::content_hash(&conv);

            // Check if this content_hash already exists (unchanged → skip)
            let existing_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM conversations WHERE external_id = ?1",
                    [&conv.id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            if let Some(existing_row_id) = existing_id {
                let existing_hash: Option<String> = conn
                    .query_row(
                        "SELECT content_hash FROM conversations WHERE id = ?1",
                        [existing_row_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?
                    .flatten();

                if existing_hash.as_deref() == Some(&content_hash) {
                    conn.execute(
                        "UPDATE conversations
                         SET title = ?1, source_path = ?2, updated_at = ?3
                         WHERE id = ?4",
                        params![
                            conv.title,
                            conv.project_path,
                            now,
                            existing_row_id,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                    skipped += 1;
                    continue;
                }

                // Content changed – delete messages and re-import
                conn.execute(
                    "DELETE FROM messages WHERE conversation_id = ?1",
                    [existing_row_id],
                )
                .map_err(|e| e.to_string())?;

                let message_count = conv.messages.len() as i64;
                let tool_call_count: i64 = conv
                    .messages
                    .iter()
                    .map(|m| m.tool_calls.len() as i64)
                    .sum();

                conn.execute(
                    "UPDATE conversations
                     SET title = ?1, content_hash = ?2, message_count = ?3,
                         tool_call_count = ?4, started_at = ?5, completed_at = ?6,
                         updated_at = ?7
                     WHERE id = ?8",
                    params![
                        conv.title,
                        content_hash,
                        message_count,
                        tool_call_count,
                        conv.started_at,
                        conv.ended_at,
                        now,
                        existing_row_id,
                    ],
                )
                .map_err(|e| e.to_string())?;

                insert_messages(conn, existing_row_id, &conv.messages, &now)?;
                imported += 1;
            } else {
                // New conversation
                let message_count = conv.messages.len() as i64;
                let tool_call_count: i64 = conv
                    .messages
                    .iter()
                    .map(|m| m.tool_calls.len() as i64)
                    .sum();

                conn.execute(
                    "INSERT INTO conversations
                        (external_id, provider, title, source_path, content_hash,
                         message_count, tool_call_count, started_at, completed_at,
                         imported_at, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        conv.id,
                        source_type,
                        conv.title,
                        conv.project_path,
                        content_hash,
                        message_count,
                        tool_call_count,
                        conv.started_at,
                        conv.ended_at,
                        now,
                        now,
                        now,
                    ],
                )
                .map_err(|e| e.to_string())?;

                let row_id = conn.last_insert_rowid();
                insert_messages(conn, row_id, &conv.messages, &now)?;
                imported += 1;
            }

            if log_progress {
                importers::cursor::log_upsert_progress(processed, total, imported, skipped);
            }
        }

            Ok((imported, skipped))
        })();

        match upsert_result {
            Ok((imported_count, skipped_count)) => {
                imported = imported_count;
                skipped = skipped_count;
                conn.execute_batch("COMMIT")
                    .map_err(|e| e.to_string())?;
            }
            Err(error) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(error);
            }
        }
    }

    if log_progress {
        log::info!(
            "[cursor-import] Upsert complete in {:.1}s — imported {imported}, skipped {skipped}",
            started.elapsed().as_secs_f64()
        );
    }

    Ok(ImportResult {
        source_type: source_type.to_string(),
        imported,
        skipped,
        errors,
    })
}

fn insert_messages(
    conn: &rusqlite::Connection,
    conversation_id: i64,
    messages: &[importers::types::Message],
    now: &str,
) -> Result<(), String> {
    for (seq, msg) in messages.iter().enumerate() {
        let tool_name = msg.tool_calls.first().cloned();
        conn.execute(
            "INSERT INTO messages (conversation_id, role, content, tool_name, sequence_num, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(conversation_id, sequence_num) DO UPDATE SET
                role = excluded.role,
                content = excluded.content,
                tool_name = excluded.tool_name",
            params![conversation_id, msg.role, msg.content, tool_name, seq as i64, now],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Scoring commands ──────────────────────────────────────────────────────────

/// Minimal message row for building the scoring prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRecord {
    pub role: String,
    pub content: String,
    pub sequence_num: i64,
}

/// Full score record as stored in the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreRecord {
    pub id: i64,
    pub conversation_id: i64,
    pub conceptual_knowledge: f64,
    pub attention_to_detail: f64,
    pub problem_decomposition: f64,
    pub critical_evaluation: f64,
    pub robustness_awareness: f64,
    pub debugging_skill: f64,
    pub prompt_specificity: f64,
    pub scope_discipline: f64,
    pub final_score: f64,
    pub explanation: Option<String>,
    pub model_id: String,
    pub rubric_version: String,
    pub prompt_version: String,
    pub content_hash: String,
    pub cache_key: String,
    pub scored_at: String,
    pub created_at: String,
}

/// Conversation joined with its score (for top-N queries).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationWithScore {
    pub id: i64,
    pub title: String,
    pub provider: String,
    pub project_name: Option<String>,
    pub source_path: Option<String>,
    pub final_score: Option<f64>,
    pub completed_at: Option<String>,
    pub message_count: i64,
    pub tool_call_count: i64,
    pub conceptual_knowledge: Option<f64>,
    pub attention_to_detail: Option<f64>,
    pub problem_decomposition: Option<f64>,
    pub critical_evaluation: Option<f64>,
    pub robustness_awareness: Option<f64>,
    pub debugging_skill: Option<f64>,
    pub prompt_specificity: Option<f64>,
    pub scope_discipline: Option<f64>,
    pub explanation: Option<String>,
    pub model_id: Option<String>,
    pub scored_at: Option<String>,
}

/// Score all un-scored (or stale) conversations in a project folder.
/// `min_user_messages`: when set, only conversations with strictly more than this many user messages are scored.
#[tauri::command]
pub async fn score_project(
    db: tauri::State<'_, Database>,
    api_key: String,
    project_path: String,
    model_id: Option<String>,
    min_user_messages: Option<i64>,
) -> Result<Vec<ScoringResult>, String> {
    let config = resolve_llm_config(&db, &api_key, model_id)?;
    let model = config.model();
    let pending =
        fetch_conversations_for_scoring(&db, None, Some(&project_path), min_user_messages)?;

    if pending.is_empty() {
        return Ok(Vec::new());
    }

    let results = score_and_persist(&db, &pending, &config, &model).await?;

    let conn_arc = db.raw();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        Database::run_with(conn_arc, refresh_analytics_on_conn)
    })
    .await
    .map_err(|e| format!("analytics refresh panicked: {e}"))?
    {
        log::warn!("Analytics refresh after score_project failed: {e}");
    }

    Ok(results)
}

/// Top-N scored conversations within a single project folder.
#[tauri::command]
pub fn get_project_top_conversations(
    db: tauri::State<'_, Database>,
    project_path: String,
    limit: Option<i64>,
) -> Result<Vec<ConversationWithScore>, String> {
    let n = limit.unwrap_or(3).max(1);

    db.with_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    c.id,
                    c.title,
                    c.provider,
                    p.name,
                    c.source_path,
                    s.final_score,
                    c.completed_at,
                    c.message_count,
                    c.tool_call_count,
                    s.conceptual_knowledge,
                    s.attention_to_detail,
                    s.problem_decomposition,
                    s.critical_evaluation,
                    s.robustness_awareness,
                    s.debugging_skill,
                    s.prompt_specificity,
                    s.scope_discipline,
                    s.explanation,
                    s.model_id,
                    s.scored_at
                 FROM conversations c
                 LEFT JOIN projects p ON p.id = c.project_id
                 JOIN scores s ON s.conversation_id = c.id
                 WHERE COALESCE(c.source_path, 'Unassigned') = ?1
                   AND s.rubric_version = ?3
                   AND s.prompt_version = ?4
                   AND s.content_hash  = c.content_hash
                 ORDER BY s.final_score DESC, c.completed_at DESC, c.id ASC
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![project_path, n, RUBRIC_VERSION, PROMPT_VERSION], |row| {
                let project_name: Option<String> = row.get(3)?;
                let source_path: Option<String> = row.get(4)?;
                Ok(ConversationWithScore {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    provider: row.get(2)?,
                    project_name: display_project_name(project_name, source_path.clone()),
                    source_path,
                    final_score: row.get(5)?,
                    completed_at: row.get(6)?,
                    message_count: row.get(7)?,
                    tool_call_count: row.get(8)?,
                    conceptual_knowledge: row.get(9)?,
                    attention_to_detail: row.get(10)?,
                    problem_decomposition: row.get(11)?,
                    critical_evaluation: row.get(12)?,
                    robustness_awareness: row.get(13)?,
                    debugging_skill: row.get(14)?,
                    prompt_specificity: row.get(15)?,
                    scope_discipline: row.get(16)?,
                    explanation: row.get(17)?,
                    model_id: row.get(18)?,
                    scored_at: row.get(19)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(rows)
    })
}

/// Score all un-scored (or stale) conversations, in batches of 5.
#[tauri::command]
pub async fn score_pending(
    db: tauri::State<'_, Database>,
    api_key: String,
    model_id: Option<String>,
) -> Result<Vec<ScoringResult>, String> {
    let config = resolve_llm_config(&db, &api_key, model_id)?;
    let model = config.model();
    let pending = fetch_conversations_for_scoring(&db, None, None, None)?;

    if pending.is_empty() {
        return Ok(Vec::new());
    }

    score_and_persist(&db, &pending, &config, &model).await
}

/// Score a single conversation if it is un-scored or stale.
#[tauri::command]
pub async fn score_conversation(
    db: tauri::State<'_, Database>,
    api_key: String,
    conversation_id: i64,
    model_id: Option<String>,
) -> Result<ScoringResult, String> {
    let config = resolve_llm_config(&db, &api_key, model_id)?;
    let model = config.model();
    let pending = fetch_conversations_for_scoring(&db, Some(conversation_id), None, None)?;

    if pending.is_empty() {
        return Err(format!(
            "Conversation {conversation_id} not found or already scored with current content"
        ));
    }

    let mut results = score_and_persist(&db, &pending, &config, &model).await?;
    results
        .pop()
        .ok_or_else(|| "Scoring produced no result".to_string())
}

fn user_message_min_filter_sql(param_index: usize) -> String {
    format!(
        "AND (?{param_index} IS NULL OR (SELECT COUNT(*) FROM messages m \
         WHERE m.conversation_id = c.id AND m.role = 'user') > ?{param_index})"
    )
}

/// A conversation is "pending scoring" if any of the following are true:
///   - it has never been scored,
///   - its content has changed since the last score (content_hash mismatch),
///   - the rubric version has changed since the last score, or
///   - the prompt version has changed since the last score.
///
/// The last two cases ensure that bumping the scoring rubric automatically
/// makes every previously-scored conversation eligible for re-scoring.
fn stale_score_predicate(rubric_param: usize, prompt_param: usize) -> String {
    format!(
        "(s.id IS NULL \
            OR s.content_hash != c.content_hash \
            OR s.rubric_version != ?{rubric_param} \
            OR s.prompt_version != ?{prompt_param})"
    )
}

fn fetch_conversations_for_scoring(
    db: &Database,
    conversation_id: Option<i64>,
    project_path: Option<&str>,
    min_user_messages: Option<i64>,
) -> Result<Vec<ConversationForScoring>, String> {
    db.with_connection(|conn| {
        let conv_ids: Vec<(i64, String)> = if let Some(id) = conversation_id {
            let sql = format!(
                "SELECT c.id, c.content_hash
                 FROM conversations c
                 LEFT JOIN scores s ON s.conversation_id = c.id
                 WHERE c.id = ?1
                   AND {stale}
                 {min_filter}
                 ORDER BY c.id ASC",
                stale = stale_score_predicate(3, 4),
                min_filter = user_message_min_filter_sql(2),
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(
                    params![id, min_user_messages, RUBRIC_VERSION, PROMPT_VERSION],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        } else if let Some(path) = project_path {
            let sql = format!(
                "SELECT c.id, c.content_hash
                 FROM conversations c
                 LEFT JOIN scores s ON s.conversation_id = c.id
                 WHERE COALESCE(c.source_path, 'Unassigned') = ?1
                   AND {stale}
                 {min_filter}
                 ORDER BY c.id ASC",
                stale = stale_score_predicate(3, 4),
                min_filter = user_message_min_filter_sql(2),
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(
                    params![path, min_user_messages, RUBRIC_VERSION, PROMPT_VERSION],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        } else {
            let sql = format!(
                "SELECT c.id, c.content_hash
                 FROM conversations c
                 LEFT JOIN scores s ON s.conversation_id = c.id
                 WHERE {stale}
                 {min_filter}
                 ORDER BY c.id ASC",
                stale = stale_score_predicate(2, 3),
                min_filter = user_message_min_filter_sql(1),
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(
                    params![min_user_messages, RUBRIC_VERSION, PROMPT_VERSION],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };

        let mut convs = Vec::new();
        for (conv_id, content_hash) in conv_ids {
            let mut msg_stmt = conn
                .prepare(
                    "SELECT role, content FROM messages
                     WHERE conversation_id = ?1
                     ORDER BY sequence_num ASC",
                )
                .map_err(|e| e.to_string())?;

            let messages: Vec<(String, String)> = msg_stmt
                .query_map([conv_id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            convs.push(ConversationForScoring {
                id: conv_id,
                content_hash,
                messages,
            });
        }

        Ok(convs)
    })
}

async fn score_and_persist(
    db: &Database,
    pending: &[ConversationForScoring],
    config: &LlmConfig,
    model: &str,
) -> Result<Vec<ScoringResult>, String> {
    let mut all_results: Vec<ScoringResult> = Vec::new();

    for batch in pending.chunks(5) {
        let results =
            crate::scoring::scorer::score_batch(batch, config, model).await?;
        persist_scoring_results(db, &results)?;
        all_results.extend(results);
    }

    Ok(all_results)
}

fn persist_scoring_results(db: &Database, results: &[ScoringResult]) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    db.with_connection(|conn| {
        for r in results {
            let conv_id: i64 = r.conversation_id.parse().map_err(|_| {
                format!("Invalid conversation_id in result: {}", r.conversation_id)
            })?;
            conn.execute(
                "INSERT INTO scores
                    (conversation_id,
                     conceptual_knowledge, attention_to_detail, problem_decomposition,
                     critical_evaluation, robustness_awareness, debugging_skill,
                     prompt_specificity, scope_discipline,
                     final_score, explanation, model_id,
                     rubric_version, prompt_version, content_hash, cache_key,
                     scored_at, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
                 ON CONFLICT(conversation_id) DO UPDATE SET
                    conceptual_knowledge  = excluded.conceptual_knowledge,
                    attention_to_detail   = excluded.attention_to_detail,
                    problem_decomposition = excluded.problem_decomposition,
                    critical_evaluation   = excluded.critical_evaluation,
                    robustness_awareness  = excluded.robustness_awareness,
                    debugging_skill       = excluded.debugging_skill,
                    prompt_specificity    = excluded.prompt_specificity,
                    scope_discipline      = excluded.scope_discipline,
                    final_score           = excluded.final_score,
                    explanation           = excluded.explanation,
                    model_id              = excluded.model_id,
                    rubric_version        = excluded.rubric_version,
                    prompt_version        = excluded.prompt_version,
                    content_hash          = excluded.content_hash,
                    cache_key             = excluded.cache_key,
                    scored_at             = excluded.scored_at",
                params![
                    conv_id,
                    r.dimensions.conceptual_knowledge,
                    r.dimensions.attention_to_detail,
                    r.dimensions.problem_decomposition,
                    r.dimensions.critical_evaluation,
                    r.dimensions.robustness_awareness,
                    r.dimensions.debugging_skill,
                    r.dimensions.prompt_specificity,
                    r.dimensions.scope_discipline,
                    r.final_score,
                    r.explanation,
                    r.model_id,
                    r.rubric_version,
                    r.prompt_version,
                    r.content_hash,
                    r.cache_key,
                    r.scored_at,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}

/// Fetch score records, optionally filtered by conversation_id.
#[tauri::command]
pub fn get_scores(
    db: tauri::State<'_, Database>,
    conversation_id: Option<i64>,
) -> Result<Vec<ScoreRecord>, String> {
    db.with_connection(|conn| {
        if let Some(cid) = conversation_id {
            let mut stmt = conn
                .prepare(
                    "SELECT id, conversation_id,
                            conceptual_knowledge, attention_to_detail, problem_decomposition,
                            critical_evaluation, robustness_awareness, debugging_skill,
                            prompt_specificity, scope_discipline,
                            final_score, explanation, model_id, rubric_version, prompt_version,
                            content_hash, cache_key, scored_at, created_at
                     FROM scores
                     WHERE conversation_id = ?1
                     ORDER BY scored_at DESC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([cid], map_score_row)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(rows)
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, conversation_id,
                            conceptual_knowledge, attention_to_detail, problem_decomposition,
                            critical_evaluation, robustness_awareness, debugging_skill,
                            prompt_specificity, scope_discipline,
                            final_score, explanation, model_id, rubric_version, prompt_version,
                            content_hash, cache_key, scored_at, created_at
                     FROM scores
                     ORDER BY scored_at DESC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], map_score_row)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(rows)
        }
    })
}

fn map_score_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScoreRecord> {
    Ok(ScoreRecord {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        conceptual_knowledge: row.get(2)?,
        attention_to_detail: row.get(3)?,
        problem_decomposition: row.get(4)?,
        critical_evaluation: row.get(5)?,
        robustness_awareness: row.get(6)?,
        debugging_skill: row.get(7)?,
        prompt_specificity: row.get(8)?,
        scope_discipline: row.get(9)?,
        final_score: row.get(10)?,
        explanation: row.get(11)?,
        model_id: row.get(12)?,
        rubric_version: row.get(13)?,
        prompt_version: row.get(14)?,
        content_hash: row.get(15)?,
        cache_key: row.get(16)?,
        scored_at: row.get(17)?,
        created_at: row.get(18)?,
    })
}

/// Fetch top-N conversations ordered by score, then recency, then id.
#[tauri::command]
pub fn get_top_conversations(
    db: tauri::State<'_, Database>,
    limit: Option<i64>,
) -> Result<Vec<ConversationWithScore>, String> {
    let n = limit.unwrap_or(3).max(1);

    db.with_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT
                    c.id,
                    c.title,
                    c.provider,
                    p.name,
                    c.source_path,
                    s.final_score,
                    c.completed_at,
                    c.message_count,
                    c.tool_call_count,
                    s.conceptual_knowledge,
                    s.attention_to_detail,
                    s.problem_decomposition,
                    s.critical_evaluation,
                    s.robustness_awareness,
                    s.debugging_skill,
                    s.prompt_specificity,
                    s.scope_discipline,
                    s.explanation,
                    s.model_id,
                    s.scored_at
                 FROM conversations c
                 LEFT JOIN projects p ON p.id = c.project_id
                 LEFT JOIN scores s ON s.conversation_id = c.id
                 ORDER BY s.final_score DESC, c.completed_at DESC, c.id ASC
                 LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([n], |row| {
                let project_name: Option<String> = row.get(3)?;
                let source_path: Option<String> = row.get(4)?;
                Ok(ConversationWithScore {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    provider: row.get(2)?,
                    project_name: display_project_name(project_name, source_path.clone()),
                    source_path,
                    final_score: row.get(5)?,
                    completed_at: row.get(6)?,
                    message_count: row.get(7)?,
                    tool_call_count: row.get(8)?,
                    conceptual_knowledge: row.get(9)?,
                    attention_to_detail: row.get(10)?,
                    problem_decomposition: row.get(11)?,
                    critical_evaluation: row.get(12)?,
                    robustness_awareness: row.get(13)?,
                    debugging_skill: row.get(14)?,
                    prompt_specificity: row.get(15)?,
                    scope_discipline: row.get(16)?,
                    explanation: row.get(17)?,
                    model_id: row.get(18)?,
                    scored_at: row.get(19)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(rows)
    })
}

/// Fetch messages for a single conversation (for the detail page).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationExportMarkdown {
    pub markdown: String,
    pub suggested_filename: String,
    pub provider: String,
}

fn load_conversation_markdown_export(
    db: &Database,
    conversation_id: i64,
) -> Result<ConversationExportMarkdown, String> {
    db.with_connection(|conn| {
        let (title, provider, completed_at): (String, String, Option<String>) = conn
            .query_row(
                "SELECT title, provider, completed_at FROM conversations WHERE id = ?1",
                [conversation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| format!("Conversation {conversation_id} not found"))?;

        let mut stmt = conn
            .prepare(
                "SELECT role, content FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY sequence_num ASC",
            )
            .map_err(|e| e.to_string())?;

        let messages: Vec<crate::exporters::ExportMessage> = stmt
            .query_map([conversation_id], |row| {
                Ok(crate::exporters::ExportMessage {
                    role: row.get(0)?,
                    content: row.get(1)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        if messages.is_empty() {
            return Err("Conversation has no messages to export".to_string());
        }

        let export = crate::exporters::build_conversation_markdown_export(
            &title,
            &provider,
            completed_at.as_deref(),
            &messages,
        );

        Ok(ConversationExportMarkdown {
            markdown: export.markdown,
            suggested_filename: export.suggested_filename,
            provider: export.provider,
        })
    })
}

#[tauri::command]
pub fn get_conversation_export_markdown(
    db: tauri::State<'_, Database>,
    conversation_id: i64,
) -> Result<ConversationExportMarkdown, String> {
    load_conversation_markdown_export(&db, conversation_id)
}

#[tauri::command]
pub async fn export_conversation_markdown(
    app: tauri::AppHandle,
    db: tauri::State<'_, Database>,
    conversation_id: i64,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let export = load_conversation_markdown_export(&db, conversation_id)?;

    let path = app
        .dialog()
        .file()
        .set_title("Export conversation as Markdown")
        .set_file_name(&export.suggested_filename)
        .add_filter("Markdown", &["md"])
        .blocking_save_file();

    match path {
        Some(path) => {
            let path_buf = path
                .into_path()
                .map_err(|e| format!("Invalid save path: {e}"))?;
            std::fs::write(&path_buf, export.markdown).map_err(|e| e.to_string())?;
            Ok(Some(path_buf.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn get_conversation_messages(
    db: tauri::State<'_, Database>,
    conversation_id: i64,
) -> Result<Vec<MessageRecord>, String> {
    db.with_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT role, content, sequence_num FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY sequence_num ASC",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([conversation_id], |row| {
                Ok(MessageRecord {
                    role: row.get(0)?,
                    content: row.get(1)?,
                    sequence_num: row.get(2)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(rows)
    })
}

// ── Project rules commands ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRulesView {
    pub report: ProjectRulesReport,
    pub score: Option<ProjectRulesScore>,
    /// True when the stored score's `content_hash` no longer matches the
    /// freshly-scanned report — i.e. the user changed their rule files
    /// since the last grading.
    pub stale: bool,
}

/// Scan a project directory for AI-instruction files and detect its tech
/// stack. Pulls the most recent stored score (if any) and marks it `stale`
/// when the rule files have changed since.
#[tauri::command]
pub async fn scan_project_rules(
    db: tauri::State<'_, Database>,
    project_path: String,
) -> Result<ProjectRulesView, String> {
    let path_clone = project_path.clone();
    let report = tokio::task::spawn_blocking(move || scanner_scan(&path_clone))
        .await
        .map_err(|e| format!("scan_project_rules panicked: {e}"))?;

    let score = read_project_rules_score(&db, &project_path)?;
    let stale = match &score {
        Some(s) => s.content_hash != report.content_hash
            || s.rubric_version != PROJECT_RULES_RUBRIC_VERSION,
        None => false,
    };

    Ok(ProjectRulesView {
        report,
        score,
        stale,
    })
}

/// Score the project's rule files against its detected tech stack using
/// the configured LLM provider. Persists the result so future
/// calls to `scan_project_rules` return it without a second LLM round-trip.
#[tauri::command]
pub async fn score_project_rules(
    db: tauri::State<'_, Database>,
    api_key: String,
    project_path: String,
    model_id: Option<String>,
) -> Result<ProjectRulesScore, String> {
    let config = resolve_llm_config(&db, &api_key, model_id)?;
    let model = config.model();

    let path_clone = project_path.clone();
    let report = tokio::task::spawn_blocking(move || scanner_scan(&path_clone))
        .await
        .map_err(|e| format!("scan_project_rules panicked: {e}"))?;

    let score = score_project_rules_with_llm(&report, &config, &model).await?;
    persist_project_rules_score(&db, &report, &score)?;
    Ok(score)
}

fn persist_project_rules_score(
    db: &Database,
    report: &ProjectRulesReport,
    score: &ProjectRulesScore,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    let tech_stack_json =
        serde_json::to_string(&report.tech_stack).map_err(|e| e.to_string())?;
    let rule_files_json =
        serde_json::to_string(&report.rule_files).map_err(|e| e.to_string())?;
    let suggestions_json =
        serde_json::to_string(&score.suggestions).map_err(|e| e.to_string())?;

    db.with_connection(|conn| {
        conn.execute(
            "INSERT INTO project_rule_scores
                (project_path, tech_stack_json, rule_files_json, content_hash,
                 coverage, stack_alignment, specificity, actionability, overall_score,
                 summary, suggestions_json, model_id, rubric_version, scored_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(project_path) DO UPDATE SET
                tech_stack_json  = excluded.tech_stack_json,
                rule_files_json  = excluded.rule_files_json,
                content_hash     = excluded.content_hash,
                coverage         = excluded.coverage,
                stack_alignment  = excluded.stack_alignment,
                specificity      = excluded.specificity,
                actionability    = excluded.actionability,
                overall_score    = excluded.overall_score,
                summary          = excluded.summary,
                suggestions_json = excluded.suggestions_json,
                model_id         = excluded.model_id,
                rubric_version   = excluded.rubric_version,
                scored_at        = excluded.scored_at",
            params![
                score.project_path,
                tech_stack_json,
                rule_files_json,
                score.content_hash,
                score.coverage,
                score.stack_alignment,
                score.specificity,
                score.actionability,
                score.overall_score,
                score.summary,
                suggestions_json,
                score.model_id,
                score.rubric_version,
                score.scored_at,
                now,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

fn read_project_rules_score(
    db: &Database,
    project_path: &str,
) -> Result<Option<ProjectRulesScore>, String> {
    db.with_connection(|conn| {
        let row = conn
            .query_row(
                "SELECT
                    project_path, content_hash, coverage, stack_alignment, specificity,
                    actionability, overall_score, summary, suggestions_json,
                    model_id, rubric_version, scored_at
                 FROM project_rule_scores
                 WHERE project_path = ?1",
                [project_path],
                |row| {
                    let suggestions_json: String = row.get(8)?;
                    let suggestions: Vec<String> =
                        serde_json::from_str(&suggestions_json).unwrap_or_default();
                    Ok(ProjectRulesScore {
                        project_path: row.get(0)?,
                        content_hash: row.get(1)?,
                        coverage: row.get(2)?,
                        stack_alignment: row.get(3)?,
                        specificity: row.get(4)?,
                        actionability: row.get(5)?,
                        overall_score: row.get(6)?,
                        summary: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                        suggestions,
                        model_id: row.get(9)?,
                        rubric_version: row.get(10)?,
                        scored_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        Ok(row)
    })
}

// ── Improve the Vibe command ──────────────────────────────────────────────────

/// One prompt improvement: the original bad prompt and the concrete rewrite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VibeImprovement {
    pub index: u8,
    pub bad_prompt: String,
    pub improved_prompt: String,
    pub tip: String,
}

/// Analyse up to the first 20 user prompts from a single conversation and
/// return up to 10 concrete "bad → improved" prompt examples.
#[tauri::command]
pub async fn analyze_chat_vibe(
    db: tauri::State<'_, Database>,
    conversation_id: i64,
    api_key: String,
    model_id: Option<String>,
) -> Result<Vec<VibeImprovement>, String> {
    let config = resolve_llm_config(&db, &api_key, model_id)?;

    // Load user messages synchronously.
    let user_messages: Vec<String> = db.with_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT content FROM messages
                 WHERE conversation_id = ?1 AND role = 'user'
                 ORDER BY sequence_num ASC",
            )
            .map_err(|e| e.to_string())?;

        stmt.query_map([conversation_id], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })?;

    if user_messages.is_empty() {
        return Ok(Vec::new());
    }

    // Compile up to 20 prompts, each capped at 600 chars, into the context block.
    let prompts_text: String = user_messages
        .iter()
        .take(20)
        .enumerate()
        .map(|(i, m)| {
            let excerpt = if m.chars().count() > 600 {
                format!("{}…", &m[..m.char_indices().nth(600).map(|(b, _)| b).unwrap_or(m.len())])
            } else {
                m.clone()
            };
            format!("[Prompt {}]: {}", i + 1, excerpt)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let system_prompt =
        "You are a vibe-coding coach who helps developers write better AI prompts. \
         Analyse the user's prompts from a real coding session. \
         Identify up to 10 specific problems — vagueness, missing context, no expected output, \
         imperative tone, multi-tasking in one shot, etc. \
         For each problem quote the actual bad prompt (or a short excerpt), \
         then write a concrete improved version that is specific, scoped, \
         and includes expected output. \
         Return ONLY valid JSON — no markdown fences, no commentary.";

    let user_content = format!(
        "User prompts from the coding session:\n\n{prompts_text}\n\n\
         Respond ONLY with valid JSON matching this schema:\n\
         {{\"improvements\": [\
           {{\"bad_prompt\": \"<exact or shortened quote>\", \
             \"improved_prompt\": \"<concrete rewrite>\", \
             \"tip\": \"<one-line reason why this version is better>\"}}\
         ]}}\n\
         Return up to 10 items. Only flag genuine issues you actually observed."
    );

    let raw_content = llm::chat_completion(
        &config,
        vec![
            crate::azure::ChatMessage {
                role: "system",
                content: system_prompt.to_string(),
            },
            crate::azure::ChatMessage {
                role: "user",
                content: user_content,
            },
        ],
        None,
    )
    .await?;

    let json_str = extract_first_json_object(&raw_content);

    #[derive(serde::Deserialize)]
    struct RawPayload {
        improvements: Vec<RawImprovement>,
    }
    #[derive(serde::Deserialize)]
    struct RawImprovement {
        bad_prompt: String,
        improved_prompt: String,
        tip: String,
    }

    let payload: RawPayload = serde_json::from_str(&json_str).map_err(|e| {
        format!(
            "Failed to parse vibe analysis JSON: {e}\nRaw response: {raw_content}"
        )
    })?;

    Ok(payload
        .improvements
        .into_iter()
        .take(10)
        .enumerate()
        .map(|(i, r)| VibeImprovement {
            index: (i + 1) as u8,
            bad_prompt: r.bad_prompt,
            improved_prompt: r.improved_prompt,
            tip: r.tip,
        })
        .collect())
}

fn extract_first_json_object(text: &str) -> String {
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

// ── shared helpers ────────────────────────────────────────────────────────────

fn display_project_name(
    project_name: Option<String>,
    source_path: Option<String>,
) -> Option<String> {
    if project_name.is_some() {
        return project_name;
    }

    source_path.and_then(|path| {
        std::path::Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
    })
}
