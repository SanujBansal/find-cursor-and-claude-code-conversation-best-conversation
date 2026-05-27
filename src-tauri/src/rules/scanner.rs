use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::tech_stack::{detect_tech_stack, TechStack};

/// Maximum bytes we read from a single rule file. AI-instruction docs are
/// almost always small; this just protects us from accidentally reading a
/// huge file the user committed under an unusual name.
const MAX_FILE_BYTES: u64 = 256 * 1024;

/// Hard cap on total rule-content size returned to callers / sent to the LLM.
/// Keeps prompts bounded even when a project carries dozens of `.mdc` rules.
const MAX_TOTAL_BYTES: usize = 40_000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuleKind {
    /// `AGENTS.md` (root or near-root)
    Agents,
    /// `CLAUDE.md`
    Claude,
    /// `GEMINI.md`
    Gemini,
    /// `.cursorrules` (legacy single-file format)
    CursorLegacy,
    /// Anything inside `.cursor/rules/*` (`.md` or `.mdc`)
    CursorRule,
    /// `.windsurfrules` / `.windsurf/rules/*`
    Windsurf,
    /// `.github/copilot-instructions.md`
    Copilot,
    /// `.aiderrules`, `.aider.conf.yml`
    Aider,
    /// Any other `*.md` matching a known instruction filename (e.g. `INSTRUCTIONS.md`)
    Other,
}

impl RuleKind {
    pub fn label(self) -> &'static str {
        match self {
            RuleKind::Agents => "AGENTS.md",
            RuleKind::Claude => "Claude rules",
            RuleKind::Gemini => "Gemini rules",
            RuleKind::CursorLegacy => ".cursorrules",
            RuleKind::CursorRule => "Cursor rule",
            RuleKind::Windsurf => "Windsurf rules",
            RuleKind::Copilot => "Copilot instructions",
            RuleKind::Aider => "Aider config",
            RuleKind::Other => "Other instructions",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFile {
    /// Path relative to the project root.
    pub relative_path: String,
    /// Absolute path on disk (useful for the UI's "Reveal in Finder").
    pub absolute_path: String,
    pub kind: RuleKind,
    pub bytes: u64,
    pub content: String,
    /// `true` if the file exceeded `MAX_FILE_BYTES` and was truncated.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRulesReport {
    pub project_path: String,
    pub project_name: String,
    pub exists: bool,
    pub tech_stack: TechStack,
    pub rule_files: Vec<RuleFile>,
    /// Total bytes of *included* content across `rule_files` (post-truncation).
    pub total_bytes: usize,
    /// Stable sha256 of (tech stack JSON || sorted rule contents). Used as
    /// the cache key for LLM scoring — changes whenever rules or stack
    /// detection changes.
    pub content_hash: String,
}

/// Walk a single project directory and collect:
///   1. Tech-stack signals (package.json, Cargo.toml, etc.)
///   2. AI-instruction files (AGENTS.md, .cursor/rules/*, etc.)
///
/// Returns `exists = false` when the path is not a directory (e.g. the
/// project lives somewhere else now, or it's the synthetic "Unassigned"
/// pseudo-project).
pub fn scan_project_rules(project_path: &str) -> ProjectRulesReport {
    let root = PathBuf::from(project_path);
    let project_name = project_display_name(&root);

    if !root.is_dir() {
        let stack = TechStack::default();
        let hash = compute_content_hash(&stack, &[]);
        return ProjectRulesReport {
            project_path: project_path.to_string(),
            project_name,
            exists: false,
            tech_stack: stack,
            rule_files: Vec::new(),
            total_bytes: 0,
            content_hash: hash,
        };
    }

    let tech_stack = detect_tech_stack(&root);
    let rule_files = collect_rule_files(&root);
    let total_bytes: usize = rule_files.iter().map(|f| f.content.len()).sum();
    let content_hash = compute_content_hash(&tech_stack, &rule_files);

    ProjectRulesReport {
        project_path: project_path.to_string(),
        project_name,
        exists: true,
        tech_stack,
        rule_files,
        total_bytes,
        content_hash,
    }
}

fn project_display_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().to_string())
}

/// Scan all known instruction-file locations. We stay shallow (<=3 levels)
/// to avoid descending into `node_modules`, `target`, etc. and never recurse
/// into directories whose name matches `is_ignored_dir`.
fn collect_rule_files(root: &Path) -> Vec<RuleFile> {
    let mut found: Vec<RuleFile> = Vec::new();
    let mut total_bytes: usize = 0;

    let candidates: [(RuleKind, &str); 7] = [
        (RuleKind::Agents, "AGENTS.md"),
        (RuleKind::Claude, "CLAUDE.md"),
        (RuleKind::Gemini, "GEMINI.md"),
        (RuleKind::CursorLegacy, ".cursorrules"),
        (RuleKind::Windsurf, ".windsurfrules"),
        (RuleKind::Aider, ".aiderrules"),
        (RuleKind::Copilot, ".github/copilot-instructions.md"),
    ];

    for (kind, rel) in candidates {
        let path = root.join(rel);
        if let Some(rule) = read_rule_file(&path, root, kind, &mut total_bytes) {
            found.push(rule);
        }
    }

    // Cursor rules directory: `.cursor/rules/*.md` and `.cursor/rules/*.mdc`
    let cursor_rules_dir = root.join(".cursor").join("rules");
    collect_directory(
        &cursor_rules_dir,
        root,
        RuleKind::CursorRule,
        &["md", "mdc"],
        &mut found,
        &mut total_bytes,
        2,
    );

    // Windsurf rules directory: `.windsurf/rules/*.md`
    let windsurf_rules_dir = root.join(".windsurf").join("rules");
    collect_directory(
        &windsurf_rules_dir,
        root,
        RuleKind::Windsurf,
        &["md"],
        &mut found,
        &mut total_bytes,
        2,
    );

    // Sort: bigger kinds first, then by relative path for stable display.
    found.sort_by(|a, b| {
        a.kind
            .label()
            .cmp(b.kind.label())
            .then_with(|| a.relative_path.cmp(&b.relative_path))
    });

    found
}

fn collect_directory(
    dir: &Path,
    root: &Path,
    kind: RuleKind,
    extensions: &[&str],
    found: &mut Vec<RuleFile>,
    total_bytes: &mut usize,
    max_depth: usize,
) {
    if !dir.is_dir() {
        return;
    }
    walk_directory(dir, root, kind, extensions, found, total_bytes, max_depth);
}

fn walk_directory(
    dir: &Path,
    root: &Path,
    kind: RuleKind,
    extensions: &[&str],
    found: &mut Vec<RuleFile>,
    total_bytes: &mut usize,
    remaining_depth: usize,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            if remaining_depth == 0 {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_ignored_dir(name) {
                    continue;
                }
            }
            walk_directory(
                &path,
                root,
                kind,
                extensions,
                found,
                total_bytes,
                remaining_depth - 1,
            );
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let ext_matches = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| extensions.iter().any(|wanted| wanted.eq_ignore_ascii_case(e)))
            .unwrap_or(false);

        if !ext_matches {
            continue;
        }

        if let Some(rule) = read_rule_file(&path, root, kind, total_bytes) {
            found.push(rule);
        }
    }
}

fn read_rule_file(
    path: &Path,
    root: &Path,
    kind: RuleKind,
    total_bytes: &mut usize,
) -> Option<RuleFile> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    if *total_bytes >= MAX_TOTAL_BYTES {
        return None;
    }

    let bytes = metadata.len();
    let truncated_file = bytes > MAX_FILE_BYTES;
    let read_limit = bytes.min(MAX_FILE_BYTES) as usize;

    let raw = fs::read(path).ok()?;
    let snippet = if raw.len() > read_limit {
        &raw[..read_limit]
    } else {
        &raw[..]
    };

    let mut content = String::from_utf8_lossy(snippet).into_owned();

    let remaining_budget = MAX_TOTAL_BYTES.saturating_sub(*total_bytes);
    let mut truncated_for_budget = false;
    if content.len() > remaining_budget {
        // Trim on a char boundary to avoid splitting multi-byte UTF-8.
        let mut cut = remaining_budget;
        while cut > 0 && !content.is_char_boundary(cut) {
            cut -= 1;
        }
        content.truncate(cut);
        truncated_for_budget = true;
    }
    *total_bytes += content.len();

    let relative_path = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string_lossy().to_string());

    Some(RuleFile {
        relative_path,
        absolute_path: path.to_string_lossy().to_string(),
        kind,
        bytes,
        content,
        truncated: truncated_file || truncated_for_budget,
    })
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "target"
            | "dist"
            | "build"
            | "out"
            | ".git"
            | ".next"
            | ".turbo"
            | ".cache"
            | ".venv"
            | "venv"
            | "__pycache__"
            | ".gradle"
            | ".idea"
            | ".vscode"
    )
}

fn compute_content_hash(stack: &TechStack, files: &[RuleFile]) -> String {
    let mut hasher = Sha256::new();
    if let Ok(stack_json) = serde_json::to_string(stack) {
        hasher.update(stack_json.as_bytes());
    }

    // Hash files sorted by relative_path for determinism.
    let mut sorted: Vec<&RuleFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    for f in sorted {
        hasher.update(b"\n--\n");
        hasher.update(f.relative_path.as_bytes());
        hasher.update(b"\n");
        hasher.update(f.content.as_bytes());
    }

    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let base = std::env::temp_dir().join(format!(
                "vibe-rules-test-{}-{}",
                label,
                std::process::id()
            ));
            if base.exists() {
                let _ = std::fs::remove_dir_all(&base);
            }
            std::fs::create_dir_all(&base).unwrap();
            Self { path: base }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn missing_project_path_reports_does_not_exist() {
        let report = scan_project_rules("/this/path/definitely/does/not/exist");
        assert!(!report.exists);
        assert!(report.rule_files.is_empty());
    }

    #[test]
    fn finds_root_level_rule_files() {
        let dir = TempDir::new("root");
        write_file(dir.path(), "AGENTS.md", "# Agents");
        write_file(dir.path(), "CLAUDE.md", "# Claude");
        write_file(dir.path(), ".cursorrules", "always write tests");

        let report = scan_project_rules(dir.path().to_str().unwrap());
        let kinds: Vec<RuleKind> = report.rule_files.iter().map(|f| f.kind).collect();
        assert!(kinds.contains(&RuleKind::Agents));
        assert!(kinds.contains(&RuleKind::Claude));
        assert!(kinds.contains(&RuleKind::CursorLegacy));
    }

    #[test]
    fn finds_cursor_rules_directory() {
        let dir = TempDir::new("cursor-dir");
        write_file(
            dir.path(),
            ".cursor/rules/typescript.mdc",
            "use strict typing",
        );
        write_file(dir.path(), ".cursor/rules/react.md", "prefer hooks");

        let report = scan_project_rules(dir.path().to_str().unwrap());
        let cursor_rules: Vec<&RuleFile> = report
            .rule_files
            .iter()
            .filter(|f| f.kind == RuleKind::CursorRule)
            .collect();
        assert_eq!(cursor_rules.len(), 2);
    }

    #[test]
    fn content_hash_is_deterministic() {
        let dir = TempDir::new("hash");
        write_file(dir.path(), "AGENTS.md", "a");
        write_file(dir.path(), "CLAUDE.md", "b");

        let h1 = scan_project_rules(dir.path().to_str().unwrap()).content_hash;
        let h2 = scan_project_rules(dir.path().to_str().unwrap()).content_hash;
        assert_eq!(h1, h2);
    }

    #[test]
    fn ignores_node_modules() {
        let dir = TempDir::new("ignored");
        write_file(dir.path(), "AGENTS.md", "root");
        write_file(
            dir.path(),
            "node_modules/pkg/.cursor/rules/foo.mdc",
            "should be ignored",
        );

        let report = scan_project_rules(dir.path().to_str().unwrap());
        // Only root AGENTS.md should be picked up.
        assert_eq!(report.rule_files.len(), 1);
        assert_eq!(report.rule_files[0].kind, RuleKind::Agents);
    }
}
