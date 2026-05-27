use std::{
    fs,
    path::{Path, PathBuf},
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

    for migration in collect_migrations()? {
        let already_applied: bool = connection
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE version = ?1",
                [migration.version.as_str()],
                |_| Ok(true),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or(false);

        if already_applied {
            continue;
        }

        connection
            .execute_batch(&migration.sql)
            .map_err(|error| format!("Migration {} failed: {error}", migration.version))?;

        let applied_at = chrono::Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                (&migration.version, &applied_at),
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

struct Migration {
    version: String,
    sql: String,
}

fn collect_migrations() -> Result<Vec<Migration>, String> {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut entries: Vec<PathBuf> = fs::read_dir(&migrations_dir)
        .map_err(|error| format!("Failed to read migrations directory: {error}"))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();

    entries.sort();

    entries
        .into_iter()
        .map(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "Invalid migration filename".to_string())?
                .to_string();
            let sql = fs::read_to_string(&path)
                .map_err(|error| format!("Failed to read migration {file_name}: {error}"))?;
            Ok(Migration {
                version: file_name,
                sql,
            })
        })
        .collect()
}
