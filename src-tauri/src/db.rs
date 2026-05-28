use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::{Connection, OptionalExtension};
use tauri::{AppHandle, Manager};

const DB_FILE_NAME: &str = "vibe-score.db";

pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn initialize(app: &AppHandle) -> Result<Self, String> {
        let db_path = database_path(app)?;
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let connection =
            Connection::open(&db_path).map_err(|error| format!("Failed to open database: {error}"))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| error.to_string())?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| error.to_string())?;

        run_migrations(&connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Return a cloneable handle to the connection so commands can move it
    /// into `tokio::task::spawn_blocking` without holding `tauri::State`.
    pub fn raw(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.connection)
    }

    /// Run a closure on a pre-cloned Arc handle (useful inside spawn_blocking).
    pub fn run_with<T, F>(conn: Arc<Mutex<Connection>>, f: F) -> Result<T, String>
    where
        F: FnOnce(&Connection) -> Result<T, String>,
    {
        let guard = conn.lock().map_err(|_| "Database lock poisoned".to_string())?;
        f(&*guard)
    }

    pub fn with_connection<T, F>(&self, callback: F) -> Result<T, String>
    where
        F: FnOnce(&Connection) -> Result<T, String>,
    {
        let connection = self
            .connection
            .lock()
            .map_err(|_| "Database lock poisoned".to_string())?;
        callback(&connection)
    }
}

pub fn database_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(DB_FILE_NAME))
        .map_err(|error| error.to_string())
}

fn run_migrations(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )
        .map_err(|error| error.to_string())?;

    for migration in MIGRATIONS {
        let already_applied: bool = connection
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |_| Ok(true),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or(false);

        if already_applied {
            continue;
        }

        connection
            .execute_batch(migration.sql)
            .map_err(|error| format!("Migration {} failed: {error}", migration.version))?;

        let applied_at = chrono::Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                (migration.version, &applied_at),
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

struct Migration {
    version: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "001_initial_schema.sql",
        sql: include_str!("../migrations/001_initial_schema.sql"),
    },
    Migration {
        version: "002_add_cache_key.sql",
        sql: include_str!("../migrations/002_add_cache_key.sql"),
    },
    Migration {
        version: "003_drop_search_and_suggestions.sql",
        sql: include_str!("../migrations/003_drop_search_and_suggestions.sql"),
    },
    Migration {
        version: "004_project_rule_scores.sql",
        sql: include_str!("../migrations/004_project_rule_scores.sql"),
    },
    Migration {
        version: "005_rubric_v3_dimensions.sql",
        sql: include_str!("../migrations/005_rubric_v3_dimensions.sql"),
    },
];
