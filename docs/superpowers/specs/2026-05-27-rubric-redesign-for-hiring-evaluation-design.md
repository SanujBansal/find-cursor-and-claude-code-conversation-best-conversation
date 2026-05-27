# Rubric Redesign for Hiring Evaluation

**Status:** Draft – awaiting user review
**Date:** 2026-05-27

## Problem

Vibe Score's current rubric (`src-tauri/src/scoring/rubric.rs`, v2) grades the *AI assistant's output* across six dimensions: `taskCompletion`, `technicalCorrectness`, `workflowQuality`, `toolUseAndContext`, `communicationClarity`, `learningLeverage`. The user wants to use Vibe Score as a **hiring tool** — to evaluate *human developers* through their vibe-coding transcripts. The current rubric doesn't measure the human's contribution. A copy-paster who got working code scores the same as an engineer who steered the AI through thoughtful design.

## Goal

Pivot the rubric to grade the **human developer** on hiring-relevant signal: depth of understanding, precision of communication, scrutiny of AI output, problem-solving discipline, and engineering judgment.

## Non-goals

- Preserving comparability with previously-scored conversations. Bumping `RUBRIC_VERSION` invalidates the score cache by design; old scored rows will be re-scored under the new rubric.
- Adding new hiring-workflow UI (candidate roll-ups, recruiter views). The existing dashboard re-skins to the new dimension names; richer hiring-only views are out of scope.
- Re-tuning the weakest-link penalty formula. It works as-is and the principle (one weak axis drags the headline) is correct for hiring too.

## Design

### Eight new dimensions (each scored 0–5)

Each dimension is graded *based on what the transcript reveals about the human*, not the AI. Every dimension defaults to **2** and only moves up with specific, citable evidence. A 5 is exceptional and rare.

#### 1. `conceptualKnowledge` — weight **0.18**

Does the developer reason about *why* a solution works? Do they show awareness of architecture, patterns, complexity, trade-offs, performance/security implications? Or do they treat code as a black box and ship whatever compiles?

- **5** — Demonstrates deep understanding: explains trade-offs unprompted, references correct patterns by name, anticipates downstream effects, corrects the AI on conceptual grounds.
- **3** — Shows working understanding of the immediate problem but no broader context or trade-off reasoning.
- **1** — Treats the codebase as a black box. Cannot explain *why* changes work; only that they work.

#### 2. `attentionToDetail` — weight **0.15** (absorbs `codeReviewInstinct`)

Does the developer catch AI hallucinations (fabricated APIs, wrong file paths, wrong signatures)? Do they notice subtle bugs, off-by-ones, mis-typed code? Do they actually *read* the AI's diff line-by-line, or accept whole patches blindly? Verification before acceptance.

- **5** — Catches multiple non-obvious AI errors. Reads diffs critically, asks targeted questions about specific lines, runs verification before declaring done.
- **3** — Catches obvious errors but misses subtle ones. Verifies major outputs but not all.
- **1** — Accepts AI output without scrutiny. Misses fabrications and bugs that a careful reader would catch.

#### 3. `problemDecomposition` — weight **0.13** (absorbs `ownership`)

Does the developer break work into well-scoped steps and *sequence them logically*? Does the work actually reach completion — verified, edge cases handled, loose ends tied — or do they declare done at first green light? Decomposition that doesn't close the loop isn't real decomposition.

- **5** — Clear stepwise plan, executed in order, with explicit verification at the end. No dangling TODOs, no "I think it works."
- **3** — Reasonable plan with one shortcut: either decomposition is fuzzy or the close-out is incomplete.
- **1** — Vague mega-requests, no sequencing. Or work that "completes" with obvious gaps.

#### 4. `criticalEvaluation` — weight **0.12**

Does the developer push back on the AI when it's wrong? Question suggestions? Reject overengineered, off-pattern, or wrong code? The opposite of "the AI is always right."

- **5** — Explicitly challenges the AI on substance multiple times. Rejects bad suggestions with specific reasoning.
- **3** — Pushes back occasionally but accepts more than they should.
- **1** — Accepts every AI suggestion. No challenge, no judgment.

#### 5. `robustnessAwareness` — weight **0.12**

Without being prompted by the AI, does the developer think about failure modes, error handling, security, performance, edge cases? Proactive risk-thinking, not reactive bug-fixing.

- **5** — Raises failure modes the AI didn't mention. Asks about empty/null/concurrent/large/malicious inputs. Considers security and performance implications.
- **3** — Considers edge cases when reminded; doesn't lead with them.
- **1** — Happy-path-only thinking.

#### 6. `debuggingSkill` — weight **0.10**

When things break, does the developer reason from evidence — logs, errors, repro steps, isolation — or do they thrash with random fixes? Hypothesis-driven debugging.

- **5** — Reads errors carefully, forms a hypothesis, isolates the cause, fixes the root cause. Cites specific log lines or stack frames.
- **3** — Eventually reaches the fix but with detours or guesswork.
- **1** — Thrashes. Tries random changes. Asks the AI "why doesn't it work" without inspecting evidence.
- **N/A** — If the transcript contains no broken-thing-to-debug, score `3` (neutral) and call this out in the explanation.

#### 7. `promptSpecificity` — weight **0.10** (absorbs `curiosity`)

How precise are the developer's prompts? Do they supply context (files, constraints, examples, acceptance criteria) and ask sharp clarifying questions, or do they fire off vague "fix this" / "make it better"? Specific prompts → strong communicator → likely strong PR/spec writer.

- **5** — Prompts are precise, contextual, constraint-aware. Clarifying questions probe assumptions and surface hidden requirements.
- **3** — Prompts are adequate but uneven — some specific, some vague.
- **1** — Vague one-liners. No context. No clarifying questions where they were obviously needed.

#### 8. `scopeDiscipline` — weight **0.10**

Does the developer resist gold-plating, off-topic refactors, and AI sprawl? Stays on task. YAGNI awareness.

- **5** — Stays tightly on task. Explicitly reins in the AI when it tries to expand scope. No unrelated changes.
- **3** — Mostly on task with one minor scope drift.
- **1** — Lets the AI sprawl. Accepts unrelated "improvements" and refactors that weren't asked for.

### Final score formula

Weights sum to 1.00. Final score uses the existing weighted-mean + weakest-link penalty, unchanged:

```text
weighted = Σ (dim_i × weight_i)
penalty  = 0.4 × max(0, 4 − min(dims))
final    = clamp(weighted − penalty, 0, 5)
```

A candidate who scores `5,5,5,5,5,5,5,0` ends up around **2.9**, which is correct: one zero on a hiring axis is disqualifying signal regardless of strength elsewhere.

### Schema changes

New migration `src-tauri/migrations/004_rubric_v3_dimensions.sql`:

- `ALTER TABLE scores DROP COLUMN` for each of the six old dimension columns.
- `ALTER TABLE scores ADD COLUMN` for each of the eight new dimensions with `REAL NOT NULL CHECK (col >= 0 AND col <= 5) DEFAULT 0`.
- Followed by `DELETE FROM scores` — old rows are not meaningful under the new rubric and must be re-scored.
- `DELETE FROM daily_scores; DELETE FROM weekly_scores;` — aggregates are derived from `scores` and must be recomputed.

SQLite's `ALTER TABLE DROP COLUMN` requires SQLite ≥ 3.35 (March 2021). Tauri's bundled `rusqlite` ships modern SQLite, so this is fine; the migration must run as a single transaction.

### Versioning

- `RUBRIC_VERSION`: `"v2"` → `"v3"`
- `PROMPT_VERSION`: `"v2"` → `"v3"`

Both bumps are deliberate: they invalidate the score cache (`cache_key` is derived from both) so any conversations re-scored after the migration land under the new rubric.

### Files to change

**Backend (Rust):**
- `src-tauri/src/scoring/rubric.rs` — `RubricWeights`, `RubricDimensions`, `compute_final_score`, `RUBRIC_DESCRIPTION`, `RUBRIC_VERSION`, tests.
- `src-tauri/src/scoring/scorer.rs` — `ScorePayload` (rename fields), JSON schema sent to the model, dimension-range checks, construction of `RubricDimensions`.
- `src-tauri/src/scoring/prompt.rs` — `PROMPT_VERSION` bump only; prompt text unchanged in structure (it still injects `RUBRIC_DESCRIPTION`).
- `src-tauri/src/commands.rs` — any SELECT / INSERT referencing the dropped columns; rename to new ones.
- `src-tauri/src/importers/normalizer.rs` — verify no hardcoded references to old dimension names.
- `src-tauri/migrations/004_rubric_v3_dimensions.sql` — new file.

**Frontend (TypeScript / React):**
- `src/lib/types.ts` — `RubricDimension` union, `RubricDimensions`, `ScoreRecord`, `ConversationWithScore`, `WeakRubric` label mapping.
- `app/conversations/detail/ConversationDetail.tsx` — dimension labels + display.
- Any dashboard widget that renders per-dimension data — update labels to the new dimension names.

### Calibration mindset (for the LLM grader)

The `RUBRIC_DESCRIPTION` constant is rewritten to grade the **candidate**, not the AI. Key reframings:

- "You are a senior hiring manager doing a critical review of a candidate's vibe-coding session."
- "Default every dimension to 2. Move up only with specific, citable evidence from the candidate's own messages and decisions."
- "A 5 is exceptional — the kind of session you'd forward to the hiring committee."
- "If the candidate is mostly silent while the AI does the work, that is NOT a high score — it's a low one for almost every dimension."
- "Quote the candidate's own words in the explanation."

The "things that reduce every score" list is rewritten too: the candidate accepting hallucinated APIs without comment, never pushing back, never verifying, vague prompts, no clarifying questions, etc.

## Risks and open questions

1. **Existing scored data is lost.** `DELETE FROM scores` is destructive. Users with significant scoring history will need to re-score. *Mitigation:* the migration could rename the old table to `scores_v2_archive` instead of deleting. The spec proposes the simpler "delete" path because the data is incomparable across rubrics anyway and Vibe Score is a personal tool, not a multi-tenant production system. **Decision point for user before implementation.**

2. **Eight dimensions may still be too many for the LLM to score independently.** If `gpt-4o-mini` produces highly-correlated scores in practice (e.g. all 8 always within 1 point of each other), the rubric is functionally 1-dimensional and we should consolidate further. *Mitigation:* validate empirically after implementation by scoring a known-diverse set of transcripts and checking the per-dimension variance. This is an implementation-time check, not a spec-time one.

3. **Frontend dashboard real estate.** Eight dimensions is more than six; if the conversation detail or "weakest rubrics" widget renders a fixed-width column per dimension, the layout may need adjustment. *Mitigation:* implementation will read the current layout and adjust as needed; this is straightforward.

4. **Single-transcript hiring inference is noisy.** One conversation isn't a complete picture of a candidate. *Mitigation:* out of scope for this rubric change — a separate "candidate roll-up across N conversations" feature would address it.

## Open decision for user

**Migration approach for existing scored rows:** (1) delete them, or (2) archive into `scores_v2_archive`?
