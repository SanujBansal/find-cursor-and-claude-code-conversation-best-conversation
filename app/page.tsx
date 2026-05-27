"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Shell } from "@/components/Shell";
import {
  getDashboard,
  getSettings,
  importAll,
  refreshAnalytics,
} from "../src/lib/tauri";
import { useTauriRuntime } from "../src/lib/useTauriRuntime";
import type {
  ConversationWithScore,
  DashboardData,
  WeakRubric,
  WeeklyTrendPoint,
} from "../src/lib/types";

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmt(value: number | null | undefined, digits = 2): string {
  return value != null ? value.toFixed(digits) : "—";
}

function deltaLabel(delta: number | null | undefined): string {
  if (delta == null) return "";
  const sign = delta >= 0 ? "+" : "";
  return `${sign}${delta.toFixed(2)} vs prev`;
}

function deltaPositive(delta: number | null | undefined): boolean {
  return delta != null && delta >= 0;
}

function sourceLabel(provider: string): string {
  const map: Record<string, string> = {
    "cursor-local": "Cursor",
    "claude-code-local": "Claude Code",
    "claude-web-markdown": "Claude Web",
  };
  return map[provider] ?? provider;
}

function sourceBadgeClass(provider: string): string {
  const map: Record<string, string> = {
    "cursor-local": "bg-violet-50 text-violet-700 ring-violet-200",
    "claude-code-local": "bg-orange-50 text-orange-700 ring-orange-200",
    "claude-web-markdown": "bg-sky-50 text-sky-700 ring-sky-200",
  };
  return map[provider] ?? "bg-gray-50 text-gray-600 ring-gray-200";
}

function shortDate(iso: string | null | undefined): string {
  if (!iso) return "";
  return new Date(iso).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

function scoreColor(score: number): string {
  if (score >= 4) return "text-emerald-600";
  if (score >= 2.5) return "text-amber-600";
  return "text-red-500";
}

function scoreBarColor(score: number): string {
  if (score >= 4) return "bg-emerald-500";
  if (score >= 2.5) return "bg-amber-400";
  return "bg-red-400";
}

// ── Stat card ─────────────────────────────────────────────────────────────────

function StatCard({
  label,
  value,
  delta,
  sub,
  loading,
  icon,
  colorAsScore = true,
}: {
  label: string;
  value: number | null;
  delta?: number | null;
  sub?: string;
  loading?: boolean;
  icon?: React.ReactNode;
  colorAsScore?: boolean;
}) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-panel-border bg-panel p-5 shadow-sm">
      <div className="flex items-center justify-between">
        <p className="text-sm font-medium text-muted">{label}</p>
        {icon && (
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-accent-light text-accent">
            {icon}
          </div>
        )}
      </div>
      {loading ? (
        <div className="h-9 w-24 animate-pulse rounded-lg bg-muted-light" />
      ) : (
        <p
          className={`text-3xl font-bold tabular-nums ${
            value != null
              ? colorAsScore
                ? scoreColor(value)
                : "text-foreground"
              : "text-muted"
          }`}
        >
          {fmt(value)}
        </p>
      )}
      {delta != null && !loading && (
        <span
          className={[
            "inline-flex w-fit items-center rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset",
            deltaPositive(delta)
              ? "bg-emerald-50 text-emerald-700 ring-emerald-200"
              : "bg-red-50 text-red-700 ring-red-200",
          ].join(" ")}
        >
          {deltaPositive(delta) ? "↑" : "↓"} {deltaLabel(delta)}
        </span>
      )}
      {sub && !loading && <p className="text-xs text-muted">{sub}</p>}
    </div>
  );
}

// ── SVG Sparkline ─────────────────────────────────────────────────────────────

function SparklineChart({ data }: { data: WeeklyTrendPoint[] }) {
  if (data.length === 0) {
    return (
      <div className="flex h-40 items-center justify-center">
        <p className="text-sm text-muted">No weekly data yet — score conversations first.</p>
      </div>
    );
  }

  const W = 480;
  const H = 140;
  const padX = 32;
  const padY = 12;
  const chartW = W - 2 * padX;
  const chartH = H - padY - 28;

  const toX = (i: number) =>
    data.length > 1 ? padX + (i * chartW) / (data.length - 1) : padX + chartW / 2;
  const toY = (score: number) => padY + chartH - (score / 5) * chartH;

  const points = data.map((d, i) => ({ x: toX(i), y: toY(d.score), ...d }));
  const pathD = points
    .map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(1)},${p.y.toFixed(1)}`)
    .join(" ");
  const areaD = `${pathD} L${points[points.length - 1].x.toFixed(1)},${(padY + chartH).toFixed(1)} L${points[0].x.toFixed(1)},${(padY + chartH).toFixed(1)} Z`;

  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full" preserveAspectRatio="xMidYMid meet">
      <defs>
        <linearGradient id="sparkGrad" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#6366f1" stopOpacity="0.15" />
          <stop offset="100%" stopColor="#6366f1" stopOpacity="0" />
        </linearGradient>
      </defs>

      {/* Grid lines */}
      {[1, 2, 3, 4, 5].map((score) => {
        const y = toY(score);
        return (
          <g key={score}>
            <line x1={padX} y1={y} x2={W - padX} y2={y} stroke="#e5e7eb" strokeWidth="1" />
            <text x={padX - 6} y={y + 3} textAnchor="end" fontSize="9" fill="#9ca3af">
              {score}
            </text>
          </g>
        );
      })}

      {/* Area fill */}
      <path d={areaD} fill="url(#sparkGrad)" />

      {/* Line */}
      <path
        d={pathD}
        fill="none"
        stroke="#6366f1"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />

      {/* Data points */}
      {points.map((p, i) => (
        <g key={i}>
          <circle cx={p.x} cy={p.y} r="4" fill="white" stroke="#6366f1" strokeWidth="2" />
          <title>{`${p.weekLabel}: ${p.score.toFixed(2)}`}</title>
        </g>
      ))}

      {/* X-axis labels */}
      {points.map((p, i) => (
        <text key={i} x={p.x} y={H - 6} textAnchor="middle" fontSize="9" fill="#9ca3af">
          {p.weekLabel.slice(5)}
        </text>
      ))}
    </svg>
  );
}

// ── Weakest rubric list ───────────────────────────────────────────────────────

function WeakRubricList({ rubrics, loading }: { rubrics: WeakRubric[]; loading: boolean }) {
  if (loading) {
    return (
      <div className="flex flex-col gap-2">
        {[0, 1, 2].map((i) => (
          <div key={i} className="h-12 animate-pulse rounded-lg bg-muted-light" />
        ))}
      </div>
    );
  }
  if (rubrics.length === 0) {
    return <p className="text-sm text-muted">No rubric data yet — score some conversations first.</p>;
  }
  return (
    <div className="flex flex-col gap-2.5">
      {rubrics.map((r) => (
        <div key={r.dimension} className="rounded-lg border border-panel-border bg-background p-3">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium text-foreground">{r.label}</span>
            <span className="text-sm font-bold tabular-nums text-amber-600">
              {r.averageScore.toFixed(2)}
            </span>
          </div>
          <div className="mt-2 h-1.5 rounded-full bg-muted-light">
            <div
              className="h-1.5 rounded-full bg-amber-400 transition-all"
              style={{ width: `${(r.averageScore / 5) * 100}%` }}
            />
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Conversation card ─────────────────────────────────────────────────────────

function ConversationCard({
  conv,
  onClick,
}: {
  conv: ConversationWithScore;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex flex-col gap-3 rounded-xl border border-panel-border bg-panel p-4 text-left shadow-sm transition-all hover:border-accent/40 hover:shadow-md"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-semibold text-foreground">{conv.title}</p>
          {conv.projectName && (
            <p className="mt-0.5 truncate text-xs font-medium text-foreground/80">
              {conv.projectName}
            </p>
          )}
          {conv.sourcePath && (
            <p className="mt-0.5 truncate text-xs text-muted">{conv.sourcePath}</p>
          )}
        </div>
        {conv.completedAt && (
          <span className="shrink-0 text-xs text-muted">{shortDate(conv.completedAt)}</span>
        )}
      </div>

      <div className="flex items-center gap-2">
        <span
          className={[
            "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset",
            sourceBadgeClass(conv.provider),
          ].join(" ")}
        >
          {sourceLabel(conv.provider)}
        </span>
      </div>

      {conv.finalScore != null && (
        <div className="flex items-center gap-2.5">
          <div className="h-1.5 flex-1 rounded-full bg-muted-light">
            <div
              className={`h-1.5 rounded-full transition-all ${scoreBarColor(conv.finalScore)}`}
              style={{ width: `${(conv.finalScore / 5) * 100}%` }}
            />
          </div>
          <span className={`text-sm font-bold tabular-nums ${scoreColor(conv.finalScore)}`}>
            {conv.finalScore.toFixed(2)}
          </span>
        </div>
      )}
    </button>
  );
}

// ── Buttons ───────────────────────────────────────────────────────────────────

function PrimaryBtn({
  label,
  busy,
  disabled,
  onClick,
}: {
  label: string;
  busy?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy || disabled}
      className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-4 py-2 text-sm font-medium text-white shadow-sm transition-all hover:bg-accent/90 disabled:cursor-not-allowed disabled:opacity-40"
    >
      {busy && (
        <svg className="h-3.5 w-3.5 animate-spin" viewBox="0 0 24 24" fill="none">
          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
        </svg>
      )}
      {busy ? `${label}…` : label}
    </button>
  );
}

function GhostBtn({
  label,
  busy,
  disabled,
  onClick,
}: {
  label: string;
  busy?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy || disabled}
      className="inline-flex items-center gap-1.5 rounded-lg border border-panel-border bg-panel px-3 py-2 text-sm font-medium text-foreground shadow-sm transition-all hover:bg-muted-light disabled:cursor-not-allowed disabled:opacity-40"
    >
      {busy ? `${label}…` : label}
    </button>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function DashboardPage() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const router = useRouter();
  const isRuntime = useTauriRuntime();

  async function load() {
    setLoading(true);
    setError(null);
    try {
      setData(await getDashboard());
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      if (!isRuntime) {
        if (!cancelled) setLoading(false);
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const dashboard = await getDashboard();
        if (!cancelled) setData(dashboard);
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [isRuntime]);

  async function handleRefreshAnalytics() {
    setBusy("refresh");
    try {
      await refreshAnalytics();
      await load();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  async function handleImportAll() {
    setBusy("import-all");
    try {
      const settings = await getSettings();
      await importAll({
        cursorDataPath: settings.cursorDataPath || undefined,
        claudeCodePath: settings.claudeCodePath || undefined,
        claudeMarkdownPath: settings.claudeMarkdownPath || undefined,
        clearExisting: true,
      });
      await load();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  const d = data;

  return (
    <Shell
      title="Dashboard"
      subtitle="Your vibe coding score, trends, and top sessions"
      actions={
        <>
          <PrimaryBtn
            label="Clear & Import All"
            busy={busy === "import-all"}
            disabled={!isRuntime}
            onClick={handleImportAll}
          />
          <GhostBtn
            label="Refresh Analytics"
            busy={busy === "refresh"}
            disabled={!isRuntime}
            onClick={handleRefreshAnalytics}
          />
        </>
      }
    >
      <div className="flex flex-col gap-6">
        {/* Error banner */}
        {error && (
          <div className="flex items-start gap-3 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            <svg className="mt-0.5 h-4 w-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z" />
            </svg>
            <span><strong>Error:</strong> {error}</span>
          </div>
        )}

        {isRuntime === false && (
          <div className="flex items-center gap-3 rounded-xl border border-blue-200 bg-blue-50 px-4 py-3 text-sm text-blue-700">
            <svg className="h-4 w-4 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M11.25 11.25l.041-.02a.75.75 0 011.063.852l-.708 2.836a.75.75 0 001.063.853l.041-.021M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9-3.75h.008v.008H12V8.25z" />
            </svg>
            Launch the desktop app to see live data.
          </div>
        )}

        {/* Row 1: Stat cards */}
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          <StatCard
            label="Today's Score"
            value={d?.todayScore ?? null}
            delta={d?.dailyDelta}
            loading={loading}
            icon={
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z" />
              </svg>
            }
          />
          <StatCard
            label="This Week"
            value={d?.weekScore ?? null}
            delta={d?.weeklyDelta}
            loading={loading}
            icon={
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5" />
              </svg>
            }
          />
          <StatCard
            label="7-Day Average"
            value={d?.rolling7d ?? null}
            loading={loading}
            icon={
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M7.5 14.25v2.25m3-4.5v4.5m3-6.75v6.75m3-9v9M6 20.25h12A2.25 2.25 0 0020.25 18V6A2.25 2.25 0 0018 3.75H6A2.25 2.25 0 003.75 6v12A2.25 2.25 0 006 20.25z" />
              </svg>
            }
          />
          <StatCard
            label="Conversations"
            value={d != null ? d.totalConversations : null}
            sub={d != null ? `${d.totalScored} scored` : undefined}
            loading={loading}
            colorAsScore={false}
            icon={
              <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M20.25 8.511c.884.284 1.5 1.128 1.5 2.097v4.286c0 1.136-.847 2.1-1.98 2.193-.34.027-.68.052-1.02.072v3.091l-3-3c-1.354 0-2.694-.055-4.02-.163a2.115 2.115 0 01-.825-.242m9.345-8.334a2.126 2.126 0 00-.476-.095 48.64 48.64 0 00-8.048 0c-1.131.094-1.976 1.057-1.976 2.192v4.286c0 .837.46 1.58 1.155 1.951m9.345-8.334V6.637c0-1.621-1.152-3.026-2.76-3.235A48.455 48.455 0 0011.25 3c-2.115 0-4.198.137-6.24.402-1.608.209-2.76 1.614-2.76 3.235v6.226c0 1.621 1.152 3.026 2.76 3.235.577.075 1.157.14 1.74.194V21l4.155-4.155" />
              </svg>
            }
          />
        </div>

        {/* Row 2: Chart + Weakest rubrics */}
        <div className="flex gap-4">
          {/* Chart */}
          <div className="flex min-w-0 flex-[3] flex-col rounded-xl border border-panel-border bg-panel p-5 shadow-sm">
            <div className="mb-4 flex items-center justify-between">
              <h3 className="text-sm font-semibold text-foreground">Weekly Trend</h3>
              <span className="text-xs text-muted">Last 8 weeks</span>
            </div>
            {loading ? (
              <div className="h-36 animate-pulse rounded-lg bg-muted-light" />
            ) : (
              <SparklineChart data={d?.weeklyTrend ?? []} />
            )}
          </div>

          {/* Weakest rubrics */}
          <div className="flex w-72 shrink-0 flex-col gap-4 rounded-xl border border-panel-border bg-panel p-5 shadow-sm">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold text-foreground">Weak Areas</h3>
              <GhostBtn
                label="Refresh"
                busy={busy === "refresh"}
                disabled={!isRuntime}
                onClick={handleRefreshAnalytics}
              />
            </div>
            <WeakRubricList rubrics={d?.weakestRubrics ?? []} loading={loading} />
          </div>
        </div>

        {/* Row 3: Top conversations */}
        <div className="flex flex-col gap-4 rounded-xl border border-panel-border bg-panel p-5 shadow-sm">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-foreground">Top Conversations</h3>
            <span className="text-xs text-muted">By score</span>
          </div>

          {loading ? (
            <div className="grid gap-3 lg:grid-cols-3">
              {[0, 1, 2].map((i) => (
                <div key={i} className="h-28 animate-pulse rounded-xl bg-muted-light" />
              ))}
            </div>
          ) : !d?.topConversations.length ? (
            <div className="flex flex-col items-center gap-3 py-10">
              <svg className="h-10 w-10 text-muted" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.25}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M8.625 12a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0H8.25m4.125 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0H12m4.125 0a.375.375 0 11-.75 0 .375.375 0 01.75 0zm0 0h-.375M21 12c0 4.556-4.03 8.25-9 8.25a9.764 9.764 0 01-2.555-.337A5.972 5.972 0 015.41 20.97a5.969 5.969 0 01-.474-.065 4.48 4.48 0 00.978-2.025c.09-.457-.133-.901-.467-1.226C3.93 16.178 3 14.189 3 12c0-4.556 4.03-8.25 9-8.25s9 3.694 9 8.25z" />
              </svg>
              <p className="text-sm text-muted">No conversations yet — import some to get started.</p>
            </div>
          ) : (
            <div className="grid gap-3 lg:grid-cols-3">
              {(d?.topConversations ?? []).map((conv) => (
                <ConversationCard
                  key={conv.id}
                  conv={conv}
                  onClick={() => router.push(`/conversations/detail?id=${conv.id}`)}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </Shell>
  );
}
