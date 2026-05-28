use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    azure::ChatMessage,
    llm::{self, LlmConfig},
};

use super::{
    gap_analysis::{analyze_gaps, format_for_prompt},
    scanner::{ProjectRulesReport, RuleFile},
};

pub const PROJECT_RULES_RUBRIC_VERSION: &str = "v1.1";

/// Maximum bytes from each rule file we splice into the prompt. Keeps
/// prompts bounded even when the scanner picked up large markdown docs.
const PROMPT_FILE_BUDGET: usize = 4_000;
/// Hard cap on the sum of all per-file budgets inside the prompt.
const PROMPT_TOTAL_BUDGET: usize = 24_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRulesScore {
    pub project_path: String,
    pub content_hash: String,
    pub coverage: f64,
    pub stack_alignment: f64,
    pub specificity: f64,
    pub actionability: f64,
    /// Weighted final score (0-5).
    pub overall_score: f64,
    pub summary: String,
    /// Concrete, actionable suggestions for improving the rules.
    pub suggestions: Vec<String>,
    pub model_id: String,
    pub rubric_version: String,
    pub scored_at: String,
}

#[derive(Deserialize)]
struct RawScore {
    coverage: i64,
    #[serde(rename = "stackAlignment")]
    stack_alignment: i64,
    specificity: i64,
    actionability: i64,
    summary: String,
    suggestions: Vec<String>,
}

/// Send the project's rule files + detected tech stack to the LLM and
/// return a normalized `ProjectRulesScore`. Returns `Err` with a stable
/// message if there are no rule files (nothing to grade).
pub async fn score_project_rules_with_llm(
    report: &ProjectRulesReport,
    config: &LlmConfig,
    model: &str,
) -> Result<ProjectRulesScore, String> {
    if report.rule_files.is_empty() {
        return Err(
            "No AI-instruction files found in this project — add an AGENTS.md, CLAUDE.md, or \
             .cursor/rules/* file first."
                .to_string(),
        );
    }

    let prompt = build_prompt(report);

    let json_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "coverage":        { "type": "integer", "minimum": 0, "maximum": 5 },
            "stackAlignment":  { "type": "integer", "minimum": 0, "maximum": 5 },
            "specificity":     { "type": "integer", "minimum": 0, "maximum": 5 },
            "actionability":   { "type": "integer", "minimum": 0, "maximum": 5 },
            "summary":         { "type": "string" },
            "suggestions":     {
                "type": "array",
                "items": { "type": "string" },
                "maxItems": 6
            }
        },
        "required": [
            "coverage", "stackAlignment", "specificity",
            "actionability", "summary", "suggestions"
        ],
        "additionalProperties": false
    });

    let response_format = serde_json::json!({
        "type": "json_schema",
        "json_schema": {
            "name": "project_rules_score",
            "strict": true,
            "schema": json_schema
        }
    });

    let content = llm::chat_completion(
        config,
        vec![ChatMessage {
            role: "user",
            content: prompt,
        }],
        Some(response_format),
        Some(0.0),
    )
    .await?;

    let raw: RawScore =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse score JSON: {e}"))?;

    for (name, val) in [
        ("coverage", raw.coverage),
        ("stackAlignment", raw.stack_alignment),
        ("specificity", raw.specificity),
        ("actionability", raw.actionability),
    ] {
        if !(0..=5).contains(&val) {
            return Err(format!("Dimension '{name}' value {val} is out of range 0-5"));
        }
    }

    let overall = compute_overall(
        raw.coverage as f64,
        raw.stack_alignment as f64,
        raw.specificity as f64,
        raw.actionability as f64,
    );

    Ok(ProjectRulesScore {
        project_path: report.project_path.clone(),
        content_hash: report.content_hash.clone(),
        coverage: raw.coverage as f64,
        stack_alignment: raw.stack_alignment as f64,
        specificity: raw.specificity as f64,
        actionability: raw.actionability as f64,
        overall_score: overall,
        summary: raw.summary,
        suggestions: raw.suggestions,
        model_id: model.to_string(),
        rubric_version: PROJECT_RULES_RUBRIC_VERSION.to_string(),
        scored_at: Utc::now().to_rfc3339(),
    })
}

/// Same weighted-mean + weakest-link penalty pattern as the transcript
/// rubric, so users get a consistent 0–5 number across tabs.
pub fn compute_overall(coverage: f64, stack: f64, specificity: f64, actionability: f64) -> f64 {
    // Weights chosen so stack alignment + actionability dominate, because a
    // rule file that doesn't say anything actionable about the actual tech
    // stack is essentially worthless even if it "looks complete".
    let weighted =
        coverage * 0.20 + stack * 0.30 + specificity * 0.20 + actionability * 0.30;

    let min_dim = [coverage, stack, specificity, actionability]
        .into_iter()
        .fold(f64::INFINITY, f64::min);
    let penalty = 0.4 * (4.0_f64 - min_dim).max(0.0);
    (weighted - penalty).clamp(0.0, 5.0)
}

fn build_prompt(report: &ProjectRulesReport) -> String {
    let gap_check = format_for_prompt(&analyze_gaps(report));
    let mut bytes_budget = PROMPT_TOTAL_BUDGET;

    let file_blocks: Vec<String> = report
        .rule_files
        .iter()
        .map(|file| render_rule_block(file, &mut bytes_budget))
        .collect();

    let file_inventory: Vec<String> = report
        .rule_files
        .iter()
        .map(|f| format!("- {} ({})", f.relative_path, f.kind.label()))
        .collect();

    let stack = serde_json::to_string_pretty(&report.tech_stack)
        .unwrap_or_else(|_| "{}".to_string());

    format!(
        "You are a STRICT senior engineer grading the AI-instruction files (Cursor rules, \
         AGENTS.md, CLAUDE.md, etc.) of a project against its ACTUAL detected tech stack. \
         Grade what is there, not what could be there. Default every dimension to 2 and \
         only move up when the files contain SPECIFIC, EVIDENCED guidance for the detected \
         stack. A 5 is reserved for exceptional rule files that would meaningfully change \
         how an AI assistant behaves in this codebase.\n\n\
         ## Rubric (score each 0-5)\n\
         {RUBRIC}\n\n\
         ## Detected tech stack\n```json\n{stack}\n```\n\n\
         ## Automated pre-check (deterministic — use to calibrate scores)\n{gap_check}\n\n\
         ## Files present\n{files}\n\n\
         ## Rule file contents\n{contents}\n\n\
         ## Output requirements\n\
         - Return JSON only.\n\
         - In `summary` (<= 600 chars), cite SPECIFIC strengths and weaknesses tied to the \
           detected stack. If the rules barely mention the stack, say so.\n\
         - In `suggestions`, return 2-5 concrete, copy-pasteable additions or fixes. Each \
           suggestion must reference a specific framework/tool from the detected stack OR \
           call out a missing instruction file by name (e.g. \"Add a CLAUDE.md that \\\
           imports AGENTS.md\").\n\
         - If no rule files relate to the detected stack, coverage and stackAlignment must \
           BOTH be <= 2.\n\
         - If suggestions would be empty, return an array with one entry explaining why.\n",
        RUBRIC = RUBRIC_DESCRIPTION,
        stack = stack,
        gap_check = gap_check,
        files = file_inventory.join("\n"),
        contents = file_blocks.join("\n\n"),
    )
}

fn render_rule_block(file: &RuleFile, budget: &mut usize) -> String {
    let remaining = (*budget).min(PROMPT_FILE_BUDGET);
    if remaining == 0 {
        return format!("### {} (omitted: budget exhausted)\n", file.relative_path);
    }
    let mut snippet = file.content.clone();
    let truncated_for_prompt = snippet.len() > remaining;
    if truncated_for_prompt {
        let mut cut = remaining;
        while cut > 0 && !snippet.is_char_boundary(cut) {
            cut -= 1;
        }
        snippet.truncate(cut);
    }
    *budget = budget.saturating_sub(snippet.len());

    let trunc_marker = if file.truncated || truncated_for_prompt {
        "\n[…file truncated for prompt budget…]"
    } else {
        ""
    };

    format!(
        "### {} ({})\n```\n{}{}\n```",
        file.relative_path,
        file.kind.label(),
        snippet,
        trunc_marker,
    )
}

const RUBRIC_DESCRIPTION: &str = r#"
coverage — Do the rule files cover the major surfaces an AI coding agent would
need (project conventions, testing approach, build/run commands, deployment,
common gotchas)?
  5 = Comprehensive: conventions, testing, run commands, deprecations,
      and gotchas are all addressed. RARE.
  4 = Most key surfaces covered with at most one gap.
  3 = Several important areas covered but obvious holes (e.g. no testing
      guidance).
  2 = Only one or two areas covered; mostly empty or boilerplate.
  1 = Almost nothing useful for an AI agent.
  0 = No rules of substance present.

stackAlignment — Do the rules speak specifically to the DETECTED stack
(by name, version, idioms, library APIs)?
  5 = Calls out the actual frameworks/tools by name with non-trivial, current
      guidance. Acknowledges version-specific behavior where it matters.
      RARE.
  4 = Mentions the major frameworks with at least one specific, correct rule
      per framework.
  3 = Mentions the stack but stays generic ("use TypeScript well").
  2 = Names the stack once or twice without actionable guidance.
  1 = Could be pasted into any project with no edits.
  0 = Contradicts or ignores the actual stack.

specificity — Are the rules concrete (file paths, command names, code
patterns) vs. vague ("write clean code")?
  5 = Every non-trivial rule cites a path, command, or pattern. No fluff. RARE.
  4 = Most rules are concrete; a few generic statements creep in.
  3 = Mix of concrete and vague rules.
  2 = Mostly platitudes with occasional concreteness.
  1 = Almost entirely vague.
  0 = Pure motivational text.

actionability — Could an AI agent FOLLOW these rules without further
clarification? Are do/don't decisions clearly framed?
  5 = Every rule is a crisp do/don't with a clear trigger condition. RARE.
  4 = Mostly clear directives; one or two ambiguities.
  3 = Clear in places, ambiguous in others (passive voice, "should
      probably").
  2 = Reads like prose, not rules; an agent would need to guess intent.
  1 = Confusing or contradictory directives.
  0 = Unfollowable.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overall_score_is_perfect_when_all_fives() {
        let s = compute_overall(5.0, 5.0, 5.0, 5.0);
        assert!((s - 5.0).abs() < 0.001);
    }

    #[test]
    fn weakest_link_penalty_applies() {
        // mostly 5s, one 2 → meaningful drop
        let s = compute_overall(5.0, 5.0, 5.0, 2.0);
        // weighted = 4.1, penalty = 0.4*2 = 0.8 → 3.3
        assert!(s < 4.0, "got {s}");
    }

    #[test]
    fn zero_floor_is_zero() {
        let s = compute_overall(0.0, 0.0, 0.0, 0.0);
        assert!((s - 0.0).abs() < 0.001);
    }

    #[test]
    fn weights_sum_to_one() {
        let sum: f64 = 0.20 + 0.30 + 0.20 + 0.30;
        assert!((sum - 1.0).abs() < 1e-9);
    }
}
