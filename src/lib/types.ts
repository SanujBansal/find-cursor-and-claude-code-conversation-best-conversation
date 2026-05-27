export type SourceType =
  | "cursor-local"
  | "claude-code-local"
  | "claude-web-markdown";

export type JobStatusValue = "pending" | "running" | "completed" | "failed";

/** @deprecated Use JobStatusValue for the status string type */
export type JobStatus = JobStatusValue;

export type JobType =
  | "import"
  | "score"
  | "aggregate";

export type RubricDimension =
  | "taskCompletion"
  | "technicalCorrectness"
  | "workflowQuality"
  | "toolUseAndContext"
  | "communicationClarity"
  | "learningLeverage";

export interface AppSettings {
  /** Azure OpenAI endpoint URL override (falls back to AZURE_OPENAI_ENDPOINT from `.env`) */
  azureEndpoint: string;
  /** Azure OpenAI API key override (falls back to AZURE_OPENAI_API_KEY from `.env`) */
  openaiApiKey: string;
  scoringModel: string;
  cursorDataPath: string;
  claudeCodePath: string;
  claudeMarkdownPath: string;
  azureConfigured?: boolean;
  azureEnvPath?: string;
}

export interface RubricAverage {
  dimension: RubricDimension;
  average: number;
}

// ── Phase 5: Analytics + Dashboard ───────────────────────────────────────────

export interface WeeklyTrendPoint {
  weekLabel: string;   // "2026-W21"
  score: number;
  activeDays: number;
}

export interface WeakRubric {
  dimension: string;   // "taskCompletion" etc.
  averageScore: number;
  label: string;       // "Task Completion" etc.
}

export interface TrendPoint {
  date: string;            // "2026-05-27"
  score: number;
  conversationCount: number;
}

export interface ConversationSummary {
  id: number;
  title: string;
  provider: string;
  projectName: string | null;
  sourcePath: string | null;
  finalScore: number | null;
  completedAt: string | null;
  userMessageCount: number;
  toolCallCount: number;
}

export interface DashboardData {
  todayScore: number | null;
  dailyDelta: number | null;
  weekScore: number | null;
  weeklyDelta: number | null;
  rolling7d: number | null;
  weeklyTrend: WeeklyTrendPoint[];
  topConversations: ConversationWithScore[];
  weakestRubrics: WeakRubric[];
  totalConversations: number;
  totalScored: number;
}

export interface SourceRecord {
  id: number;
  sourceType: SourceType;
  name: string;
  path: string;
  enabled: boolean;
}

export interface JobRecord {
  id: number;
  jobType: JobType;
  status: JobStatus;
  progress: number;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
}

export const DEFAULT_SETTINGS: AppSettings = {
  azureEndpoint: "",
  openaiApiKey: "",
  scoringModel: "gpt-4.1-mini",
  cursorDataPath: "",
  claudeCodePath: "",
  claudeMarkdownPath: "",
  azureConfigured: false,
  azureEnvPath: ".env",
};

export function hasAiCredentials(settings: AppSettings): boolean {
  return Boolean(
    settings.azureConfigured ||
      (settings.azureEndpoint.trim() && settings.openaiApiKey.trim()),
  );
}

export interface ImportResult {
  sourceType: string;
  imported: number;
  skipped: number;
  errors: string[];
}

export interface ProjectGroup {
  projectPath: string;
  projectName: string;
  conversationCount: number;
  scoredCount: number;
}

export interface ImportAllResult {
  cleared: boolean;
  cursor: ImportResult;
  claudeCode: ImportResult;
  claudeMarkdown: ImportResult;
  scored: number;
  scoringErrors: string[];
}

export interface ConversationExportMarkdown {
  markdown: string;
  suggestedFilename: string;
  provider: string;
}

export interface JobStatusRecord {
  id: number;
  jobType: string;
  status: JobStatusValue;
  progress: number | null;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
}

// ── Phase 4: Scoring Engine ───────────────────────────────────────────────────

export interface RubricDimensions {
  taskCompletion: number;
  technicalCorrectness: number;
  workflowQuality: number;
  toolUseAndContext: number;
  communicationClarity: number;
  learningLeverage: number;
}

export interface ScoringResult {
  conversationId: string;
  contentHash: string;
  dimensions: RubricDimensions;
  finalScore: number;
  explanation: string;
  modelId: string;
  rubricVersion: string;
  promptVersion: string;
  cacheKey: string;
  scoredAt: string;
}

export interface ScoreRecord {
  id: number;
  conversationId: number;
  taskCompletion: number;
  technicalCorrectness: number;
  workflowQuality: number;
  toolUseAndContext: number;
  communicationClarity: number;
  learningLeverage: number;
  finalScore: number;
  explanation: string | null;
  modelId: string;
  rubricVersion: string;
  promptVersion: string;
  contentHash: string;
  cacheKey: string;
  scoredAt: string;
  createdAt: string;
}

export interface ConversationWithScore {
  id: number;
  title: string;
  provider: string;
  projectName: string | null;
  sourcePath: string | null;
  finalScore: number | null;
  completedAt: string | null;
  messageCount: number;
  toolCallCount: number;
  taskCompletion: number | null;
  technicalCorrectness: number | null;
  workflowQuality: number | null;
  toolUseAndContext: number | null;
  communicationClarity: number | null;
  learningLeverage: number | null;
  explanation: string | null;
  modelId: string | null;
  scoredAt: string | null;
}

export interface MessageRecord {
  role: string;
  content: string;
  sequenceNum: number;
}

// ── Phase 6: Project Rules ───────────────────────────────────────────────────

export type RuleKind =
  | "agents"
  | "claude"
  | "gemini"
  | "cursor-legacy"
  | "cursor-rule"
  | "windsurf"
  | "copilot"
  | "aider"
  | "other";

export interface RuleFile {
  relativePath: string;
  absolutePath: string;
  kind: RuleKind;
  bytes: number;
  content: string;
  truncated: boolean;
}

export interface TechStack {
  languages: string[];
  frameworks: string[];
  tooling: string[];
  signalFiles: string[];
  detected: boolean;
}

export interface ProjectRulesReport {
  projectPath: string;
  projectName: string;
  exists: boolean;
  techStack: TechStack;
  ruleFiles: RuleFile[];
  totalBytes: number;
  contentHash: string;
}

export interface ProjectRulesScore {
  projectPath: string;
  contentHash: string;
  coverage: number;
  stackAlignment: number;
  specificity: number;
  actionability: number;
  overallScore: number;
  summary: string;
  suggestions: string[];
  modelId: string;
  rubricVersion: string;
  scoredAt: string;
}

export interface ProjectRulesView {
  report: ProjectRulesReport;
  score: ProjectRulesScore | null;
  stale: boolean;
}

export const RULE_KIND_LABELS: Record<RuleKind, string> = {
  agents: "AGENTS.md",
  claude: "Claude rules",
  gemini: "Gemini rules",
  "cursor-legacy": ".cursorrules",
  "cursor-rule": "Cursor rule",
  windsurf: "Windsurf rules",
  copilot: "Copilot instructions",
  aider: "Aider config",
  other: "Other instructions",
};
