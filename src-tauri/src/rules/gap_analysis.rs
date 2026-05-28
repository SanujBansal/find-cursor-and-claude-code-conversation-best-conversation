use super::{
    scanner::{ProjectRulesReport, RuleFile},
    tech_stack::TechStack,
};

/// Deterministic pre-checks run before the LLM grades rules. Injected into
/// the prompt so identical rule content always produces the same checklist,
/// and fixing a listed gap gives the model explicit evidence to raise scores.
#[derive(Debug, Clone)]
pub struct GapAnalysis {
    pub stack_mentions: Vec<StackMention>,
    pub contradictions: Vec<String>,
    pub coverage_gaps: Vec<CoverageGap>,
    pub specificity_signals: SpecificitySignals,
}

#[derive(Debug, Clone)]
pub struct StackMention {
    pub item: String,
    pub mentioned: bool,
    /// Short hint for the LLM when the item is missing from rules.
    pub hint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageGap {
    Testing,
    CiCd,
    RunCommands,
    Deployment,
    Gotchas,
}

impl CoverageGap {
    fn label(self) -> &'static str {
        match self {
            CoverageGap::Testing => "testing approach",
            CoverageGap::CiCd => "CI/CD pipeline",
            CoverageGap::RunCommands => "build/run commands",
            CoverageGap::Deployment => "deployment guidance",
            CoverageGap::Gotchas => "common gotchas / deprecations",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpecificitySignals {
    pub file_path_refs: usize,
    pub shell_commands: usize,
    pub do_dont_rules: usize,
}

pub fn analyze_gaps(report: &ProjectRulesReport) -> GapAnalysis {
    let corpus = build_corpus(&report.rule_files);
    let lower = corpus.to_lowercase();

    let stack_mentions = analyze_stack_mentions(&report.tech_stack, &lower);
    let contradictions = detect_contradictions(&report.tech_stack, &lower);
    let coverage_gaps = detect_coverage_gaps(&lower, &report.tech_stack);
    let specificity_signals = count_specificity_signals(&corpus, &lower);

    GapAnalysis {
        stack_mentions,
        contradictions,
        coverage_gaps,
        specificity_signals,
    }
}

pub fn format_for_prompt(analysis: &GapAnalysis) -> String {
    let mut out = String::from(
        "Use this deterministic pre-check to calibrate scores. Do NOT ignore it.\n\
         - If a stack item is listed as MISSING but the rule files clearly cover it, \
           treat it as MENTIONED.\n\
         - If a coverage gap is listed as MISSING but the rule files address it with \
           concrete guidance, remove that gap from consideration.\n\
         - When a gap listed below is clearly still missing, cap the relevant dimension \
           at 4 (coverage for coverage gaps, stackAlignment for stack gaps).\n\
         - When ALL gaps below are addressed with specific, evidenced guidance, you \
           MAY score 5 on the relevant dimensions.\n\n",
    );

    out.push_str("### Stack mention checklist\n");
    for m in &analysis.stack_mentions {
        let status = if m.mentioned {
            "MENTIONED"
        } else {
            "MISSING"
        };
        out.push_str(&format!("- [{status}] {} — {}\n", m.item, m.hint));
    }

    if !analysis.contradictions.is_empty() {
        out.push_str("\n### Stack contradictions (lower stackAlignment until fixed)\n");
        for c in &analysis.contradictions {
            out.push_str(&format!("- {c}\n"));
        }
    }

    out.push_str("\n### Coverage gaps (lower coverage until fixed)\n");
    if analysis.coverage_gaps.is_empty() {
        out.push_str("- None detected — all major surfaces appear addressed.\n");
    } else {
        for gap in &analysis.coverage_gaps {
            out.push_str(&format!(
                "- MISSING: {} — add a dedicated rule file section with concrete guidance.\n",
                gap.label()
            ));
        }
    }

    out.push_str("\n### Specificity signals (deterministic counts)\n");
    out.push_str(&format!(
        "- File/path references: {}\n",
        analysis.specificity_signals.file_path_refs
    ));
    out.push_str(&format!(
        "- Shell/yarn/npm/pnpm/cargo commands: {}\n",
        analysis.specificity_signals.shell_commands
    ));
    out.push_str(&format!(
        "- Do/don't style rules: {}\n",
        analysis.specificity_signals.do_dont_rules
    ));
    out.push_str(
        "- Specificity >= 4 needs path/command refs in most rules; 5 needs them in \
         essentially all non-trivial rules.\n",
    );

    out
}

fn build_corpus(files: &[RuleFile]) -> String {
    files
        .iter()
        .map(|f| format!("{}\n{}", f.relative_path, f.content))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn analyze_stack_mentions(stack: &TechStack, lower: &str) -> Vec<StackMention> {
    let mut items: Vec<(&str, Vec<String>, &str)> = Vec::new();

    for lang in &stack.languages {
        items.push((lang, keyword_aliases(lang), "Name the language and idioms."));
    }
    for fw in &stack.frameworks {
        items.push((
            fw,
            keyword_aliases(fw),
            "Name the framework with at least one concrete, correct rule.",
        ));
    }
    for tool in &stack.tooling {
        items.push((
            tool,
            keyword_aliases(tool),
            "Reference the tool by name with commands or config paths.",
        ));
    }

    items
        .into_iter()
        .map(|(item, aliases, hint)| StackMention {
            item: item.to_string(),
            mentioned: aliases.iter().any(|kw| lower.contains(kw.as_str())),
            hint: hint.to_string(),
        })
        .collect()
}

fn keyword_aliases(item: &str) -> Vec<String> {
    let aliases: Vec<&str> = match item {
        "TypeScript" => vec!["typescript", "tsconfig", ".ts", ".tsx"],
        "JavaScript" => vec!["javascript", ".js", ".jsx", "node"],
        "Next.js" => vec!["next.js", "nextjs", "app router", "src/app"],
        "React" => vec!["react", "usestate", "useeffect", "jsx"],
        "Tailwind CSS" => vec!["tailwind", "tailwindcss", "@tailwindcss"],
        "Ant Design" => vec!["ant design", "antd", "@/lib/antd"],
        "Tauri" => vec!["tauri", "src-tauri"],
        "Prisma" => vec!["prisma", "@prisma/client"],
        "jest" => vec!["jest", "jest.config", "jest.setup", "__tests__"],
        "vitest" => vec!["vitest", "vitest.config"],
        "eslint" => vec!["eslint", ".eslintrc", "eslint.config"],
        "yarn" => vec!["yarn ", "yarn4", "yarn.lock", "packageManager"],
        "pnpm" => vec!["pnpm ", "pnpm-lock"],
        "npm" => vec!["npm run", "npm install", "package-lock"],
        "GitHub Actions" => vec![
            "github actions",
            ".github/workflows",
            "workflow.yml",
            "ci/cd",
            "ci cd",
        ],
        _ => return vec![item.to_lowercase()],
    };
    aliases.into_iter().map(str::to_string).collect()
}

/// Frameworks referenced in rules but absent from the detected stack, or
/// common mismatches (e.g. Hero UI mentioned as primary when stack uses antd).
fn detect_contradictions(stack: &TechStack, lower: &str) -> Vec<String> {
    let mut out = Vec::new();

    let stack_has_antd = stack.frameworks.iter().any(|f| f == "Ant Design");
    let rules_primary_hero = lower.contains("hero ui") || lower.contains("@heroui");
    let stack_has_hero = stack
        .frameworks
        .iter()
        .any(|f| f.contains("Hero UI") || f.contains("HeroUI"));

    if rules_primary_hero && !stack_has_hero && stack_has_antd {
        out.push(
            "Rules treat Hero UI as the UI library but detected stack uses Ant Design — \
             align rules with antd + Tailwind or document when each applies."
                .to_string(),
        );
    }

    let ui_libs_in_rules = [
        ("Ant Design", vec!["ant design", "antd"]),
        ("Hero UI", vec!["hero ui", "@heroui", "heroui"]),
        ("shadcn/ui", vec!["shadcn", "shadcn/ui"]),
        ("MUI", vec!["material-ui", " @mui/", "mui "]),
    ];

    let detected_frameworks_lower: Vec<String> = stack
        .frameworks
        .iter()
        .map(|f| f.to_lowercase())
        .collect();

    for (name, keywords) in ui_libs_in_rules {
        let mentioned = keywords.iter().any(|kw| lower.contains(kw));
        let in_stack = detected_frameworks_lower
            .iter()
            .any(|f| f.contains(&name.to_lowercase()) || name.to_lowercase().contains(f.as_str()));
        if mentioned && !in_stack && name != "Ant Design" {
            // antd handled separately via stack detection
            if name == "Ant Design" && !stack_has_antd {
                out.push(format!(
                    "Rules reference {name} but it was not detected in package.json — \
                     verify dependencies or remove stale guidance."
                ));
            } else if name != "Ant Design" {
                out.push(format!(
                    "Rules reference {name} but it was not detected in the project stack."
                ));
            }
        }
    }

    out
}

fn detect_coverage_gaps(lower: &str, stack: &TechStack) -> Vec<CoverageGap> {
    let mut gaps = Vec::new();

    let has_test_tool = stack.tooling.iter().any(|t| t == "jest" || t == "vitest");
    let testing_covered = has_test_tool
        && (lower.contains("jest")
            || lower.contains("vitest")
            || lower.contains("__tests__")
            || lower.contains(".test.")
            || lower.contains(".spec."))
        && (lower.contains("mock") || lower.contains("testing library") || lower.contains("rtl"));
    if has_test_tool && !testing_covered {
        gaps.push(CoverageGap::Testing);
    } else if !has_test_tool
        && !lower.contains("test")
        && !lower.contains("spec")
        && !lower.contains("tdd")
    {
        gaps.push(CoverageGap::Testing);
    }

    let has_ci = stack
        .tooling
        .iter()
        .any(|t| t == "GitHub Actions" || t == "GitLab CI");
    let ci_covered = lower.contains("github actions")
        || lower.contains(".github/workflows")
        || lower.contains("ci/cd")
        || lower.contains("pipeline");
    if has_ci && !ci_covered {
        gaps.push(CoverageGap::CiCd);
    }

    let run_covered = lower.contains("yarn ")
        || lower.contains("npm run")
        || lower.contains("pnpm ")
        || lower.contains("cargo run")
        || lower.contains("make ")
        || lower.contains("uv run");
    if !run_covered {
        gaps.push(CoverageGap::RunCommands);
    }

    let deploy_covered = lower.contains("deploy")
        || lower.contains("vercel")
        || lower.contains("production")
        || lower.contains("docker")
        || lower.contains("release");
    if !deploy_covered {
        gaps.push(CoverageGap::Deployment);
    }

    let gotchas_covered = lower.contains("gotcha")
        || lower.contains("deprecat")
        || lower.contains("do not")
        || lower.contains("don't")
        || lower.contains("never ")
        || lower.contains("avoid ");
    if !gotchas_covered {
        gaps.push(CoverageGap::Gotchas);
    }

    gaps
}

fn count_specificity_signals(corpus: &str, lower: &str) -> SpecificitySignals {
    let file_path_refs = count_matches(corpus, &["src/", "app/", ".cursor/", "jest.config"]);
    let shell_commands = count_matches(
        lower,
        &[
            "yarn ",
            "npm run",
            "pnpm ",
            "cargo ",
            "npx ",
            "docker ",
            "make ",
        ],
    );
    let do_dont_rules = count_matches(
        lower,
        &["do not", "don't", "never ", "always ", "avoid ", "must not", "must "],
    );

    SpecificitySignals {
        file_path_refs,
        shell_commands,
        do_dont_rules,
    }
}

fn count_matches(haystack: &str, needles: &[&str]) -> usize {
    needles
        .iter()
        .map(|n| haystack.matches(n).count())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{
        scanner::{ProjectRulesReport, RuleFile, RuleKind},
        tech_stack::TechStack,
    };

    fn sample_report(content: &str, stack: TechStack) -> ProjectRulesReport {
        ProjectRulesReport {
            project_path: "/tmp/test".to_string(),
            project_name: "test".to_string(),
            exists: true,
            tech_stack: stack,
            rule_files: vec![RuleFile {
                relative_path: ".cursor/rules/test.mdc".to_string(),
                absolute_path: "/tmp/test/.cursor/rules/test.mdc".to_string(),
                kind: RuleKind::CursorRule,
                content: content.to_string(),
                bytes: content.len() as u64,
                truncated: false,
            }],
            total_bytes: content.len(),
            content_hash: "test".to_string(),
        }
    }

    #[test]
    fn detects_missing_jest_and_ci_gaps() {
        let mut stack = TechStack::default();
        stack.detected = true;
        stack.tooling = vec!["jest".to_string(), "GitHub Actions".to_string()];
        stack.frameworks = vec!["Next.js".to_string()];

        let content = "Use Next.js App Router in src/app/. Run yarn dev.";
        let analysis = analyze_gaps(&sample_report(content, stack));

        assert!(analysis
            .coverage_gaps
            .contains(&CoverageGap::Testing));
        assert!(analysis.coverage_gaps.contains(&CoverageGap::CiCd));
    }

    #[test]
    fn clears_gaps_when_jest_and_ci_documented() {
        let mut stack = TechStack::default();
        stack.detected = true;
        stack.tooling = vec!["jest".to_string(), "GitHub Actions".to_string(), "yarn".to_string()];
        stack.frameworks = vec!["Next.js".to_string()];

        let content = r#"
            Run yarn test and yarn test:coverage. Jest config at jest.config.ts.
            Mock API with jest.mock. Use React Testing Library.
            CI/CD: .github/workflows/ci.yml runs lint and test on PRs.
            Deploy to Vercel on merge to main. Never commit .env files.
        "#;
        let analysis = analyze_gaps(&sample_report(content, stack));

        assert!(!analysis.coverage_gaps.contains(&CoverageGap::Testing));
        assert!(!analysis.coverage_gaps.contains(&CoverageGap::CiCd));
        assert!(!analysis.coverage_gaps.contains(&CoverageGap::RunCommands));
        assert!(!analysis.coverage_gaps.contains(&CoverageGap::Deployment));
        assert!(!analysis.coverage_gaps.contains(&CoverageGap::Gotchas));
    }

    #[test]
    fn format_is_stable_for_same_input() {
        let mut stack = TechStack::default();
        stack.detected = true;
        stack.frameworks = vec!["React".to_string()];
        let report = sample_report("Use React hooks.", stack);
        let a1 = format_for_prompt(&analyze_gaps(&report));
        let a2 = format_for_prompt(&analyze_gaps(&report));
        assert_eq!(a1, a2);
    }
}
