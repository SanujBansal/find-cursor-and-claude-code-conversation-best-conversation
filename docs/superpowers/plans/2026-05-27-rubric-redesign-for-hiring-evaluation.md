# Rubric Redesign for Hiring Evaluation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the AI-output-focused rubric (6 dimensions) with a hiring-focused rubric that grades the human developer (8 dimensions) through their vibe-coding transcripts.

**Architecture:** Single rubric swap — bump `RUBRIC_VERSION` to `v3` and `PROMPT_VERSION` to `v3` so the cache invalidates, run a destructive migration that drops the old score columns and adds the new ones, then thread the new field names through the Rust DB layer, the LLM JSON schema, the TypeScript types, and the UI label map. Existing scored rows are deleted (per spec).

**Tech Stack:** Rust (Tauri / rusqlite / serde), SQLite, TypeScript / React (Next.js).

**Spec:** `docs/superpowers/specs/2026-05-27-rubric-redesign-for-hiring-evaluation-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src-tauri/src/scoring/rubric.rs` | Modify | New 8-dim `RubricWeights` / `RubricDimensions`, new `RUBRIC_DESCRIPTION`, bump `RUBRIC_VERSION` to `v3`, updated tests. |
| `src-tauri/src/scoring/scorer.rs` | Modify | New `ScorePayload`, new JSON schema sent to the model, new validation/construction. |
| `src-tauri/src/scoring/prompt.rs` | Modify | Bump `PROMPT_VERSION` to `v3`. |
| `src-tauri/migrations/004_rubric_v3_dimensions.sql` | Create | Drop old score columns, add new, wipe stale rows. |
| `src-tauri/src/commands.rs` | Modify | Update `ScoreRecord` + `ConversationWithScore` structs and every SELECT/INSERT that touches dimension columns. |
| `src/lib/types.ts` | Modify | New `RubricDimension` union, `RubricDimensions`, `ScoreRecord`, `ConversationWithScore`. |
| `app/conversations/detail/ConversationDetail.tsx` | Modify | New `DIMENSION_LABELS` map. |

---

## Conventions

- The eight new field names in **snake_case** (Rust) and **camelCase** (TS / DB camelCase via serde rename):

  | snake_case (Rust / SQL) | camelCase (TS / JSON) |
  |---|---|
  | `conceptual_knowledge` | `conceptualKnowledge` |
  | `attention_to_detail` | `attentionToDetail` |
  | `problem_decomposition` | `problemDecomposition` |
  | `critical_evaluation` | `criticalEvaluation` |
  | `robustness_awareness` | `robustnessAwareness` |
  | `debugging_skill` | `debuggingSkill` |
  | `prompt_specificity` | `promptSpecificity` |
  | `scope_discipline` | `scopeDiscipline` |

- Weights (sum to 1.00):

  | field | weight |
  |---|---|
  | `conceptual_knowledge` | 0.18 |
  | `attention_to_detail` | 0.15 |
  | `problem_decomposition` | 0.13 |
  | `criticalEvaluation` | 0.12 |
  | `robustness_awareness` | 0.12 |
  | `debugging_skill` | 0.10 |
  | `prompt_specificity` | 0.10 |
  | `scope_discipline` | 0.10 |

---

### Task 1: Update `rubric.rs` tests + types + weights (TDD)

**Files:**
- Modify: `src-tauri/src/scoring/rubric.rs`

- [ ] **Step 1: Update the existing tests with the new field names and weights**

Replace the `tests` module's helpers and assertions. The math doesn't change (`compute_final_score` is still weighted mean + weakest-link penalty), so the existing numeric expectations stay the same — only field names change. Replace the entire `#[cfg(test)] mod tests { ... }` block at the bottom of `src-tauri/src/scoring/rubric.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn dims_all(value: f64) -> RubricDimensions {
        RubricDimensions {
            conceptual_knowledge: value,
            attention_to_detail: value,
            problem_decomposition: value,
            critical_evaluation: value,
            robustness_awareness: value,
            debugging_skill: value,
            prompt_specificity: value,
            scope_discipline: value,
        }
    }

    #[test]
    fn perfect_score_is_five() {
        let score = compute_final_score(&dims_all(5.0));
        assert!((score - 5.0).abs() < 0.001, "perfect dims should give 5.0, got {}", score);
    }

    #[test]
    fn zero_score_is_zero() {
        let score = compute_final_score(&dims_all(0.0));
        assert!((score - 0.0).abs() < 0.001);
    }

    #[test]
    fn all_fours_has_no_penalty() {
        let score = compute_final_score(&dims_all(4.0));
        assert!((score - 4.0).abs() < 0.001, "all-4s should give 4.0, got {}", score);
    }

    #[test]
    fn weakest_link_penalty_applies() {
        // All 5s with one attention_to_detail=2: weighted_avg = 5 - 3*0.15 = 4.55,
        // penalty = 0.4 * (4-2) = 0.8, final = 3.75.
        let dims = RubricDimensions {
            conceptual_knowledge: 5.0,
            attention_to_detail: 2.0,
            problem_decomposition: 5.0,
            critical_evaluation: 5.0,
            robustness_awareness: 5.0,
            debugging_skill: 5.0,
            prompt_specificity: 5.0,
            scope_discipline: 5.0,
        };
        let score = compute_final_score(&dims);
        assert!(score < 4.0, "weak attention should pull final below 4.0, got {}", score);
        assert!((score - 3.75).abs() < 0.01, "expected 3.75, got {}", score);
    }

    #[test]
    fn single_zero_dimension_caps_top_end() {
        // All 5s except scope_discipline = 0:
        // weighted = 5 - 5*0.10 = 4.5, penalty = 0.4*4 = 1.6, final = 2.9.
        let dims = RubricDimensions {
            conceptual_knowledge: 5.0,
            attention_to_detail: 5.0,
            problem_decomposition: 5.0,
            critical_evaluation: 5.0,
            robustness_awareness: 5.0,
            debugging_skill: 5.0,
            prompt_specificity: 5.0,
            scope_discipline: 0.0,
        };
        let score = compute_final_score(&dims);
        assert!(score < 3.5, "a 0 dim should keep final below 3.5, got {}", score);
        assert!((score - 2.9).abs() < 0.01, "expected ~2.9, got {}", score);
    }

    #[test]
    fn weights_sum_to_one() {
        let w = &DEFAULT_WEIGHTS;
        let sum = w.conceptual_knowledge
            + w.attention_to_detail
            + w.problem_decomposition
            + w.critical_evaluation
            + w.robustness_awareness
            + w.debugging_skill
            + w.prompt_specificity
            + w.scope_discipline;
        assert!((sum - 1.0).abs() < 0.001, "weights must sum to 1.0, got {}", sum);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cd src-tauri && cargo test --lib scoring::rubric`
Expected: compilation error — `RubricDimensions` does not have field `conceptual_knowledge` etc. This is the "red" of red-green.

- [ ] **Step 3: Replace `RubricWeights`, `RubricDimensions`, `DEFAULT_WEIGHTS`, `compute_final_score`, and bump `RUBRIC_VERSION`**

Edit `src-tauri/src/scoring/rubric.rs`. Replace lines 1 through 70 (everything above the `#[cfg(test)]` block) with:

```rust
use serde::{Deserialize, Serialize};

pub const RUBRIC_VERSION: &str = "v3";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubricWeights {
    pub conceptual_knowledge: f64,
    pub attention_to_detail: f64,
    pub problem_decomposition: f64,
    pub critical_evaluation: f64,
    pub robustness_awareness: f64,
    pub debugging_skill: f64,
    pub prompt_specificity: f64,
    pub scope_discipline: f64,
}

pub const DEFAULT_WEIGHTS: RubricWeights = RubricWeights {
    conceptual_knowledge: 0.18,
    attention_to_detail: 0.15,
    problem_decomposition: 0.13,
    critical_evaluation: 0.12,
    robustness_awareness: 0.12,
    debugging_skill: 0.10,
    prompt_specificity: 0.10,
    scope_discipline: 0.10,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubricDimensions {
    pub conceptual_knowledge: f64,
    pub attention_to_detail: f64,
    pub problem_decomposition: f64,
    pub critical_evaluation: f64,
    pub robustness_awareness: f64,
    pub debugging_skill: f64,
    pub prompt_specificity: f64,
    pub scope_discipline: f64,
}

/// Compute the final score using a weighted arithmetic mean with a
/// "weakest-link" penalty. The penalty exists because a candidate who is
/// great on most axes but mediocre on one should NOT round to a perfect 5 —
/// a single weak hiring signal drags the headline number down.
///
/// Penalty: subtract `0.4 * max(0, 4 - min_dim)`.
///   - all 5s → penalty 0  → final 5.00
///   - all 4s → penalty 0  → final 4.00
///   - mostly 5s, one 2   → penalty 0.8 → top-end ~3.75 (varies by weight)
///   - mostly 5s, one 0   → penalty 1.6 → top-end ~2.9
///   - all 0s             → 0.0 (clamped)
pub fn compute_final_score(dims: &RubricDimensions) -> f64 {
    let weighted = dims.conceptual_knowledge * DEFAULT_WEIGHTS.conceptual_knowledge
        + dims.attention_to_detail * DEFAULT_WEIGHTS.attention_to_detail
        + dims.problem_decomposition * DEFAULT_WEIGHTS.problem_decomposition
        + dims.critical_evaluation * DEFAULT_WEIGHTS.critical_evaluation
        + dims.robustness_awareness * DEFAULT_WEIGHTS.robustness_awareness
        + dims.debugging_skill * DEFAULT_WEIGHTS.debugging_skill
        + dims.prompt_specificity * DEFAULT_WEIGHTS.prompt_specificity
        + dims.scope_discipline * DEFAULT_WEIGHTS.scope_discipline;

    let min_dim = [
        dims.conceptual_knowledge,
        dims.attention_to_detail,
        dims.problem_decomposition,
        dims.critical_evaluation,
        dims.robustness_awareness,
        dims.debugging_skill,
        dims.prompt_specificity,
        dims.scope_discipline,
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min);

    let penalty = 0.4 * (4.0_f64 - min_dim).max(0.0);
    (weighted - penalty).clamp(0.0, 5.0)
}
```

Note: this leaves `RUBRIC_DESCRIPTION` untouched for now — Task 2 rewrites it. The file will still compile because `RUBRIC_DESCRIPTION` is only referenced from `prompt.rs`.

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cd src-tauri && cargo test --lib scoring::rubric`
Expected: all 6 tests pass.

- [ ] **Step 5: Commit**

Do not auto-commit if the working tree already has unrelated staged changes — verify with `git status` first. If clean (other than this work), commit:

```bash
git add src-tauri/src/scoring/rubric.rs
git commit -m "rubric: replace AI-output dimensions with hiring-focused dimensions (v3)"
```

---

### Task 2: Rewrite `RUBRIC_DESCRIPTION`

**Files:**
- Modify: `src-tauri/src/scoring/rubric.rs` (the `RUBRIC_DESCRIPTION` constant)

- [ ] **Step 1: Replace the `RUBRIC_DESCRIPTION` constant**

In `src-tauri/src/scoring/rubric.rs`, replace the existing `pub const RUBRIC_DESCRIPTION: &str = r#"..."#;` block (the long calibration prompt) with the following exact content:

```rust
pub const RUBRIC_DESCRIPTION: &str = r#"
You are a SENIOR HIRING MANAGER reviewing a candidate's AI-assisted coding
session (a "vibe coding" transcript). Your job is to grade the CANDIDATE —
the human developer — on signals that matter for hiring. Do NOT grade the
AI assistant. Do NOT grade whether the task got done. Grade the developer's
thinking, judgment, and discipline as revealed in their messages and the
decisions they make about the AI's output.

## Calibration mindset (read this twice)

- DEFAULT every dimension to 2. Move UP only when the transcript shows
  specific, citable evidence from the CANDIDATE's own messages or visible
  decisions. Move DOWN whenever you see a smell.
- A 5 is EXCEPTIONAL — the kind of session you would forward to the hiring
  committee. It is NOT "they got code that compiled."
- The MOST COMMON honest score is 2 or 3. Across 100 sessions, expect ~5
  fives, ~15 fours, ~40 threes, ~30 twos, ~10 ones, and rare zeros.
- A candidate who is mostly silent while the AI does the work is NOT high-
  scoring — they're low-scoring on almost every dimension. Vibe coding is
  not a passive activity; the hireable candidate STEERS.
- Never give a 5 to compensate for low scores elsewhere. Never round up.

## Things that REDUCE every score by at least 1

- The candidate accepts hallucinated APIs, wrong file paths, or fabricated
  signatures without comment.
- The candidate never pushes back on the AI, even when the AI is obviously
  wrong or off-pattern.
- Prompts are vague one-liners ("fix this", "make it work") with no
  context, constraints, or acceptance criteria.
- The candidate doesn't read the AI's diffs — patches are accepted whole.
- No verification of output (no tests run, no behavior checked, no edge
  cases probed).
- The candidate lets scope sprawl into unrelated "improvements" without
  pushback.
- The candidate thrashes when debugging instead of reasoning from evidence.
- The session ends ambiguously (no clear "done", error left dangling).

## Dimensions (score each 0-5 INDEPENDENTLY)

conceptualKnowledge — Does the candidate reason about WHY a solution works?
  5 = Demonstrates deep understanding: explains trade-offs unprompted,
      references correct patterns by name, anticipates downstream effects,
      corrects the AI on conceptual grounds. RARE.
  4 = Shows clear conceptual grasp, with one moment of hand-waving.
  3 = Working understanding of the immediate problem but no broader
      context or trade-off reasoning.
  2 = Surface-level — gets things working but can't explain why.
  1 = Treats the codebase as a black box. Copies and accepts.
  0 = Visibly wrong conceptual claims they don't catch.

attentionToDetail — Do they catch AI mistakes and read diffs carefully?
  5 = Catches multiple non-obvious AI errors. Reads diffs critically,
      asks targeted questions about specific lines, runs verification
      before declaring done. RARE.
  4 = Catches the obvious errors plus at least one subtle one.
  3 = Catches obvious errors but misses subtle ones. Verifies major
      outputs but not all.
  2 = Accepts most AI output without scrutiny. Misses fabrications a
      careful reader would catch.
  1 = Rubber-stamps everything.
  0 = Accepts visibly broken code with no comment.

problemDecomposition — Do they break work into well-scoped, sequenced steps,
                       AND drive it to actual completion?
  5 = Clear stepwise plan, executed in order, with explicit verification
      at the end. No dangling TODOs, no "I think it works." RARE.
  4 = Solid plan with one shortcut — either decomposition fuzzy or close-
      out incomplete.
  3 = Reasonable approach but with a fuzzy step or unverified completion.
  2 = Vague mega-requests; no sequencing. Or "completes" with obvious gaps.
  1 = Single dump with no structure, no verification.
  0 = Chaotic, no direction.

criticalEvaluation — Do they push back on the AI when it's wrong?
  5 = Explicitly challenges the AI on substance multiple times. Rejects
      bad suggestions with specific reasoning. RARE.
  4 = Pushes back at least twice with concrete reasoning.
  3 = Pushes back occasionally but accepts more than they should.
  2 = Rare or weak pushback. Mostly accepts what the AI produces.
  1 = Accepts every AI suggestion. No challenge, no judgment.
  0 = Defers entirely; treats AI output as authoritative.

robustnessAwareness — Do they proactively consider failure modes?
  5 = Raises failure modes the AI didn't mention. Asks about empty / null
      / concurrent / large / malicious inputs. Considers security and
      perf without being prompted. RARE.
  4 = Surfaces at least two real edge cases proactively.
  3 = Considers edge cases when reminded; doesn't lead with them.
  2 = Happy-path-focused. Acknowledges robustness only after a bug.
  1 = Happy-path-only thinking, even after bugs appear.
  0 = Actively dismisses concerns about edge cases.

debuggingSkill — When things break, do they reason from evidence?
  5 = Reads errors carefully, forms a hypothesis, isolates the cause,
      fixes the root cause. Cites specific log lines or stack frames. RARE.
  4 = Evidence-driven debugging with at most one guess.
  3 = Eventually reaches the fix but with detours or guesswork.
  2 = Mostly guesses; arrives at fix by trial and error.
  1 = Thrashes. Random changes. Asks the AI "why doesn't it work" with no
      inspection of evidence.
  0 = Actively makes the bug worse.
  N/A — If the transcript contains nothing to debug, score 3 (neutral)
        and call this out in the explanation.

promptSpecificity — Are prompts precise, contextual, and constraint-aware?
                    (Includes clarifying questions.)
  5 = Prompts are precise, contextual, constraint-aware. Clarifying
      questions probe assumptions and surface hidden requirements. RARE.
  4 = Mostly specific with one or two vague moments.
  3 = Adequate but uneven — some specific, some vague.
  2 = Mostly vague. Lacks context, examples, constraints.
  1 = Vague one-liners throughout. No clarifying questions where obviously
      needed.
  0 = Incoherent or contradictory prompts.

scopeDiscipline — Do they resist gold-plating and stay on task?
  5 = Stays tightly on task. Explicitly reins in the AI when it expands
      scope. No unrelated changes. RARE.
  4 = Mostly on task with one minor scope drift.
  3 = On task overall but accepts one unnecessary refactor or expansion.
  2 = Lets the AI sprawl into unrelated improvements. Mixes concerns.
  1 = Tolerates major off-topic refactors.
  0 = No scope at all — the conversation wanders.

## Output requirements

Score each transcript INDEPENDENTLY. Do NOT grade on a curve against
other transcripts.

Your `explanation` field MUST:
  1. Quote the CANDIDATE'S own words (not the AI's) as evidence for each
     dimension that scored 4 or 5.
  2. Name at least one concrete weakness, even for high-scoring sessions.
     If you cannot find a weakness, the score is too high — lower it.
  3. Be 3-6 sentences. No filler.

If you are unsure between two scores on a dimension, pick the LOWER one.
"#;
```

- [ ] **Step 2: Verify the file still builds**

Run: `cd src-tauri && cargo build`
Expected: build succeeds (this constant is only used in `prompt.rs`).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/scoring/rubric.rs
git commit -m "rubric: rewrite calibration prompt to grade the candidate, not the AI"
```

---

### Task 3: Update `scorer.rs` payload, schema, and validation

**Files:**
- Modify: `src-tauri/src/scoring/scorer.rs`

- [ ] **Step 1: Replace `ScorePayload`**

In `src-tauri/src/scoring/scorer.rs`, replace the existing `ScorePayload` struct (around lines 33–48) with:

```rust
#[derive(serde::Deserialize)]
struct ScorePayload {
    #[serde(rename = "conceptualKnowledge")]
    conceptual_knowledge: i64,
    #[serde(rename = "attentionToDetail")]
    attention_to_detail: i64,
    #[serde(rename = "problemDecomposition")]
    problem_decomposition: i64,
    #[serde(rename = "criticalEvaluation")]
    critical_evaluation: i64,
    #[serde(rename = "robustnessAwareness")]
    robustness_awareness: i64,
    #[serde(rename = "debuggingSkill")]
    debugging_skill: i64,
    #[serde(rename = "promptSpecificity")]
    prompt_specificity: i64,
    #[serde(rename = "scopeDiscipline")]
    scope_discipline: i64,
    explanation: String,
}
```

- [ ] **Step 2: Replace the JSON schema sent to the model**

Replace the `json_schema` block (around lines 90–107) with:

```rust
    let json_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "conceptualKnowledge":   { "type": "integer", "minimum": 0, "maximum": 5 },
            "attentionToDetail":     { "type": "integer", "minimum": 0, "maximum": 5 },
            "problemDecomposition":  { "type": "integer", "minimum": 0, "maximum": 5 },
            "criticalEvaluation":    { "type": "integer", "minimum": 0, "maximum": 5 },
            "robustnessAwareness":   { "type": "integer", "minimum": 0, "maximum": 5 },
            "debuggingSkill":        { "type": "integer", "minimum": 0, "maximum": 5 },
            "promptSpecificity":     { "type": "integer", "minimum": 0, "maximum": 5 },
            "scopeDiscipline":       { "type": "integer", "minimum": 0, "maximum": 5 },
            "explanation":           { "type": "string" }
        },
        "required": [
            "conceptualKnowledge", "attentionToDetail", "problemDecomposition",
            "criticalEvaluation", "robustnessAwareness", "debuggingSkill",
            "promptSpecificity", "scopeDiscipline", "explanation"
        ],
        "additionalProperties": false
    });
```

- [ ] **Step 3: Replace the validation loop and `RubricDimensions` construction**

Replace the validation-loop + `dimensions` construction block (the section that iterates over named tuples like `("taskCompletion", payload.task_completion)` and the `let dimensions = RubricDimensions { ... }` that follows) with:

```rust
    for (name, val) in [
        ("conceptualKnowledge", payload.conceptual_knowledge),
        ("attentionToDetail", payload.attention_to_detail),
        ("problemDecomposition", payload.problem_decomposition),
        ("criticalEvaluation", payload.critical_evaluation),
        ("robustnessAwareness", payload.robustness_awareness),
        ("debuggingSkill", payload.debugging_skill),
        ("promptSpecificity", payload.prompt_specificity),
        ("scopeDiscipline", payload.scope_discipline),
    ] {
        if !(0..=5).contains(&val) {
            return Err(format!("Dimension '{name}' value {val} is out of range 0-5"));
        }
    }

    let dimensions = RubricDimensions {
        conceptual_knowledge: payload.conceptual_knowledge as f64,
        attention_to_detail: payload.attention_to_detail as f64,
        problem_decomposition: payload.problem_decomposition as f64,
        critical_evaluation: payload.critical_evaluation as f64,
        robustness_awareness: payload.robustness_awareness as f64,
        debugging_skill: payload.debugging_skill as f64,
        prompt_specificity: payload.prompt_specificity as f64,
        scope_discipline: payload.scope_discipline as f64,
    };
```

- [ ] **Step 4: Run `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: `scorer.rs` compiles. `commands.rs` may still fail to compile — that's fine, Task 6 fixes it.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/scoring/scorer.rs
git commit -m "scorer: update payload + JSON schema for v3 hiring rubric"
```

---

### Task 4: Bump `PROMPT_VERSION` to `v3`

**Files:**
- Modify: `src-tauri/src/scoring/prompt.rs`

- [ ] **Step 1: Bump the constant**

In `src-tauri/src/scoring/prompt.rs`, change:

```rust
pub const PROMPT_VERSION: &str = "v2";
```

to:

```rust
pub const PROMPT_VERSION: &str = "v3";
```

The prompt body itself (`build_prompt`) does not change — it still injects `RUBRIC_DESCRIPTION`, which Task 2 already rewrote.

- [ ] **Step 2: Run the prompt tests**

Run: `cd src-tauri && cargo test --lib scoring::prompt`
Expected: both `cache_key_changes_with_content_hash` and `cache_key_changes_with_model` pass — they don't assert on the literal version value.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/scoring/prompt.rs
git commit -m "prompt: bump PROMPT_VERSION to v3 to invalidate v2 cache"
```

---

### Task 5: Create migration `004_rubric_v3_dimensions.sql`

**Files:**
- Create: `src-tauri/migrations/004_rubric_v3_dimensions.sql`

- [ ] **Step 1: Write the migration**

Create `src-tauri/migrations/004_rubric_v3_dimensions.sql` with this exact content:

```sql
-- Replace v2 AI-output rubric (6 dimensions) with v3 hiring rubric (8 dimensions).
-- Existing scored rows are incomparable across rubrics and are deleted.
-- Aggregates derived from `scores` are also wiped and will be recomputed
-- by `refresh_analytics` on the next scoring run.

DELETE FROM daily_scores;
DELETE FROM weekly_scores;
DELETE FROM scores;

ALTER TABLE scores DROP COLUMN task_completion;
ALTER TABLE scores DROP COLUMN technical_correctness;
ALTER TABLE scores DROP COLUMN workflow_quality;
ALTER TABLE scores DROP COLUMN tool_use_and_context;
ALTER TABLE scores DROP COLUMN communication_clarity;
ALTER TABLE scores DROP COLUMN learning_leverage;

ALTER TABLE scores ADD COLUMN conceptual_knowledge   REAL NOT NULL DEFAULT 0 CHECK (conceptual_knowledge   >= 0 AND conceptual_knowledge   <= 5);
ALTER TABLE scores ADD COLUMN attention_to_detail    REAL NOT NULL DEFAULT 0 CHECK (attention_to_detail    >= 0 AND attention_to_detail    <= 5);
ALTER TABLE scores ADD COLUMN problem_decomposition  REAL NOT NULL DEFAULT 0 CHECK (problem_decomposition  >= 0 AND problem_decomposition  <= 5);
ALTER TABLE scores ADD COLUMN critical_evaluation    REAL NOT NULL DEFAULT 0 CHECK (critical_evaluation    >= 0 AND critical_evaluation    <= 5);
ALTER TABLE scores ADD COLUMN robustness_awareness   REAL NOT NULL DEFAULT 0 CHECK (robustness_awareness   >= 0 AND robustness_awareness   <= 5);
ALTER TABLE scores ADD COLUMN debugging_skill        REAL NOT NULL DEFAULT 0 CHECK (debugging_skill        >= 0 AND debugging_skill        <= 5);
ALTER TABLE scores ADD COLUMN prompt_specificity     REAL NOT NULL DEFAULT 0 CHECK (prompt_specificity     >= 0 AND prompt_specificity     <= 5);
ALTER TABLE scores ADD COLUMN scope_discipline       REAL NOT NULL DEFAULT 0 CHECK (scope_discipline       >= 0 AND scope_discipline       <= 5);
```

Notes:
- The `DELETE` statements come *before* the `DROP COLUMN`s because deleting empty tables is harmless but dropping non-existent columns on rows with stale data is fine — order chosen for readability.
- `ALTER TABLE DROP COLUMN` requires SQLite ≥ 3.35 (March 2021); rusqlite 0.31+ bundles a modern SQLite. If a developer is on an older SQLite via a system feature flag, this migration will fail with a clear error — that's correct behavior.
- The migration runner in `src-tauri/src/db.rs` already uses `execute_batch`, which runs the whole file as a transaction.

- [ ] **Step 2: Verify the migration applies cleanly on a fresh DB**

Manual verification (since the project has no DB integration test harness):

```bash
cd src-tauri
# Build to confirm migration file is discovered
cargo build
```

Then run the app once (`npm run dev`) on a dev machine; the migration runs at startup and any failure aborts initialization with a logged error.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/migrations/004_rubric_v3_dimensions.sql
git commit -m "db: migration 004 — replace v2 score columns with v3 hiring dimensions"
```

---

### Task 6: Update `commands.rs` (structs + every SELECT/INSERT)

**Files:**
- Modify: `src-tauri/src/commands.rs`

Eight call sites need updating. Touch them in order so `cargo check` becomes progressively cleaner.

- [ ] **Step 1: Update `ScoreRecord` struct (around lines 1278–1298)**

Replace the six `task_completion … learning_leverage` fields with the eight new ones, preserving order to match the new column order in the schema:

```rust
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
```

- [ ] **Step 2: Update `ConversationWithScore` struct (around lines 1303–1322)**

Replace the dimension fields:

```rust
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
```

- [ ] **Step 3: Update `get_dashboard`'s top-3 SELECT (around lines 211–252)**

Replace the SELECT column list and `query_map` row reads. The SELECT must list the eight new columns in place of the six old ones (same position 9–14, now 9–16):

```rust
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
```

And the row reader:

```rust
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
```

- [ ] **Step 4: Update `compute_weak_rubrics` (around lines 472–524)**

Replace the function body:

```rust
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
```

- [ ] **Step 5: Update `get_project_top_conversations` SELECT + reader (around lines 1368–1428)**

The pattern is identical to Step 3. Update the SELECT column list to the eight new dimension columns (replacing the six old ones in the same position group) and shift the row indices in the `ConversationWithScore` construction from `9..14` to `9..16`, then bump `explanation/model_id/scored_at` indices to `17/18/19`. Apply the same field substitution as in Step 3 to this `query_map` block.

- [ ] **Step 6: Update `persist_scoring_results` INSERT (around lines 1629–1671)**

Replace the `INSERT INTO scores` block:

```rust
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
```

- [ ] **Step 7: Update `get_scores` SELECTs (around lines 1685–1721) and `map_score_row` (around lines 1725–1745)**

Both SELECTs (filtered by `conversation_id` and unfiltered) must list the eight new dimension columns in place of the six old ones, in the order they appear in `ScoreRecord` (positions 2–9). After the eight dimensions come `final_score, explanation, model_id, rubric_version, prompt_version, content_hash, cache_key, scored_at, created_at` (positions 10–18).

Replace the filtered SELECT:

```rust
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
```

Replace the unfiltered SELECT identically except dropping the `WHERE conversation_id = ?1` clause. Replace `map_score_row`:

```rust
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
```

- [ ] **Step 8: Update `get_top_conversations` SELECT + reader (around lines 1755–1814)**

Identical pattern to Step 3 / Step 5. Apply the same column-list and row-index changes to this `query_map` block (the SELECT is structurally the same as `get_project_top_conversations`'s SELECT but without the project filter).

- [ ] **Step 9: Run `cargo check` and `cargo test`**

Run: `cd src-tauri && cargo check`
Expected: zero errors.

Run: `cd src-tauri && cargo test`
Expected: all existing tests pass, including the six rubric tests added in Task 1.

If a SELECT column count or `row.get(N)` index is off, the compile-time errors (or test runtime errors) will point to the exact line. Re-read the affected block against the order in `ScoreRecord` / `ConversationWithScore`.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "commands: thread v3 hiring rubric dimensions through DB layer"
```

---

### Task 7: Update TypeScript types

**Files:**
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Replace the `RubricDimension` union and dimension types**

Edit `src/lib/types.ts`:

1. Replace lines 16–22 (`export type RubricDimension = ...`):

```ts
export type RubricDimension =
  | "conceptualKnowledge"
  | "attentionToDetail"
  | "problemDecomposition"
  | "criticalEvaluation"
  | "robustnessAwareness"
  | "debuggingSkill"
  | "promptSpecificity"
  | "scopeDiscipline";
```

2. Replace the existing `RubricDimensions` interface (around lines 164–171):

```ts
export interface RubricDimensions {
  conceptualKnowledge: number;
  attentionToDetail: number;
  problemDecomposition: number;
  criticalEvaluation: number;
  robustnessAwareness: number;
  debuggingSkill: number;
  promptSpecificity: number;
  scopeDiscipline: number;
}
```

3. In `ScoreRecord` (around lines 186–204), replace the six dimension fields with the eight new ones (keep all other fields):

```ts
  conceptualKnowledge: number;
  attentionToDetail: number;
  problemDecomposition: number;
  criticalEvaluation: number;
  robustnessAwareness: number;
  debuggingSkill: number;
  promptSpecificity: number;
  scopeDiscipline: number;
```

4. In `ConversationWithScore` (around lines 216–221), replace the six dimension fields with the eight `number | null`:

```ts
  conceptualKnowledge: number | null;
  attentionToDetail: number | null;
  problemDecomposition: number | null;
  criticalEvaluation: number | null;
  robustnessAwareness: number | null;
  debuggingSkill: number | null;
  promptSpecificity: number | null;
  scopeDiscipline: number | null;
```

- [ ] **Step 2: Run the linter / typechecker**

Run: `npm run lint`
Expected: any remaining usage of `taskCompletion` etc. surfaces as a lint/type error (specifically in `ConversationDetail.tsx`, which Task 8 fixes).

- [ ] **Step 3: Commit**

```bash
git add src/lib/types.ts
git commit -m "types: update RubricDimension union and related interfaces to v3"
```

---

### Task 8: Update UI labels

**Files:**
- Modify: `app/conversations/detail/ConversationDetail.tsx`

- [ ] **Step 1: Replace `DIMENSION_LABELS`**

In `app/conversations/detail/ConversationDetail.tsx`, replace lines 17–24 with:

```tsx
const DIMENSION_LABELS: Record<string, string> = {
  conceptualKnowledge: "Conceptual Knowledge",
  attentionToDetail: "Attention to Detail",
  problemDecomposition: "Problem Decomposition",
  criticalEvaluation: "Critical Evaluation",
  robustnessAwareness: "Robustness Awareness",
  debuggingSkill: "Debugging Skill",
  promptSpecificity: "Prompt Specificity",
  scopeDiscipline: "Scope Discipline",
};

const DIMENSION_KEYS = Object.keys(DIMENSION_LABELS);
```

No other changes are needed in this file — `ScoreBar` already takes a generic label and value, and the iteration uses the `Object.keys(DIMENSION_LABELS)` array.

- [ ] **Step 2: Run linter and a production build**

Run: `npm run lint`
Expected: zero errors.

Run: `npm run build:web`
Expected: Next.js production build succeeds.

- [ ] **Step 3: Commit**

```bash
git add app/conversations/detail/ConversationDetail.tsx
git commit -m "ui: update conversation detail labels to v3 hiring rubric"
```

---

### Task 9: End-to-end verification

**Files:** (none — verification only)

- [ ] **Step 1: Run the full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: all tests pass. Specifically:
- 6 tests in `scoring::rubric::tests` (Task 1).
- 2 tests in `scoring::prompt::tests`.
- All other pre-existing tests.

- [ ] **Step 2: Run the frontend checks**

Run: `npm run lint && npm run build:web`
Expected: both succeed.

- [ ] **Step 3: Smoke-test the desktop app**

Run: `npm run dev`

In the running app:

1. The migration runs at startup. Watch the terminal for any migration error (`Migration 004_rubric_v3_dimensions.sql failed`). If it succeeds, no log appears.
2. Navigate to the Conversations list. Any previously-scored conversations now show `null` for `final_score` (because their scores were deleted) — they should render as "unscored" in the UI.
3. Pick one conversation, click "Score this conversation". Confirm:
   - The LLM call succeeds (Azure credentials must be set).
   - The conversation detail page now shows the eight new dimension bars labelled `Conceptual Knowledge`, `Attention to Detail`, `Problem Decomposition`, `Critical Evaluation`, `Robustness Awareness`, `Debugging Skill`, `Prompt Specificity`, `Scope Discipline`.
   - The `Rubric` metadata field reads `v3`, `Prompt` reads `v3`.
4. Return to the dashboard. The "Weakest Rubrics" widget should show up to three of the new dimension labels (e.g. "Critical Evaluation", not "Tool Use & Context").

- [ ] **Step 4: No final commit needed**

All commits were made per task. Run `git status` to confirm a clean working tree.

---

## Self-Review (completed by plan author)

**1. Spec coverage:**

| Spec section | Implementing task(s) |
|---|---|
| 8 new dimensions with 0/3/5 anchors | Task 2 (`RUBRIC_DESCRIPTION`) |
| Weights summing to 1.00 | Task 1 (`DEFAULT_WEIGHTS`) + test in `weights_sum_to_one` |
| Weakest-link penalty preserved | Task 1 (formula unchanged) |
| Schema drop+add+wipe | Task 5 (migration 004) |
| `RUBRIC_VERSION` → v3 | Task 1 |
| `PROMPT_VERSION` → v3 | Task 4 |
| New `RUBRIC_DESCRIPTION` framed for the human | Task 2 |
| Rust DB layer updated | Task 6 |
| Frontend types updated | Task 7 |
| UI labels updated | Task 8 |

No gaps.

**2. Placeholder scan:** No "TBD", no "TODO", no "implement later", no generic "add appropriate error handling". Every code block is the actual code.

**3. Type consistency:** All eight field names appear in the same order (`conceptual_knowledge`, `attention_to_detail`, `problem_decomposition`, `critical_evaluation`, `robustness_awareness`, `debugging_skill`, `prompt_specificity`, `scope_discipline`) across `RubricWeights`, `RubricDimensions`, `ScorePayload`, `ScoreRecord`, `ConversationWithScore`, the migration, every SELECT/INSERT, the JSON schema, and the TS interfaces. Row indices in `query_map` blocks consistently start dimensions at position 9 (for `ConversationWithScore`) or 2 (for `ScoreRecord`).
