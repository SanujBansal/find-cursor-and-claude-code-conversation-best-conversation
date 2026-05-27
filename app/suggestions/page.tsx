"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Shell } from "@/components/Shell";
import {
  dismissSuggestion,
  generateSuggestions,
  getSuggestions,
  getSettings,
} from "../../src/lib/tauri";
import { useTauriRuntime } from "../../src/lib/useTauriRuntime";
import type { LearningSuggestion, SuggestionPriority } from "../../src/lib/types";
import { hasAiCredentials } from "../../src/lib/types";

// ── helpers ───────────────────────────────────────────────────────────────────

function formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

function priorityStyles(priority: SuggestionPriority): {
  badge: string;
  dot: string;
  label: string;
} {
  switch (priority) {
    case "high":
      return { badge: "text-[#f87171] border-[#f87171]/40 bg-[#f87171]/8", dot: "bg-[#f87171]", label: "HIGH" };
    case "medium":
      return { badge: "text-[#fb923c] border-[#fb923c]/40 bg-[#fb923c]/8", dot: "bg-[#fb923c]", label: "MED" };
    default:
      return { badge: "text-muted border-panel-border bg-panel", dot: "bg-muted", label: "LOW" };
  }
}

function dimensionLabel(dim: string): string {
  const map: Record<string, string> = {
    taskCompletion: "Task Completion",
    technicalCorrectness: "Technical Correctness",
    workflowQuality: "Workflow Quality",
    toolUseAndContext: "Tool Use & Context",
    communicationClarity: "Communication Clarity",
    learningLeverage: "Learning Leverage",
  };
  return map[dim] ?? dim;
}

// ── sub-components ────────────────────────────────────────────────────────────

function PriorityBadge({ priority }: { priority: SuggestionPriority }) {
  const { badge, dot, label } = priorityStyles(priority);
  return (
    <span className={`inline-flex items-center gap-1.5 rounded border px-2 py-0.5 text-[10px] tracking-wider ${badge}`}>
      <span className={`h-1.5 w-1.5 rounded-full ${dot}`} />
      {label}
    </span>
  );
}

function DimensionChip({ dimension }: { dimension: string }) {
  return (
    <span className="rounded border border-accent/30 px-2 py-0.5 text-[10px] text-accent tracking-wide">
      {dimensionLabel(dimension)}
    </span>
  );
}

function SkeletonCard() {
  return (
    <div className="rounded border border-panel-border bg-panel p-5 animate-pulse">
      <div className="flex items-start justify-between gap-3 mb-3">
        <div className="h-4 w-48 rounded bg-panel-border" />
        <div className="h-5 w-14 rounded bg-panel-border" />
      </div>
      <div className="space-y-2">
        <div className="h-3 w-full rounded bg-panel-border" />
        <div className="h-3 w-4/5 rounded bg-panel-border" />
        <div className="h-3 w-3/5 rounded bg-panel-border" />
      </div>
      <div className="mt-3 h-5 w-28 rounded bg-panel-border" />
    </div>
  );
}

function SuggestionCard({
  suggestion,
  onDismiss,
}: {
  suggestion: LearningSuggestion;
  onDismiss: (id: string) => void;
}) {
  const [dismissing, setDismissing] = useState(false);

  async function handleDismiss() {
    setDismissing(true);
    onDismiss(suggestion.id);
  }

  return (
    <div className={`rounded border border-panel-border bg-panel p-5 flex flex-col gap-3 transition-opacity ${dismissing ? "opacity-40" : ""}`}>
      <div className="flex items-start justify-between gap-3">
        <p className="font-mono text-sm font-semibold text-foreground leading-snug">
          {suggestion.concept}
        </p>
        <div className="flex items-center gap-2 shrink-0">
          <PriorityBadge priority={suggestion.priority} />
          <button
            type="button"
            onClick={handleDismiss}
            disabled={dismissing}
            aria-label="Dismiss suggestion"
            className="rounded border border-panel-border w-6 h-6 flex items-center justify-center text-muted hover:text-[#f87171] hover:border-[#f87171]/40 transition-colors disabled:opacity-40"
          >
            ×
          </button>
        </div>
      </div>

      <p className="text-xs text-muted leading-relaxed">{suggestion.whyItHelps}</p>

      <div className="flex flex-wrap items-center gap-2">
        <DimensionChip dimension={suggestion.relatedDimension} />
        {suggestion.exampleConversationId && (
          <Link
            href={`/conversations/detail?id=${suggestion.exampleConversationId}`}
            className="text-xs text-accent-muted hover:text-accent transition-colors underline underline-offset-2"
          >
            See example ›
          </Link>
        )}
      </div>
    </div>
  );
}

// ── main page ─────────────────────────────────────────────────────────────────

export default function SuggestionsPage() {
  const [suggestions, setSuggestions] = useState<LearningSuggestion[]>([]);
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [aiReady, setAiReady] = useState(false);
  const [scoringModel, setScoringModel] = useState("");
  const [showDismissed, setShowDismissed] = useState(false);
  const [lastGenerated, setLastGenerated] = useState<string | null>(null);

  const isRuntime = useTauriRuntime();
  const allSuggestions = showDismissed
    ? suggestions
    : suggestions.filter((s) => !s.isDismissed);
  const dismissedCount = suggestions.filter((s) => s.isDismissed).length;
  const activeSuggestions = suggestions.filter((s) => !s.isDismissed);

  // Derive top 3 weak dimensions from active suggestions
  const weakDimensions = Array.from(
    activeSuggestions.reduce<Map<string, number>>((acc, s) => {
      acc.set(s.relatedDimension, (acc.get(s.relatedDimension) ?? 0) + 1);
      return acc;
    }, new Map()),
  )
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3)
    .map(([dim]) => dim);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      if (!isRuntime) {
        if (!cancelled) setLoading(false);
        return;
      }

      try {
        const [settings, data] = await Promise.all([
          getSettings().catch(() => null),
          getSuggestions(true),
        ]);
        if (cancelled) return;
        if (settings) {
          setApiKey(settings.openaiApiKey);
          setAiReady(hasAiCredentials(settings));
          setScoringModel(settings.scoringModel);
        }
        setSuggestions(data);
        const latest = data
          .map((s) => s.generatedAt)
          .sort()
          .at(-1);
        if (latest) setLastGenerated(latest);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [isRuntime]);

  async function handleGenerate() {
    if (!isRuntime || !aiReady || generating) return;
    setGenerating(true);
    setError(null);
    try {
      const newSuggestions = await generateSuggestions(
        apiKey,
        scoringModel || undefined,
      );
      // Merge: replace existing or append new
      setSuggestions((prev) => {
        const byId = new Map(prev.map((s) => [s.id, s]));
        newSuggestions.forEach((s) => byId.set(s.id, s));
        return Array.from(byId.values());
      });
      if (newSuggestions.length > 0) {
        setLastGenerated(newSuggestions[0].generatedAt);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setGenerating(false);
    }
  }

  async function handleDismiss(id: string) {
    try {
      await dismissSuggestion(id);
      setSuggestions((prev) =>
        prev.map((s) => (s.id === id ? { ...s, isDismissed: true } : s)),
      );
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <Shell
      title="Learning Brief"
      subtitle="Targeted recommendations from your weakest rubric dimensions."
    >
      <div className="flex flex-col gap-6">
        {/* ── header bar ── */}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-xs text-muted">
            <span className="uppercase tracking-[0.16em] text-accent">Coach Briefing</span>
            <span className="opacity-40">·</span>
            <span>{new Date().toLocaleDateString(undefined, { month: "long", day: "numeric", year: "numeric" })}</span>
          </div>
          <div className="flex items-center gap-2">
            {isRuntime === false && (
              <span className="text-xs text-muted">Tauri runtime required</span>
            )}
            {isRuntime && !aiReady && (
              <span className="text-xs text-[#f87171]">Configure Azure AI in `.env`</span>
            )}
            <button
              type="button"
              onClick={handleGenerate}
              disabled={generating || !isRuntime || !aiReady}
              className="rounded border border-panel-border px-3 py-1.5 text-xs text-muted hover:text-foreground hover:border-accent transition-colors disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-2"
            >
              {generating && <span className="animate-pulse">⏳</span>}
              {generating ? "Generating…" : "Generate New Suggestions"}
            </button>
          </div>
        </div>

        {/* ── error banner ── */}
        {error && (
          <div className="rounded border border-[#f87171]/30 bg-[#f87171]/8 px-4 py-3 text-xs text-[#f87171]">
            <span className="font-semibold">Error: </span>{error}
          </div>
        )}

        {/* ── weakness summary badges ── */}
        {weakDimensions.length > 0 && (
          <div className="flex flex-col gap-2">
            <p className="text-[10px] uppercase tracking-[0.18em] text-muted">
              Identified Weak Areas
            </p>
            <div className="flex flex-wrap gap-2">
              {weakDimensions.map((dim) => (
                <span
                  key={dim}
                  className="rounded border border-[#f87171]/30 bg-[#f87171]/8 px-3 py-1 text-xs text-[#f87171]"
                >
                  {dimensionLabel(dim)}
                </span>
              ))}
            </div>
          </div>
        )}

        {/* ── suggestion cards ── */}
        <div className="flex flex-col gap-3">
          {loading ? (
            <>
              <SkeletonCard />
              <SkeletonCard />
              <SkeletonCard />
            </>
          ) : allSuggestions.length === 0 ? (
            <div className="rounded border border-panel-border bg-panel px-6 py-12 text-center flex flex-col items-center gap-3">
              <p className="text-xs uppercase tracking-[0.16em] text-muted">No suggestions yet</p>
              <p className="text-sm text-muted max-w-sm leading-relaxed">
                Run scoring first to populate rubric data, then click{" "}
                <span className="text-foreground">Generate New Suggestions</span> to get personalised coaching.
              </p>
            </div>
          ) : (
            allSuggestions.map((s) => (
              <SuggestionCard key={s.id} suggestion={s} onDismiss={handleDismiss} />
            ))
          )}
        </div>

        {/* ── footer ── */}
        {!loading && (
          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-panel-border pt-4 text-xs text-muted">
            <span>
              {lastGenerated
                ? `Last generated: ${formatDate(lastGenerated)}`
                : "Not yet generated"}
            </span>
            {dismissedCount > 0 && (
              <button
                type="button"
                onClick={() => setShowDismissed((v) => !v)}
                className="text-xs text-muted hover:text-foreground transition-colors underline underline-offset-2"
              >
                {showDismissed
                  ? "Hide dismissed"
                  : `Show ${dismissedCount} dismissed`}
              </button>
            )}
          </div>
        )}
      </div>
    </Shell>
  );
}
