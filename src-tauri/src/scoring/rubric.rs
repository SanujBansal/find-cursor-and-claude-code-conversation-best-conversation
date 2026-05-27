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
