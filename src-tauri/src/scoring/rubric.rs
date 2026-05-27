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

/// Per-dimension hiring scores for a single transcript (0-5 scale).
/// Defined here so both `rubric` and `scorer` can reference the same type
/// without a circular import. Each dimension grades the human DEVELOPER's
/// behavior in the transcript — not the AI assistant's output.
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
/// great on most axes but mediocre on one should NOT round to a perfect 5
/// — a single weak hiring signal drags the headline number down.
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

pub const RUBRIC_DESCRIPTION: &str = r#"
You are a SENIOR STAFF ENGINEER doing a critical code-review of an
AI-assisted coding session. Your job is to grade the WORK, not to be kind.
Most real-world sessions are mediocre — your scores should reflect that.

## Calibration mindset (read this twice)

- DEFAULT every dimension to 2. Move UP only when the transcript shows
  specific, verifiable evidence. Move DOWN whenever you see a smell.
- A 5 is EXCEPTIONAL — the kind of work you would screenshot and share with
  the team. It is NOT "the task got done." Across 100 sessions, expect ~5
  fives, ~15 fours, ~40 threes, ~30 twos, ~10 ones, and rare zeros.
- The MOST COMMON honest score is 2 or 3. If you find yourself giving four
  or more 5s in one transcript, you are being too generous — re-read with a
  skeptic's eye and lower at least two of them.
- Never give a 5 to compensate for low scores elsewhere. Never round up.
- An "average" or "fine" session is a 2, not a 4.

## Things that REDUCE every score by at least 1

- Repeated failed attempts at the same problem
- The user has to correct the assistant's understanding or direction
- The assistant fabricates APIs, file paths, function signatures, or facts
- The fix is partial, hacky, or leaves obvious TODOs / dead code
- No verification step (no tests run, no lints checked, no output inspected)
- Excessive verbosity, repeated apologies, or filler text
- The assistant ignores or contradicts existing project conventions
- The conversation ends ambiguously (no clear "done", error left dangling)

## Dimensions (score each 0-5 INDEPENDENTLY)

taskCompletion — Did the loop actually reach the user's intended outcome?
  5 = Goal fully achieved AND verified end-to-end in the transcript
      (tests passing, output checked, user explicitly confirms). RARE.
  4 = Goal achieved but verification is light or one minor sub-goal slipped.
  3 = Substantive progress; core ask done but with caveats, TODOs, or
      unresolved follow-ups.
  2 = Partial progress; the user would still need significant work to finish.
  1 = Barely started, abandoned, or fundamentally off-track.
  0 = No useful progress, or actively made things worse.

technicalCorrectness — Are the code, commands, and reasoning actually right?
  5 = Zero meaningful errors. Code is idiomatic, edge cases considered,
      no fabricated APIs. A senior reviewer would approve as-is. RARE.
  4 = Solid, with at most one trivial nit you'd flag in review.
  3 = Mostly correct but with real issues (missing edge case, wrong-ish
      typing, suboptimal but functional approach).
  2 = Multiple correctness problems; would fail review or break in
      common cases.
  1 = Significant errors that would break the feature or mislead the user.
  0 = Fundamentally wrong, dangerous, or hallucinated.

workflowQuality — Was the engineering process disciplined?
  5 = Read relevant code first, planned, made tight scoped changes,
      verified with tests/lints/runtime checks, clean diff. RARE.
  4 = Mostly disciplined with one shortcut (e.g. skipped verification
      on a low-risk change).
  3 = Reasonable approach but with shortcuts: skipped exploration,
      no verification, or noticeable churn / rework.
  2 = Chaotic: edits before understanding, repeated re-tries, no
      verification, or scope creep.
  1 = Reckless: destructive commands without checks, ignored failures,
      large unrelated changes.
  0 = Actively destructive or completely undisciplined.

toolUseAndContext — Did the agent use tools and the project's context well?
  5 = Excellent: read the right files, used search/grep precisely, ran
      tests/linters, respected existing patterns, made minimal-context
      assumptions. RARE.
  4 = Strong tool use with one or two missed opportunities.
  3 = Adequate — used tools but missed obvious ones (didn't search before
      guessing, didn't read the file it was editing, ignored linter).
  2 = Poor — ignored relevant context, made changes blind, picked wrong
      tools, or duplicated existing functionality.
  1 = Almost no use of available tools where they were clearly needed.
  0 = No meaningful tool use at all despite obvious need.

communicationClarity — Was the session understandable and well-summarized?
  5 = Crisp, concise, well-structured. Progress updates are informative
      and free of filler. Every claim is grounded. RARE.
  4 = Clear with minor verbosity or one unclear passage.
  3 = Understandable but verbose, repetitive, or vague in places.
  2 = Hard to follow: walls of text, missing context, confusing pivots.
  1 = Confusing — critical details missing or wrong; reader would be lost.
  0 = Incomprehensible.

learningLeverage — Does the session surface reusable lessons or patterns?
  5 = Surfaces a non-obvious pattern, debugging technique, or
      architectural insight that would help future work. RARE.
  4 = Concrete transferable knowledge (a gotcha, a tested approach).
  3 = Some takeaways but mostly mechanical.
  2 = Little reusable signal; this session is one-off.
  1 = Nothing transferable.
  0 = Anti-pattern — would mislead anyone who learned from it.

## Output requirements

Score each transcript INDEPENDENTLY. Do NOT grade on a curve against
other transcripts.

Your `explanation` field MUST:
  1. Cite SPECIFIC evidence from the transcript (quote phrases, name files
     or errors) for each dimension that scored 4 or 5.
  2. Name at least one concrete weakness, even for high-scoring sessions.
     If you cannot find a weakness, the score is too high — lower it.

If you are unsure between two scores on a dimension, pick the LOWER one.
"#;
