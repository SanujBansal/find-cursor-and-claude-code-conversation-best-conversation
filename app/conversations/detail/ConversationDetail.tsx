"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Shell } from "@/components/Shell";
import {
  analyzeChatVibe,
  exportConversationMarkdown,
  getConversationMessages,
  getScores,
  getSettings,
  scoreConversation,
} from "../../../src/lib/tauri";
import { useTauriRuntime } from "../../../src/lib/useTauriRuntime";
import type { MessageRecord, ScoreRecord, VibeImprovement } from "../../../src/lib/types";
import { hasAiCredentials, getActiveApiKey } from "../../../src/lib/types";

// ── constants ─────────────────────────────────────────────────────────────────

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

// ── small shared components ───────────────────────────────────────────────────

function ScoreBar({ label, value }: { label: string; value: number }) {
  const pct = Math.round((value / 5) * 100);
  const color =
    value >= 4 ? "bg-emerald-500" : value >= 2.5 ? "bg-amber-400" : "bg-red-400";
  return (
    <div className="flex items-center gap-3">
      <span className="w-40 shrink-0 text-xs text-muted">{label}</span>
      <div className="flex-1 h-1.5 rounded-full bg-muted-light overflow-hidden">
        <div className={`h-full rounded-full transition-all duration-500 ${color}`} style={{ width: `${pct}%` }} />
      </div>
      <span className="w-7 text-right text-xs font-semibold tabular-nums text-foreground">
        {value.toFixed(1)}
      </span>
    </div>
  );
}

// ── transcript helpers ────────────────────────────────────────────────────────

type TranscriptPart = { role: string; content: string };
type TranscriptBlock =
  | { type: "user"; content: string; key: string }
  | { type: "assistant"; parts: TranscriptPart[]; key: string };

function groupMessages(messages: MessageRecord[]): TranscriptBlock[] {
  const blocks: TranscriptBlock[] = [];
  for (const msg of messages) {
    if (msg.role === "user") {
      blocks.push({ type: "user", content: msg.content, key: `user-${msg.sequenceNum}` });
      continue;
    }
    const last = blocks[blocks.length - 1];
    if (last?.type === "assistant") {
      last.parts.push({ role: msg.role, content: msg.content });
      continue;
    }
    blocks.push({ type: "assistant", parts: [{ role: msg.role, content: msg.content }], key: `assistant-${msg.sequenceNum}` });
  }
  return blocks;
}

function previewText(content: string, maxLength = 120): string {
  const trimmed = content.trim().replace(/\s+/g, " ");
  if (trimmed.length <= maxLength) return trimmed;
  return `${trimmed.slice(0, maxLength).trimEnd()}…`;
}

function assistantSummary(parts: TranscriptPart[]): string {
  const assistantCount = parts.filter((p) => p.role === "assistant").length;
  const totalChars = parts.reduce((sum, p) => sum + p.content.length, 0);
  if (assistantCount > 1) return `${assistantCount} assistant messages · ${totalChars.toLocaleString()} chars`;
  if (parts.length > 1) return `${parts.length} messages · ${totalChars.toLocaleString()} chars`;
  return previewText(parts[0]?.content ?? "");
}

function UserMessageBlock({ content }: { content: string }) {
  return (
    <div className="rounded-xl border-2 border-accent/25 bg-accent-light/40 p-5 shadow-sm">
      <div className="mb-3 flex items-center gap-2">
        <span className="inline-flex h-6 w-6 items-center justify-center rounded-full bg-accent text-[10px] font-bold uppercase tracking-wide text-white">U</span>
        <span className="text-xs font-semibold uppercase tracking-widest text-accent">You</span>
      </div>
      <p className="text-[15px] font-medium leading-relaxed text-foreground whitespace-pre-wrap break-words">{content}</p>
    </div>
  );
}

function AssistantMessageBlock({ parts, expanded, onToggle }: { parts: TranscriptPart[]; expanded: boolean; onToggle: () => void }) {
  const summary = assistantSummary(parts);
  return (
    <div className="rounded-xl border border-panel-border/80 bg-muted-light/40">
      <button type="button" onClick={onToggle} className="flex w-full items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-muted-light/80" aria-expanded={expanded}>
        <span className={`mt-0.5 shrink-0 text-xs text-muted transition-transform ${expanded ? "rotate-90" : ""}`} aria-hidden>▶</span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs font-medium uppercase tracking-widest text-muted">Assistant</span>
            {!expanded && <span className="text-xs text-muted/80">{summary}</span>}
          </div>
          {!expanded && summary !== previewText(parts[0]?.content ?? "") && (
            <p className="mt-1 truncate text-sm text-muted">{previewText(parts[0]?.content ?? "", 80)}</p>
          )}
        </div>
      </button>
      {expanded && (
        <div className="space-y-3 border-t border-panel-border/60 px-4 py-3">
          {parts.map((part, idx) => (
            <div key={idx}>
              {parts.length > 1 && (
                <span className="mb-1 inline-flex rounded-full bg-background px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-muted ring-1 ring-panel-border">{part.role}</span>
              )}
              <p className="text-sm leading-relaxed text-foreground/75 whitespace-pre-wrap break-words">{part.content}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Improvement suggestions modal ─────────────────────────────────────────────

function ImprovementCard({ item }: { item: VibeImprovement }) {
  return (
    <div className="overflow-hidden rounded-xl border border-panel-border">
      <div className="flex items-start gap-3 border-b border-panel-border bg-[#f87171]/5 px-4 py-3">
        <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-[#f87171]/20 text-[11px] font-bold text-[#f87171]">
          {item.index}
        </span>
        <p className="text-xs text-muted leading-relaxed pt-0.5">{item.tip}</p>
      </div>
      <div className="grid sm:grid-cols-2 divide-y sm:divide-y-0 sm:divide-x divide-panel-border">
        <div className="p-4 flex flex-col gap-1.5">
          <p className="text-[10px] uppercase tracking-[0.15em] font-semibold text-[#f87171]">Original prompt</p>
          <p className="text-xs font-mono text-muted leading-relaxed whitespace-pre-wrap break-words">{item.badPrompt}</p>
        </div>
        <div className="p-4 flex flex-col gap-1.5">
          <p className="text-[10px] uppercase tracking-[0.15em] font-semibold text-emerald-500">Improved prompt</p>
          <p className="text-xs font-mono text-foreground leading-relaxed whitespace-pre-wrap break-words">{item.improvedPrompt}</p>
        </div>
      </div>
    </div>
  );
}

function SkeletonCard() {
  return (
    <div className="animate-pulse overflow-hidden rounded-xl border border-panel-border">
      <div className="h-10 border-b border-panel-border bg-panel-border/30" />
      <div className="grid sm:grid-cols-2 divide-x divide-panel-border">
        <div className="space-y-2 p-4">
          <div className="h-2 w-16 rounded bg-panel-border" />
          <div className="h-2 w-full rounded bg-panel-border" />
          <div className="h-2 w-4/5 rounded bg-panel-border" />
        </div>
        <div className="space-y-2 p-4">
          <div className="h-2 w-16 rounded bg-panel-border" />
          <div className="h-2 w-full rounded bg-panel-border" />
          <div className="h-2 w-4/5 rounded bg-panel-border" />
        </div>
      </div>
    </div>
  );
}

function ImprovementsModal({
  open,
  onClose,
  onAnalyse,
  analysing,
  analysed,
  improvements,
  error,
}: {
  open: boolean;
  onClose: () => void;
  onAnalyse: () => void;
  analysing: boolean;
  analysed: boolean;
  improvements: VibeImprovement[];
  error: string | null;
}) {
  const overlayRef = useRef<HTMLDivElement>(null);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      ref={overlayRef}
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm"
      onClick={(e) => { if (e.target === overlayRef.current) onClose(); }}
    >
      <div className="relative flex flex-col w-full max-w-3xl max-h-[88vh] rounded-2xl border border-panel-border bg-panel shadow-2xl overflow-hidden">
        {/* Modal header */}
        <div className="flex items-center justify-between border-b border-panel-border px-6 py-4 shrink-0">
          <div className="flex items-center gap-2.5">
            <svg className="h-4 w-4 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z" />
            </svg>
            <h2 className="text-sm font-semibold text-foreground">Improvement suggestions</h2>
            {!analysing && improvements.length > 0 && (
              <span className="rounded-full border border-accent/30 bg-accent/10 px-2 py-0.5 text-[11px] font-medium text-accent">
                {improvements.length}
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={onAnalyse}
              disabled={analysing}
              className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent/90 disabled:opacity-50 transition-colors"
            >
              {analysing && (
                <span className="inline-block h-3 w-3 rounded-full border-2 border-white/40 border-t-white animate-spin" />
              )}
              {analysing ? "Analysing…" : analysed ? "Re-analyse" : "Analyse"}
            </button>
            <button
              type="button"
              onClick={onClose}
              className="flex h-7 w-7 items-center justify-center rounded-lg border border-panel-border text-muted hover:text-foreground hover:border-accent/40 transition-colors text-lg leading-none"
              aria-label="Close"
            >
              ×
            </button>
          </div>
        </div>

        {/* Modal body */}
        <div className="flex-1 overflow-y-auto px-6 py-5 space-y-4">
          {error && (
            <div className="rounded-lg border border-[#f87171]/30 bg-[#f87171]/8 px-4 py-3 text-xs text-[#f87171]">
              {error}
            </div>
          )}

          {analysing && (
            <>
              <p className="text-xs text-muted">Reading your prompts and crafting improvements…</p>
              {[0, 1, 2, 3].map((i) => <SkeletonCard key={i} />)}
            </>
          )}

          {!analysing && !analysed && !error && (
            <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
              <p className="text-sm font-medium text-foreground">Ready to analyse</p>
              <p className="text-xs text-muted max-w-sm leading-relaxed">
                Click <strong>Analyse</strong> to get the 3–5 highest-impact prompt improvements from this conversation — no filler, only what actually matters.
              </p>
            </div>
          )}

          {!analysing && analysed && improvements.length === 0 && !error && (
            <div className="flex flex-col items-center justify-center gap-3 py-16 text-center">
              <p className="text-sm font-medium text-foreground">No issues found</p>
              <p className="text-xs text-muted max-w-sm leading-relaxed">
                Your prompts in this chat look solid — the coach found nothing to flag.
              </p>
            </div>
          )}

          {!analysing && improvements.length > 0 && (
            <>
              <p className="text-xs text-muted">
                <span className="font-semibold text-foreground">{improvements.length}</span> prompt improvement{improvements.length === 1 ? "" : "s"} found
              </p>
              {improvements.map((item) => (
                <ImprovementCard key={item.index} item={item} />
              ))}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

// ── main component ────────────────────────────────────────────────────────────

export function ConversationDetail() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const isRuntime = useTauriRuntime();
  const rawId = searchParams?.get("id");
  const conversationId = rawId ? parseInt(rawId, 10) : null;

  const [messages, setMessages] = useState<MessageRecord[]>([]);
  const [expandedBlocks, setExpandedBlocks] = useState<Set<string>>(new Set());
  const [score, setScore] = useState<ScoreRecord | null>(null);
  const [loading, setLoading] = useState(true);
  const [scoring, setScoring] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [exportNotice, setExportNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Improvement suggestions state
  const [modalOpen, setModalOpen] = useState(false);
  const [vibeImprovements, setVibeImprovements] = useState<VibeImprovement[]>([]);
  const [vibeAnalysing, setVibeAnalysing] = useState(false);
  const [vibeAnalysed, setVibeAnalysed] = useState(false);
  const [vibeError, setVibeError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (!conversationId || !isRuntime) {
        if (!cancelled) setLoading(false);
        return;
      }
      setLoading(true);
      try {
        const [msgs, scores] = await Promise.all([
          getConversationMessages(conversationId),
          getScores(conversationId),
        ]);
        if (cancelled) return;
        setMessages(msgs);
        setScore(scores[0] ?? null);
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [conversationId, isRuntime]);

  const transcriptBlocks = useMemo(() => groupMessages(messages), [messages]);

  function toggleBlock(key: string) {
    setExpandedBlocks((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });
  }

  async function handleExportMarkdown() {
    if (!conversationId) return;
    setExporting(true);
    setError(null);
    setExportNotice(null);
    try {
      const savedPath = await exportConversationMarkdown(conversationId);
      if (savedPath) setExportNotice(`Exported to ${savedPath}`);
    } catch (err) {
      setError(String(err));
    } finally {
      setExporting(false);
    }
  }

  async function handleScore() {
    if (!conversationId) return;
    setScoring(true);
    setError(null);
    try {
      const settings = await getSettings();
      if (!hasAiCredentials(settings)) {
        setError("AI is not configured — set OpenAI or Azure credentials in Settings");
        return;
      }
      await scoreConversation(getActiveApiKey(settings), conversationId, settings.scoringModel);
      const updated = await getScores(conversationId);
      setScore(updated[0] ?? null);
    } catch (err) {
      setError(String(err));
    } finally {
      setScoring(false);
    }
  }

  const handleVibeAnalyse = useCallback(async () => {
    if (!conversationId || vibeAnalysing) return;
    setVibeAnalysing(true);
    setVibeError(null);
    setVibeImprovements([]);
    setVibeAnalysed(false);
    try {
      const settings = await getSettings();
      if (!hasAiCredentials(settings)) {
        setVibeError("Configure AI credentials in Settings first.");
        return;
      }
      const results = await analyzeChatVibe(
        conversationId,
        getActiveApiKey(settings),
        settings.scoringModel || undefined,
      );
      setVibeImprovements(results);
      setVibeAnalysed(true);
    } catch (err) {
      setVibeError(String(err));
    } finally {
      setVibeAnalysing(false);
    }
  }, [conversationId, vibeAnalysing]);

  function handleOpenModal() {
    setModalOpen(true);
    // Auto-start analysis if not yet done
    if (!vibeAnalysed && !vibeAnalysing) {
      void handleVibeAnalyse();
    }
  }

  if (!conversationId) {
    return (
      <Shell title="Conversation" subtitle="No conversation selected">
        <p className="text-sm text-muted">Open a conversation from the list to view its transcript.</p>
      </Shell>
    );
  }

  return (
    <>
      <Shell
        title={`Conversation #${conversationId}`}
        subtitle="Transcript and rubric breakdown"
        actions={
          <button
            type="button"
            onClick={() => router.back()}
            className="inline-flex items-center gap-1.5 rounded-lg border border-panel-border bg-panel px-3 py-2 text-sm font-medium text-muted shadow-sm transition-all hover:border-accent/40 hover:text-foreground"
          >
            ← Back
          </button>
        }
      >
        {exportNotice && (
          <div className="mb-4 rounded-xl border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-800">
            {exportNotice}
          </div>
        )}
        {error && (
          <div className="mb-4 flex items-start gap-3 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            <strong>Error:</strong> {error}
          </div>
        )}
        {isRuntime === false && (
          <div className="mb-4 rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-700">
            Launch the desktop app to view conversation data.
          </div>
        )}

        {loading ? (
          <div className="flex h-64 items-center justify-center">
            <div className="text-sm text-muted animate-pulse">Loading…</div>
          </div>
        ) : (
          <div className="flex min-h-[calc(100vh-12rem)] overflow-hidden rounded-xl border border-panel-border bg-panel shadow-sm">
            {/* ── Transcript ── */}
            <section className="flex-[3] overflow-y-auto border-r border-panel-border p-6">
              <div className="mb-4 flex items-center justify-between gap-3">
                <h2 className="text-xs font-semibold uppercase tracking-widest text-muted">Transcript</h2>
                <div className="flex items-center gap-2">
                  {messages.length > 0 && isRuntime && (
                    <button
                      type="button"
                      onClick={handleExportMarkdown}
                      disabled={exporting}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-panel-border bg-background px-3 py-1.5 text-xs font-medium text-foreground shadow-sm hover:border-accent/40 disabled:opacity-50"
                    >
                      {exporting ? "Exporting…" : "Export Markdown"}
                    </button>
                  )}
                  {!score && isRuntime && (
                    <button
                      type="button"
                      onClick={handleScore}
                      disabled={scoring}
                      className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-white shadow-sm hover:bg-accent/90 disabled:opacity-50"
                    >
                      {scoring ? "Scoring…" : "Score this conversation"}
                    </button>
                  )}
                </div>
              </div>
              {messages.length === 0 ? (
                <p className="text-sm text-muted">No messages found.</p>
              ) : (
                <div className="space-y-4">
                  {transcriptBlocks.map((block) =>
                    block.type === "user" ? (
                      <UserMessageBlock key={block.key} content={block.content} />
                    ) : (
                      <AssistantMessageBlock
                        key={block.key}
                        parts={block.parts}
                        expanded={expandedBlocks.has(block.key)}
                        onToggle={() => toggleBlock(block.key)}
                      />
                    ),
                  )}
                </div>
              )}
            </section>

            {/* ── Right panel ── */}
            <section className="flex w-80 shrink-0 flex-col gap-4 overflow-y-auto p-6">

              {/* ── See improvement suggestions — always at top ── */}
              {isRuntime && messages.length > 0 && (
                <button
                  type="button"
                  onClick={handleOpenModal}
                  className="flex w-full items-center justify-between gap-3 rounded-xl border border-accent/30 bg-accent/5 px-4 py-3 text-left transition-colors hover:bg-accent/10 hover:border-accent/50 group"
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    <svg className="h-4 w-4 shrink-0 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z" />
                    </svg>
                    <div className="min-w-0">
                      <p className="text-xs font-semibold text-accent">See improvement suggestions</p>
                      <p className="text-[10px] text-muted mt-0.5 truncate">
                        {vibeAnalysed
                          ? `${vibeImprovements.length} suggestion${vibeImprovements.length === 1 ? "" : "s"} found`
                          : "Get targeted improvements"}
                      </p>
                    </div>
                  </div>
                  <svg className="h-4 w-4 shrink-0 text-accent/60 group-hover:text-accent transition-colors" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
                  </svg>
                </button>
              )}

              {/* ── Score section ── */}
              {score ? (
                <>
                  <div className="rounded-xl border border-panel-border bg-background p-5 text-center">
                    <p className="mb-1 text-xs font-semibold uppercase tracking-widest text-muted">Vibe Score</p>
                    <p className={`text-6xl font-black tabular-nums ${score.finalScore >= 4 ? "text-emerald-600" : score.finalScore >= 2.5 ? "text-amber-600" : "text-red-500"}`}>
                      {score.finalScore.toFixed(2)}
                    </p>
                    <p className="mt-1 text-xs text-muted">out of 5.00</p>
                  </div>

                  <div className="rounded-xl border border-panel-border bg-background p-5">
                    <h3 className="mb-4 text-xs font-semibold uppercase tracking-widest text-muted">Rubric Breakdown</h3>
                    <div className="space-y-3">
                      {DIMENSION_KEYS.map((key) => (
                        <ScoreBar key={key} label={DIMENSION_LABELS[key]} value={(score as unknown as Record<string, number>)[key] ?? 0} />
                      ))}
                    </div>
                  </div>

                  {score.explanation && (
                    <div className="rounded-xl border border-panel-border bg-background p-5">
                      <h3 className="mb-2 text-xs font-semibold uppercase tracking-widest text-muted">Explanation</h3>
                      <p className="text-sm leading-relaxed text-foreground/80">{score.explanation}</p>
                    </div>
                  )}

                  <div className="rounded-xl border border-panel-border bg-background p-5">
                    <h3 className="mb-3 text-xs font-semibold uppercase tracking-widest text-muted">Metadata</h3>
                    <dl className="space-y-2 text-sm">
                      {([
                        ["Model", score.modelId],
                        ["Rubric", score.rubricVersion],
                        ["Prompt", score.promptVersion],
                        ["Scored", new Date(score.scoredAt).toLocaleString()],
                      ] as [string, string][]).map(([label, value]) => (
                        <div key={label} className="flex justify-between gap-4">
                          <dt className="text-muted">{label}</dt>
                          <dd className="truncate text-right font-mono text-xs text-foreground max-w-36">{value}</dd>
                        </div>
                      ))}
                    </dl>
                  </div>
                </>
              ) : (
                <div className="rounded-xl border border-panel-border bg-background p-8 text-center">
                  <p className="mb-4 text-sm text-muted">Not yet scored.</p>
                  {isRuntime && (
                    <button
                      type="button"
                      onClick={handleScore}
                      disabled={scoring}
                      className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-accent/90 disabled:opacity-50"
                    >
                      {scoring ? "Scoring…" : "Score now"}
                    </button>
                  )}
                </div>
              )}
            </section>
          </div>
        )}
      </Shell>

      {/* ── Improvements modal (rendered outside Shell so it overlays everything) ── */}
      <ImprovementsModal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        onAnalyse={handleVibeAnalyse}
        analysing={vibeAnalysing}
        analysed={vibeAnalysed}
        improvements={vibeImprovements}
        error={vibeError}
      />
    </>
  );
}
