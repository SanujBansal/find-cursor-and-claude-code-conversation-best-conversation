"use client";

import { useEffect, useState } from "react";
import { Shell } from "@/components/Shell";
import { analyzeChatVibe, getSettings, listConversations } from "../../src/lib/tauri";
import { useTauriRuntime } from "../../src/lib/useTauriRuntime";
import type { ConversationSummary, VibeImprovement } from "../../src/lib/types";
import { getActiveApiKey, hasAiCredentials } from "../../src/lib/types";

// ── helpers ───────────────────────────────────────────────────────────────────

function fmt(iso: string | null): string {
  if (!iso) return "—";
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return iso;
  }
}

function scoreLabel(s: number | null): string {
  if (s == null) return "—";
  return s.toFixed(2);
}

function scoreColor(s: number | null): string {
  if (s == null) return "text-muted";
  if (s >= 4) return "text-emerald-500";
  if (s >= 2.5) return "text-amber-500";
  return "text-[#f87171]";
}

function scoreBarColor(s: number | null): string {
  if (s == null) return "bg-panel-border";
  if (s >= 4) return "bg-emerald-500";
  if (s >= 2.5) return "bg-amber-400";
  return "bg-[#f87171]";
}

// ── sub-components ────────────────────────────────────────────────────────────

function ChatRow({
  conv,
  selected,
  onSelect,
}: {
  conv: ConversationSummary;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={[
        "w-full text-left flex items-center gap-3 px-4 py-3 border-b border-panel-border transition-colors",
        selected
          ? "bg-accent/10 border-l-2 border-l-accent"
          : "hover:bg-muted/30 border-l-2 border-l-transparent",
      ].join(" ")}
    >
      {/* Score mini-bar */}
      <div className="flex flex-col items-center gap-1 shrink-0 w-10">
        <span className={`text-xs font-semibold tabular-nums ${scoreColor(conv.finalScore)}`}>
          {scoreLabel(conv.finalScore)}
        </span>
        <div className="h-1 w-10 rounded-full bg-panel-border overflow-hidden">
          <div
            className={`h-1 rounded-full ${scoreBarColor(conv.finalScore)}`}
            style={{ width: conv.finalScore != null ? `${(conv.finalScore / 5) * 100}%` : "0%" }}
          />
        </div>
      </div>

      {/* Title + meta */}
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-foreground truncate leading-snug">
          {conv.title || "Untitled"}
        </p>
        <p className="text-[10px] text-muted mt-0.5">
          {fmt(conv.completedAt)} · {conv.userMessageCount} prompts
        </p>
      </div>

      {selected && (
        <svg className="h-4 w-4 shrink-0 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
        </svg>
      )}
    </button>
  );
}

function ImprovementCard({ item }: { item: VibeImprovement }) {
  return (
    <div className="rounded-xl border border-panel-border bg-panel overflow-hidden">
      {/* Header: index + tip */}
      <div className="flex items-start gap-3 px-4 py-3 border-b border-panel-border bg-[#f87171]/5">
        <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-[#f87171]/20 text-[11px] font-bold text-[#f87171] mt-0.5">
          {item.index}
        </span>
        <p className="text-xs text-muted leading-relaxed pt-0.5">{item.tip}</p>
      </div>

      {/* Before / After */}
      <div className="grid sm:grid-cols-2 divide-y sm:divide-y-0 sm:divide-x divide-panel-border">
        <div className="p-4 flex flex-col gap-1.5">
          <p className="text-[10px] uppercase tracking-[0.16em] text-[#f87171] font-medium">
            Original prompt
          </p>
          <p className="text-xs text-muted font-mono leading-relaxed whitespace-pre-wrap">
            {item.badPrompt}
          </p>
        </div>
        <div className="p-4 flex flex-col gap-1.5">
          <p className="text-[10px] uppercase tracking-[0.16em] text-emerald-500 font-medium">
            Improved prompt
          </p>
          <p className="text-xs text-foreground font-mono leading-relaxed whitespace-pre-wrap">
            {item.improvedPrompt}
          </p>
        </div>
      </div>
    </div>
  );
}

function SkeletonCard() {
  return (
    <div className="rounded-xl border border-panel-border bg-panel p-4 animate-pulse flex flex-col gap-3">
      <div className="h-3 w-2/3 rounded bg-panel-border" />
      <div className="grid sm:grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <div className="h-2 w-16 rounded bg-panel-border" />
          <div className="h-2 w-full rounded bg-panel-border" />
          <div className="h-2 w-4/5 rounded bg-panel-border" />
        </div>
        <div className="space-y-1.5">
          <div className="h-2 w-16 rounded bg-panel-border" />
          <div className="h-2 w-full rounded bg-panel-border" />
          <div className="h-2 w-4/5 rounded bg-panel-border" />
        </div>
      </div>
    </div>
  );
}

// ── main page ─────────────────────────────────────────────────────────────────

export default function ImproveTheVibePage() {
  const isRuntime = useTauriRuntime();

  const [chats, setChats] = useState<ConversationSummary[]>([]);
  const [loadingChats, setLoadingChats] = useState(true);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [improvements, setImprovements] = useState<VibeImprovement[]>([]);
  const [analysing, setAnalysing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [aiReady, setAiReady] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [scoringModel, setScoringModel] = useState("");
  const [analysedId, setAnalysedId] = useState<number | null>(null);

  // Load settings + recent 50 chats
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (!isRuntime) {
        if (!cancelled) setLoadingChats(false);
        return;
      }
      try {
        const [settings, all] = await Promise.all([
          getSettings().catch(() => null),
          listConversations(),
        ]);
        if (cancelled) return;
        if (settings) {
          setApiKey(getActiveApiKey(settings));
          setAiReady(hasAiCredentials(settings));
          setScoringModel(settings.scoringModel || "");
        }
        // Most recent 50 chats (list_conversations already orders by completed_at DESC)
        setChats(all.slice(0, 50));
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoadingChats(false);
      }
    })();
    return () => { cancelled = true; };
  }, [isRuntime]);

  async function handleAnalyse() {
    if (selectedId == null || analysing || !aiReady) return;
    setAnalysing(true);
    setError(null);
    setImprovements([]);
    setAnalysedId(null);
    try {
      const results = await analyzeChatVibe(
        selectedId,
        apiKey,
        scoringModel || undefined,
      );
      setImprovements(results);
      setAnalysedId(selectedId);
    } catch (e) {
      setError(String(e));
    } finally {
      setAnalysing(false);
    }
  }

  const selectedChat = chats.find((c) => c.id === selectedId) ?? null;
  const showResults = analysedId === selectedId && improvements.length > 0;
  const showEmpty = analysedId === selectedId && !analysing && improvements.length === 0;

  return (
    <Shell
      title="Improve the Vibe"
      subtitle="Pick a chat, get up to 10 concrete prompt upgrades based on how you actually prompted."
    >
      <div className="flex flex-col gap-5">
        {/* Error banner */}
        {error && (
          <div className="rounded-lg border border-[#f87171]/30 bg-[#f87171]/8 px-4 py-3 text-xs text-[#f87171]">
            <span className="font-semibold">Error: </span>{error}
          </div>
        )}

        {isRuntime === false && (
          <div className="rounded-lg border border-panel-border bg-panel px-4 py-3 text-sm text-muted">
            Tauri runtime required — launch the desktop app.
          </div>
        )}

        {/* ── two-column layout: chat picker left, results right ── */}
        <div className="flex min-h-[600px] overflow-hidden rounded-xl border border-panel-border bg-panel shadow-sm">

          {/* Left: chat list */}
          <aside className="w-72 shrink-0 border-r border-panel-border bg-background flex flex-col">
            <div className="border-b border-panel-border px-4 py-3">
              <p className="text-xs font-semibold uppercase tracking-wider text-muted">
                Recent chats
              </p>
              <p className="mt-0.5 text-[11px] text-muted">
                {loadingChats ? "Loading…" : `${chats.length} most recent`}
              </p>
            </div>

            <div className="flex-1 overflow-y-auto">
              {loadingChats ? (
                <div className="flex flex-col gap-2 p-3">
                  {[0, 1, 2, 3, 4].map((i) => (
                    <div key={i} className="h-14 animate-pulse rounded-lg bg-muted/30" />
                  ))}
                </div>
              ) : chats.length === 0 ? (
                <p className="px-4 py-8 text-xs text-muted">
                  No conversations yet. Import transcripts from Settings first.
                </p>
              ) : (
                chats.map((c) => (
                  <ChatRow
                    key={c.id}
                    conv={c}
                    selected={c.id === selectedId}
                    onSelect={() => {
                      setSelectedId(c.id);
                      setImprovements([]);
                      setAnalysedId(null);
                      setError(null);
                    }}
                  />
                ))
              )}
            </div>
          </aside>

          {/* Right: analysis panel */}
          <div className="flex flex-1 flex-col min-w-0">
            {/* Toolbar */}
            <div className="border-b border-panel-border px-5 py-4 flex flex-wrap items-center justify-between gap-3">
              <div className="min-w-0">
                {selectedChat ? (
                  <>
                    <h2 className="text-sm font-semibold text-foreground truncate">
                      {selectedChat.title || "Untitled"}
                    </h2>
                    <p className="text-[11px] text-muted mt-0.5">
                      {fmt(selectedChat.completedAt)} · {selectedChat.userMessageCount} user prompts
                      {selectedChat.finalScore != null && (
                        <> · score <span className={scoreColor(selectedChat.finalScore)}>{scoreLabel(selectedChat.finalScore)}</span></>
                      )}
                    </p>
                  </>
                ) : (
                  <p className="text-sm text-muted">Select a chat to analyse</p>
                )}
              </div>

              <div className="flex items-center gap-2">
                {isRuntime && !aiReady && (
                  <span className="text-xs text-[#f87171]">Configure AI credentials in Settings</span>
                )}
                <button
                  type="button"
                  onClick={handleAnalyse}
                  disabled={selectedId == null || analysing || !isRuntime || !aiReady}
                  className="rounded-lg bg-accent px-4 py-1.5 text-xs font-medium text-white hover:bg-accent/90 disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-2 transition-colors"
                >
                  {analysing && (
                    <span className="inline-block h-3 w-3 rounded-full border-2 border-white/40 border-t-white animate-spin" />
                  )}
                  {analysing ? "Analysing…" : "Analyse prompts"}
                </button>
              </div>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto p-5">
              {!selectedId ? (
                <div className="flex flex-col items-center justify-center h-full gap-3 py-16 text-center">
                  <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-accent/10">
                    <svg className="h-6 w-6 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.75}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09z" />
                    </svg>
                  </div>
                  <p className="text-sm font-medium text-foreground">Pick a chat on the left</p>
                  <p className="text-xs text-muted max-w-xs leading-relaxed">
                    Select any of your recent 50 sessions and click <strong>Analyse prompts</strong> to get up to 10 concrete prompt upgrades.
                  </p>
                </div>
              ) : analysing ? (
                <div className="flex flex-col gap-4">
                  <p className="text-xs text-muted mb-2">Reading your prompts and crafting improvements…</p>
                  {[0, 1, 2, 3].map((i) => <SkeletonCard key={i} />)}
                </div>
              ) : showEmpty ? (
                <div className="flex flex-col items-center justify-center h-full gap-3 py-16 text-center">
                  <p className="text-sm font-medium text-foreground">No issues found</p>
                  <p className="text-xs text-muted max-w-xs leading-relaxed">
                    Your prompts in this chat look solid — the AI coach found nothing specific to flag. Try another session.
                  </p>
                </div>
              ) : showResults ? (
                <div className="flex flex-col gap-4">
                  <div className="flex items-center justify-between">
                    <p className="text-xs text-muted">
                      <span className="text-foreground font-semibold">{improvements.length}</span> prompt improvement{improvements.length === 1 ? "" : "s"} found
                    </p>
                    <span className="text-[10px] uppercase tracking-[0.16em] text-accent">Improve the Vibe</span>
                  </div>
                  {improvements.map((item) => (
                    <ImprovementCard key={item.index} item={item} />
                  ))}
                </div>
              ) : (
                <div className="flex flex-col items-center justify-center h-full gap-3 py-16 text-center">
                  <p className="text-sm text-muted">Click <strong>Analyse prompts</strong> to start</p>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </Shell>
  );
}
