# Scoring — Rubric & Implementation

Vibe Score evaluates **human developers** by analysing their vibe-coding transcripts with an LLM. This module contains the rubric definition, prompt builder, and scoring engine.

---

## Philosophy

The rubric does **not** grade the AI assistant's output. It grades the **candidate's** behaviour: how they steer the AI, how carefully they read its output, and how they reason about problems. A copy-paster who got working code and an engineer who shaped every step of the solution should score very differently.

---

## Dimensions (v3)

Each dimension is scored **0–5 independently**. Every dimension defaults to **2**; scores move up only with specific, citable evidence from the candidate's own messages.

| # | Key | Weight | What it measures |
|---|---|---|---|
| 1 | `conceptualKnowledge` | **0.18** | Does the candidate reason about *why* a solution works? Trade-off awareness, architectural understanding, correcting the AI on conceptual grounds. |
| 2 | `attentionToDetail` | **0.15** | Do they catch AI hallucinations and read diffs critically before accepting? Includes line-by-line code-review instinct. |
| 3 | `problemDecomposition` | **0.13** | Do they break work into well-sequenced steps and drive it to *verified* completion? Ownership is baked in — decomposition that doesn't close the loop doesn't count. |
| 4 | `criticalEvaluation` | **0.12** | Do they push back on the AI when it's wrong? Reject bad suggestions with specific reasoning, not just vague discomfort. |
| 5 | `robustnessAwareness` | **0.12** | Do they proactively raise failure modes, edge cases, security/perf concerns — *before* being prompted? |
| 6 | `debuggingSkill` | **0.10** | Evidence-driven debugging (form a hypothesis, isolate the cause) vs. thrashing and random changes. Score 3 (neutral) when the transcript has nothing to debug. |
| 7 | `promptSpecificity` | **0.10** | Are their prompts precise and contextual? Do they ask sharp clarifying questions that surface hidden requirements? Absorbs curiosity signal. |
| 8 | `scopeDiscipline` | **0.10** | Do they resist gold-plating and AI-driven scope sprawl? Stays on task; explicitly reins in unrelated "improvements". |

Weights sum to **1.00**.

---

## Final Score Formula

```
weighted_avg = Σ (dim_i × weight_i)
penalty      = 0.4 × max(0, 4 − min(dims))
final        = clamp(weighted_avg − penalty, 0, 5)
```

The **weakest-link penalty** ensures one poor dimension drags the headline score down — a hireable-looking aggregate can't hide a disqualifying signal.

| Example | Weighted avg | Penalty | Final |
|---|---|---|---|
| All 5s | 5.00 | 0.00 | **5.00** |
| All 4s | 4.00 | 0.00 | **4.00** |
| All 5s, one 2 | ~4.55 | 0.80 | **~3.75** |
| All 5s, one 0 | ~4.50 | 1.60 | **~2.90** |
| All 2s | 2.00 | 0.80 | **~1.20** |
| All 0s | 0.00 | 1.60 | **0.00** (clamped) |

---

## Calibration Guide

| Score | Meaning | Expected frequency |
|---|---|---|
| 5 | Exceptional — forward to the hiring committee | ~5 % |
| 4 | Solid with one notable gap | ~15 % |
| 3 | Adequate; mostly mechanical | ~40 % |
| 2 | Surface-level; significant gaps | ~30 % |
| 1 | Barely any signal | ~10 % |
| 0 | Anti-pattern or actively harmful | Rare |

A candidate who is mostly **silent** while the AI does the work scores **low** on nearly every dimension. Vibe coding is not a passive activity.

---

## Score Reduction Triggers

Any of the following pull **every** dimension down by at least 1:

- Accepts hallucinated APIs, wrong file paths, or fabricated signatures without comment
- Never pushes back on the AI, even when obviously wrong
- Prompts are vague one-liners with no context or constraints
- Doesn't read AI diffs — accepts patches wholesale
- No verification of output (no tests, no behaviour checks, no edge-case probing)
- Lets scope sprawl into unrelated "improvements" without pushback
- Thrashes when debugging instead of reasoning from evidence
- Session ends ambiguously (dangling error, no clear "done")

---

## Versioning

| Constant | Value | Location |
|---|---|---|
| `RUBRIC_VERSION` | `v3` | `rubric.rs` |
| `PROMPT_VERSION` | `v3` | `prompt.rs` |

Both constants are included in the score cache key (`sha256(content_hash ‖ rubric_version ‖ prompt_version ‖ model_id)`). Bumping either constant invalidates all cached scores and forces re-scoring, which is intentional when the rubric changes.

The migration that introduced v3 is `005_rubric_v3_dimensions.sql`.

---

## File Map

```
src/scoring/
├── rubric.rs    — RubricWeights, RubricDimensions, DEFAULT_WEIGHTS, compute_final_score, RUBRIC_DESCRIPTION
├── scorer.rs    — LLM call, JSON schema, ScorePayload, validation, score_batch / score_one
├── prompt.rs    — build_prompt, build_cache_key, PROMPT_VERSION, message truncation
├── mod.rs       — module re-exports
└── README.md    — this file
```

---

## History

| Version | What changed |
|---|---|
| v1 | Initial 6-dimension rubric grading the AI assistant's output |
| v2 | Tightened calibration language; added weakest-link penalty; no dimension changes |
| v3 | **Full pivot**: replaced all 6 AI-output dimensions with 8 human-developer hiring dimensions (May 2026) |
