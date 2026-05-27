-- Core schema for Vibe Score local SQLite database

CREATE TABLE IF NOT EXISTS sources (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_type TEXT NOT NULL CHECK (
    source_type IN ('cursor-local', 'claude-code-local', 'claude-web-markdown')
  ),
  name TEXT NOT NULL,
  path TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS projects (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  workspace_path TEXT,
  slug TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS conversations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
  source_id INTEGER REFERENCES sources(id) ON DELETE SET NULL,
  external_id TEXT,
  provider TEXT NOT NULL,
  title TEXT NOT NULL,
  source_path TEXT,
  content_hash TEXT NOT NULL,
  message_count INTEGER NOT NULL DEFAULT 0,
  tool_call_count INTEGER NOT NULL DEFAULT 0,
  started_at TEXT,
  completed_at TEXT,
  imported_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_completed_at ON conversations(completed_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_content_hash ON conversations(content_hash);

CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  tool_name TEXT,
  sequence_num INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(conversation_id, sequence_num)
);

CREATE TABLE IF NOT EXISTS scores (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL UNIQUE REFERENCES conversations(id) ON DELETE CASCADE,
  task_completion REAL NOT NULL CHECK (task_completion >= 0 AND task_completion <= 5),
  technical_correctness REAL NOT NULL CHECK (technical_correctness >= 0 AND technical_correctness <= 5),
  workflow_quality REAL NOT NULL CHECK (workflow_quality >= 0 AND workflow_quality <= 5),
  tool_use_and_context REAL NOT NULL CHECK (tool_use_and_context >= 0 AND tool_use_and_context <= 5),
  communication_clarity REAL NOT NULL CHECK (communication_clarity >= 0 AND communication_clarity <= 5),
  learning_leverage REAL NOT NULL CHECK (learning_leverage >= 0 AND learning_leverage <= 5),
  final_score REAL NOT NULL CHECK (final_score >= 0 AND final_score <= 5),
  explanation TEXT,
  model_id TEXT NOT NULL,
  rubric_version TEXT NOT NULL,
  prompt_version TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  scored_at TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS daily_scores (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  score_date TEXT NOT NULL UNIQUE,
  average_score REAL NOT NULL,
  conversation_count INTEGER NOT NULL,
  total_effort_weight REAL NOT NULL,
  computed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS weekly_scores (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  week_start TEXT NOT NULL UNIQUE,
  average_score REAL NOT NULL,
  active_days INTEGER NOT NULL,
  computed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  job_type TEXT NOT NULL CHECK (
    job_type IN ('import', 'score', 'aggregate')
  ),
  status TEXT NOT NULL CHECK (
    status IN ('pending', 'running', 'completed', 'failed')
  ),
  payload TEXT,
  progress REAL NOT NULL DEFAULT 0 CHECK (progress >= 0 AND progress <= 1),
  error_message TEXT,
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
