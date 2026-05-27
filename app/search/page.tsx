"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { Shell } from "@/components/Shell";
import { chatSearch, embedPending, getSettings } from "../../src/lib/tauri";
import { useTauriRuntime } from "../../src/lib/useTauriRuntime";
import type { ChatSearchResponse, SearchResult } from "../../src/lib/types";
import { hasAiCredentials } from "../../src/lib/types";

// ── helpers ───────────────────────────────────────────────────────────────────

function SimilarityBadge({ score }: { score: number }) {
  const pct = Math.round(score * 100);
  const colour =
    pct >= 80
      ? "text-accent"
      : pct >= 60
        ? "text-foreground"
        : "text-muted";
  return <span className={`text-xs tabular-nums ${colour}`}>{pct}%</span>;
}

function SourceCard({ source }: { source: SearchResult }) {
  return (
    <div className="rounded border border-panel-border bg-panel p-4 flex flex-col gap-2">
      <div className="flex items-start justify-between gap-3">
        <div className="flex flex-col gap-0.5 min-w-0">
          <p className="text-sm text-foreground truncate">{source.conversationTitle}</p>
          {source.projectPath && (
            <p className="text-xs text-muted truncate">{source.projectPath}</p>
          )}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <SimilarityBadge score={source.similarity} />
          <span className="text-xs text-muted">{source.sourceType}</span>
          <Link
            href={`/conversations/detail?id=${source.conversationId}`}
            className="rounded border border-panel-border px-2 py-0.5 text-xs text-muted hover:text-foreground hover:border-accent transition-colors"
          >
            Open
          </Link>
        </div>
      </div>
      <p className="text-xs text-muted leading-relaxed line-clamp-3 whitespace-pre-wrap">
        {source.chunkText.slice(0, 300)}
        {source.chunkText.length > 300 ? "…" : ""}
      </p>
    </div>
  );
}

// ── message types ─────────────────────────────────────────────────────────────

type Message =
  | { role: "user"; text: string }
  | { role: "assistant"; response: ChatSearchResponse }
  | { role: "error"; text: string };

// ── main component ────────────────────────────────────────────────────────────

export default function SearchPage() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [aiReady, setAiReady] = useState(false);
  const [embeddingModel, setEmbeddingModel] = useState("");
  const [chatModel, setChatModel] = useState("");
  const [embedStatus, setEmbedStatus] = useState<string | null>(null);

  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const isRuntime = useTauriRuntime();

  // Load API key from settings
  useEffect(() => {
    if (!isRuntime) return;
    getSettings()
      .then((s) => {
        setApiKey(s.openaiApiKey);
        setAiReady(hasAiCredentials(s));
        setEmbeddingModel(s.embeddingModel);
        setChatModel(s.scoringModel);
      })
      .catch(() => {});
  }, [isRuntime]);

  // Scroll to bottom when messages change
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, loading]);

  async function handleEmbed() {
    if (!isRuntime || !aiReady || loading) return;
    setLoading(true);
    setEmbedStatus("Embedding pending transcripts…");
    try {
      const result = await embedPending(apiKey, embeddingModel || undefined);
      setEmbedStatus(
        `Embedded ${result.embedded} conversation(s) · ${result.chunksCreated} new chunks`
      );
    } catch (e) {
      setEmbedStatus(`Embed error: ${String(e)}`);
    } finally {
      setLoading(false);
    }
  }

  async function handleSend() {
    const q = input.trim();
    if (!q || loading || !isRuntime || !aiReady) return;

    setInput("");
    setMessages((prev) => [...prev, { role: "user", text: q }]);
    setLoading(true);

    try {
      const response = await chatSearch(q, apiKey, {
        embeddingModel: embeddingModel || undefined,
        chatModel: chatModel || undefined,
      });
      setMessages((prev) => [...prev, { role: "assistant", response }]);
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: "error", text: String(e) },
      ]);
    } finally {
      setLoading(false);
      inputRef.current?.focus();
    }
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  return (
    <Shell
      title="Search"
      subtitle="Chat with your local transcript memory index."
    >
      <div className="flex flex-col h-[calc(100vh-8rem)] gap-0">
        {/* ── toolbar ── */}
        <div className="flex items-center gap-3 pb-3 border-b border-panel-border">
          <button
            type="button"
            onClick={handleEmbed}
            disabled={loading || !isRuntime || !aiReady}
            className="rounded border border-panel-border px-3 py-1 text-xs text-muted hover:text-foreground hover:border-accent transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            Embed pending
          </button>
          {embedStatus && (
            <span className="text-xs text-muted">{embedStatus}</span>
          )}
          {!aiReady && isRuntime && (
            <span className="text-xs text-[#f87171]">
              Configure Azure AI in `.env`
            </span>
          )}
          {isRuntime === false && (
            <span className="text-xs text-muted">
              Running in browser preview — Tauri commands unavailable
            </span>
          )}
        </div>

        {/* ── message thread ── */}
        <div className="flex-1 overflow-y-auto py-4 flex flex-col gap-5 min-h-0">
          {messages.length === 0 && !loading && (
            <div className="flex flex-col items-start gap-2 pt-4">
              <p className="text-xs uppercase tracking-[0.16em] text-accent">
                Semantic Recall
              </p>
              <p className="text-sm text-muted leading-relaxed max-w-lg">
                Ask any question about your coding sessions. Transcripts are
                chunked and embedded locally — answers are grounded in your
                actual conversations.
              </p>
              <div className="mt-2 flex flex-col gap-1">
                {[
                  "Which sessions dealt with React server components?",
                  "What debugging approaches did I use last week?",
                  "Find conversations about database migrations.",
                ].map((hint) => (
                  <button
                    key={hint}
                    type="button"
                    onClick={() => {
                      setInput(hint);
                      inputRef.current?.focus();
                    }}
                    className="text-left text-xs text-muted hover:text-accent transition-colors"
                  >
                    <span className="text-accent-muted mr-1">›</span>
                    {hint}
                  </button>
                ))}
              </div>
            </div>
          )}

          {messages.map((msg, i) => {
            if (msg.role === "user") {
              return (
                <div key={i} className="flex gap-2 items-start">
                  <span className="text-accent-muted text-sm shrink-0 mt-0.5">&gt;</span>
                  <p className="text-sm text-foreground whitespace-pre-wrap">{msg.text}</p>
                </div>
              );
            }

            if (msg.role === "error") {
              return (
                <div key={i} className="flex gap-2 items-start">
                  <span className="text-[#f87171] text-sm shrink-0 mt-0.5">!</span>
                  <p className="text-sm text-[#f87171]">{msg.text}</p>
                </div>
              );
            }

            // assistant message
            const { response } = msg;
            return (
              <div key={i} className="flex flex-col gap-4">
                {/* Answer */}
                <div className="flex gap-2 items-start">
                  <span className="text-muted text-sm shrink-0 mt-0.5">$</span>
                  <p className="text-sm text-foreground leading-relaxed whitespace-pre-wrap">
                    {response.answer}
                  </p>
                </div>

                {/* Source cards */}
                {response.sources.length > 0 && (
                  <div className="ml-4 flex flex-col gap-2">
                    <p className="text-xs uppercase tracking-[0.14em] text-muted">
                      Sources · {response.sources.length}
                    </p>
                    {response.sources.map((source, j) => (
                      <SourceCard key={`${i}-${j}`} source={source} />
                    ))}
                  </div>
                )}
              </div>
            );
          })}

          {loading && (
            <div className="flex gap-2 items-center">
              <span className="text-muted text-sm">$</span>
              <span className="text-sm text-muted animate-pulse">Searching…</span>
            </div>
          )}

          <div ref={bottomRef} />
        </div>

        {/* ── input bar ── */}
        <div className="border-t border-panel-border pt-3">
          <div className="flex items-center gap-2 rounded border border-panel-border bg-panel px-3 py-2 focus-within:ring-1 focus-within:ring-accent">
            <span className="text-accent-muted text-sm shrink-0">&gt;</span>
            <input
              ref={inputRef}
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={
                isRuntime
                  ? "Ask about your coding sessions…"
                  : "Tauri runtime required"
              }
              disabled={loading || !isRuntime}
              className="flex-1 bg-transparent text-sm text-foreground placeholder:text-muted focus:outline-none disabled:cursor-not-allowed"
            />
            <button
              type="button"
              onClick={handleSend}
              disabled={loading || !input.trim() || !isRuntime || !aiReady}
              className="shrink-0 rounded border border-panel-border px-3 py-1 text-xs text-muted hover:text-foreground hover:border-accent transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              Send
            </button>
          </div>
          <p className="mt-1 text-xs text-muted opacity-60">
            Enter to send · Embed pending transcripts before first search
          </p>
        </div>
      </div>
    </Shell>
  );
}
