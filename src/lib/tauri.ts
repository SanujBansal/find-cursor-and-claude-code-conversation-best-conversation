import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ChatSearchResponse,
  ConversationExportMarkdown,
  ConversationSummary,
  ConversationWithScore,
  DashboardData,
  EmbedResult,
  ImportAllResult,
  ImportResult,
  JobStatusRecord,
  LearningSuggestion,
  MessageRecord,
  ProjectGroup,
  ScoreRecord,
  SearchResult,
  ScoringResult,
  TrendPoint,
} from "./types";

export async function getDashboard(): Promise<DashboardData> {
  return invoke<DashboardData>("get_dashboard");
}

export async function listConversations(): Promise<ConversationSummary[]> {
  return invoke<ConversationSummary[]>("list_conversations");
}

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  return invoke("save_settings", { settings });
}

export async function importCursor(dataPath?: string): Promise<ImportResult> {
  return invoke<ImportResult>("import_cursor", { dataPath: dataPath ?? null });
}

export async function importClaudeCode(transcriptsPath?: string): Promise<ImportResult> {
  return invoke<ImportResult>("import_claude_code", {
    transcriptsPath: transcriptsPath ?? null,
  });
}

export async function importClaudeMarkdown(folderPath: string): Promise<ImportResult> {
  return invoke<ImportResult>("import_claude_markdown", { folderPath });
}

export async function clearAllTranscripts(): Promise<void> {
  return invoke("clear_all_transcripts");
}

export async function listProjects(): Promise<ProjectGroup[]> {
  return invoke<ProjectGroup[]>("list_projects");
}

export async function importAll(options: {
  cursorDataPath?: string;
  claudeCodePath?: string;
  claudeMarkdownPath?: string;
  clearExisting?: boolean;
}): Promise<ImportAllResult> {
  return invoke<ImportAllResult>("import_all", {
    cursorDataPath: options.cursorDataPath ?? null,
    claudeCodePath: options.claudeCodePath ?? null,
    claudeMarkdownPath: options.claudeMarkdownPath ?? null,
    clearExisting: options.clearExisting ?? true,
  });
}

export async function importAllAndScore(options: {
  apiKey: string;
  cursorDataPath?: string;
  claudeCodePath?: string;
  claudeMarkdownPath?: string;
  scoringModel?: string;
  clearExisting?: boolean;
}): Promise<ImportAllResult> {
  return invoke<ImportAllResult>("import_all_and_score", {
    apiKey: options.apiKey,
    cursorDataPath: options.cursorDataPath ?? null,
    claudeCodePath: options.claudeCodePath ?? null,
    claudeMarkdownPath: options.claudeMarkdownPath ?? null,
    scoringModel: options.scoringModel ?? null,
    clearExisting: options.clearExisting ?? true,
  });
}

export async function getImportStatus(): Promise<JobStatusRecord[]> {
  return invoke<JobStatusRecord[]>("get_import_status");
}

export async function getDefaultCursorPath(): Promise<string> {
  return invoke<string>("get_default_cursor_path");
}

export async function embedPending(
  apiKey: string,
  embeddingModel?: string,
): Promise<EmbedResult> {
  return invoke<EmbedResult>("embed_pending", {
    apiKey,
    embeddingModel: embeddingModel ?? null,
  });
}

export async function searchConversations(
  query: string,
  apiKey: string,
  limit?: number,
  embeddingModel?: string,
): Promise<SearchResult[]> {
  return invoke<SearchResult[]>("search_conversations", {
    query,
    apiKey,
    limit: limit ?? null,
    embeddingModel: embeddingModel ?? null,
  });
}

export async function chatSearch(
  query: string,
  apiKey: string,
  options?: { embeddingModel?: string; chatModel?: string },
): Promise<ChatSearchResponse> {
  return invoke<ChatSearchResponse>("chat_search", {
    query,
    apiKey,
    embeddingModel: options?.embeddingModel ?? null,
    chatModel: options?.chatModel ?? null,
  });
}

export async function scoreProject(
  apiKey: string,
  projectPath: string,
  modelId?: string,
  minUserMessages?: number | null,
): Promise<ScoringResult[]> {
  return invoke<ScoringResult[]>("score_project", {
    apiKey,
    projectPath,
    modelId: modelId ?? null,
    minUserMessages: minUserMessages ?? null,
  });
}

export async function getProjectTopConversations(
  projectPath: string,
  limit?: number,
): Promise<ConversationWithScore[]> {
  return invoke<ConversationWithScore[]>("get_project_top_conversations", {
    projectPath,
    limit: limit ?? null,
  });
}

export async function scorePending(
  apiKey: string,
  modelId?: string,
): Promise<ScoringResult[]> {
  return invoke<ScoringResult[]>("score_pending", {
    apiKey,
    modelId: modelId ?? null,
  });
}

export async function scoreConversation(
  apiKey: string,
  conversationId: number,
  modelId?: string,
): Promise<ScoringResult> {
  return invoke<ScoringResult>("score_conversation", {
    apiKey,
    conversationId,
    modelId: modelId ?? null,
  });
}

export async function getScores(conversationId?: number): Promise<ScoreRecord[]> {
  return invoke<ScoreRecord[]>("get_scores", {
    conversationId: conversationId ?? null,
  });
}

export async function getTopConversations(limit?: number): Promise<ConversationWithScore[]> {
  return invoke<ConversationWithScore[]>("get_top_conversations", {
    limit: limit ?? null,
  });
}

export async function getConversationExportMarkdown(
  conversationId: number,
): Promise<ConversationExportMarkdown> {
  return invoke<ConversationExportMarkdown>("get_conversation_export_markdown", {
    conversationId,
  });
}

/** Opens a save dialog and writes the conversation as Cursor-style Markdown. */
export async function exportConversationMarkdown(
  conversationId: number,
): Promise<string | null> {
  return invoke<string | null>("export_conversation_markdown", { conversationId });
}

export async function getConversationMessages(conversationId: number): Promise<MessageRecord[]> {
  return invoke<MessageRecord[]>("get_conversation_messages", { conversationId });
}

export async function generateSuggestions(
  apiKey: string,
  modelId?: string,
): Promise<LearningSuggestion[]> {
  return invoke<LearningSuggestion[]>("generate_suggestions", {
    apiKey,
    modelId: modelId ?? null,
  });
}

export async function getSuggestions(includeDismissed?: boolean): Promise<LearningSuggestion[]> {
  return invoke<LearningSuggestion[]>("get_suggestions", {
    includeDismissed: includeDismissed ?? null,
  });
}

export async function dismissSuggestion(id: string): Promise<void> {
  return invoke("dismiss_suggestion", { id });
}

export async function refreshAnalytics(): Promise<void> {
  return invoke("refresh_analytics");
}

export async function getTrendData(period: string): Promise<TrendPoint[]> {
  return invoke<TrendPoint[]>("get_trend_data", { period });
}

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
