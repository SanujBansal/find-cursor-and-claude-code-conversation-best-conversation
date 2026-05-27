-- Stored LLM-graded scores for a project's AI instruction files
-- (AGENTS.md, CLAUDE.md, .cursor/rules/*, etc.) vs. the detected tech stack.

CREATE TABLE IF NOT EXISTS project_rule_scores (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_path TEXT NOT NULL UNIQUE,
  tech_stack_json TEXT NOT NULL,       -- serialized TechStack
  rule_files_json TEXT NOT NULL,       -- serialized list of file descriptors (path, kind, byte size)
  content_hash TEXT NOT NULL,          -- sha256 of concatenated rule file contents + tech stack
  coverage REAL NOT NULL CHECK (coverage >= 0 AND coverage <= 5),
  stack_alignment REAL NOT NULL CHECK (stack_alignment >= 0 AND stack_alignment <= 5),
  specificity REAL NOT NULL CHECK (specificity >= 0 AND specificity <= 5),
  actionability REAL NOT NULL CHECK (actionability >= 0 AND actionability <= 5),
  overall_score REAL NOT NULL CHECK (overall_score >= 0 AND overall_score <= 5),
  summary TEXT,
  suggestions_json TEXT NOT NULL,      -- JSON array of suggested improvements
  model_id TEXT NOT NULL,
  rubric_version TEXT NOT NULL,
  scored_at TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_project_rule_scores_path
  ON project_rule_scores(project_path);
