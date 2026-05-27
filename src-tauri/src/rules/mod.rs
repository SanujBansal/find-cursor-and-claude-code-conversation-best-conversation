pub mod scanner;
pub mod scorer;
pub mod tech_stack;

pub use scanner::{scan_project_rules, ProjectRulesReport};
pub use scorer::{
    score_project_rules_with_llm, ProjectRulesScore, PROJECT_RULES_RUBRIC_VERSION,
};
