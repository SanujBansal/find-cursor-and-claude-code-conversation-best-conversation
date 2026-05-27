"use client";

import { useRouter, useSearchParams } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import { Shell } from "@/components/Shell";
import {
  exportConversationMarkdown,
  getConversationMessages,
  getScores,
  getSettings,
  scoreConversation,
} from "../../../src/lib/tauri";
import { useTauriRuntime } from "../../../src/lib/useTauriRuntime";
import type { MessageRecord, ScoreRecord } from "../../../src/lib/types";
import { hasAiCredentials } from "../../../src/lib/types";

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

function ScoreBar({ label, value }: { label: string; value: number }) {
  const pct = Math.round((value / 5) * 100);
  const color =
    value >= 4 ? "bg-emerald-500" : value >= 2.5 ? "bg-amber-400" : "bg-red-400";

  return (
    <div className="flex items-center gap-3">
      <span className="w-40 shrink-0 text-xs text-muted">{label}</span>
      <div className="flex-1 h-1.5 rounded-full bg-muted-light overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-500 ${color}`}
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="w-7 text-right text-xs font-semibold tabular-nums text-foreground">
        {value.toFixed(1)}
      </span>
    </div>
  );
}

type TranscriptPart = { role: string; content: string };

type TranscriptBlock =
  | { type: "user"; content: string; key: string }
  | { type: "assistant"; parts: TranscriptPart[]; key: string };

function groupMessages(messages: MessageRecord[]): TranscriptBlock[] {
  const blocks: TranscriptBlock[] = [];

  for (const msg of messages) {
    if (msg.role === "user") {
      blocks.push({
        type: "user",
        content: msg.content,
        key: `user-${msg.sequenceNum}`,
      });
      continue;
    }

    const last = blocks[blocks.length - 1];
    if (last?.type === "assistant") {
      last.parts.push({ role: msg.role, content: msg.content });
      continue;
    }

    blocks.push({
      type: "assistant",
      parts: [{ role: msg.role, content: msg.content }],
      key: `assistant-${msg.sequenceNum}`,
    });
  }

  return blocks;
}

function previewText(content: string, maxLength = 120): string {
  const trimmed = content.trim().replace(/\s+/g, " ");
  if (trimmed.length <= maxLength) return trimmed;
  return `${trimmed.slice(0, maxLength).trimEnd()}…`;
}

function assistantSummary(parts: TranscriptPart[]): string {
  const assistantCount = parts.filter((part) => part.role === "assistant").length;
  const totalChars = parts.reduce((sum, part) => sum + part.content.length, 0);

  if (assistantCount > 1) {
    return `${assistantCount} assistant messages · ${totalChars.toLocaleString()} chars`;
  }

  if (parts.length > 1) {
    return `${parts.length} messages · ${totalChars.toLocaleString()} chars`;
  }

  return previewText(parts[0]?.content ?? "");
}

function UserMessageBlock({ content }: { content: string }) {
  return (
    <div className="rounded-xl border-2 border-accent/25 bg-accent-light/40 p-5 shadow-sm">
      <div className="mb-3 flex items-center gap-2">
        <span className="inline-flex h-6 w-6 items-center justify-center rounded-full bg-accent text-[10px] font-bold uppercase tracking-wide text-white">
          U
        </span>
        <span className="text-xs font-semibold uppercase tracking-widest text-accent">
          You
        </span>
      </div>
      <p className="text-[15px] font-medium leading-relaxed text-foreground whitespace-pre-wrap break-words">
        {content}
      </p>
    </div>
  );
}

function AssistantMessageBlock({
  parts,
  expanded,
  onToggle,
}: {
  parts: TranscriptPart[];
  expanded: boolean;
  onToggle: () => void;
}) {
  const summary = assistantSummary(parts);

  return (
    <div className="rounded-xl border border-panel-border/80 bg-muted-light/40">
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-start gap-3 px-4 py-3 text-left transition-colors hover:bg-muted-light/80"
        aria-expanded={expanded}
      >
        <span
          className={`mt-0.5 shrink-0 text-xs text-muted transition-transform ${expanded ? "rotate-90" : ""}`}
          aria-hidden
        >
          ▶
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs font-medium uppercase tracking-widest text-muted">
              Assistant
            </span>
            {!expanded && (
              <span className="text-xs text-muted/80">{summary}</span>
            )}
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
                <span className="mb-1 inline-flex rounded-full bg-background px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-muted ring-1 ring-panel-border">
                  {part.role}
                </span>
              )}
              <p className="text-sm leading-relaxed text-foreground/75 whitespace-pre-wrap break-words">
                {part.content}
              </p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

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

    return () => {
      cancelled = true;
    };
  }, [conversationId, isRuntime]);

  const transcriptBlocks = useMemo(() => groupMessages(messages), [messages]);

  function toggleBlock(key: string) {
    setExpandedBlocks((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
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
      if (savedPath) {
        setExportNotice(`Exported to ${savedPath}`);
      }
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
        setError("Azure AI is not configured — set credentials in `.env` or Settings");
        return;
      }
      await scoreConversation(
        settings.openaiApiKey,
        conversationId,
        settings.scoringModel,
      );
      const updated = await getScores(conversationId);
      setScore(updated[0] ?? null);
    } catch (err) {
      setError(String(err));
    } finally {
      setScoring(false);
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
          <section className="flex-[3] overflow-y-auto border-r border-panel-border p-6">
            <div className="mb-4 flex items-center justify-between gap-3">
              <h2 className="text-xs font-semibold uppercase tracking-widest text-muted">
                Transcript
              </h2>
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

          <section className="flex w-80 shrink-0 flex-col gap-4 overflow-y-auto p-6">
            {score ? (
              <>
                <div className="rounded-xl border border-panel-border bg-background p-5 text-center">
                  <p className="mb-1 text-xs font-semibold uppercase tracking-widest text-muted">
                    Vibe Score
                  </p>
                  <p
                    className={`text-6xl font-black tabular-nums ${
                      score.finalScore >= 4
                        ? "text-emerald-600"
                        : score.finalScore >= 2.5
                          ? "text-amber-600"
                          : "text-red-500"
                    }`}
                  >
                    {score.finalScore.toFixed(2)}
                  </p>
                  <p className="mt-1 text-xs text-muted">out of 5.00</p>
                </div>

                <div className="rounded-xl border border-panel-border bg-background p-5">
                  <h3 className="mb-4 text-xs font-semibold uppercase tracking-widest text-muted">
                    Rubric Breakdown
                  </h3>
                  <div className="space-y-3">
                    {DIMENSION_KEYS.map((key) => (
                      <ScoreBar
                        key={key}
                        label={DIMENSION_LABELS[key]}
                        value={(score as unknown as Record<string, number>)[key] ?? 0}
                      />
                    ))}
                  </div>
                </div>

                {score.explanation && (
                  <div className="rounded-xl border border-panel-border bg-background p-5">
                    <h3 className="mb-2 text-xs font-semibold uppercase tracking-widest text-muted">
                      Explanation
                    </h3>
                    <p className="text-sm leading-relaxed text-foreground/80">
                      {score.explanation}
                    </p>
                  </div>
                )}

                <div className="rounded-xl border border-panel-border bg-background p-5">
                  <h3 className="mb-3 text-xs font-semibold uppercase tracking-widest text-muted">
                    Metadata
                  </h3>
                  <dl className="space-y-2 text-sm">
                    {(
                      [
                        ["Model", score.modelId],
                        ["Rubric", score.rubricVersion],
                        ["Prompt", score.promptVersion],
                        ["Scored", new Date(score.scoredAt).toLocaleString()],
                      ] as [string, string][]
                    ).map(([label, value]) => (
                      <div key={label} className="flex justify-between gap-4">
                        <dt className="text-muted">{label}</dt>
                        <dd className="truncate text-right font-mono text-xs text-foreground max-w-36">
                          {value}
                        </dd>
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
  );
}
