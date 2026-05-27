"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Shell } from "@/components/Shell";
import {
  getProjectTopConversations,
  getSettings,
  listConversations,
  listProjects,
  scoreProject,
} from "../../src/lib/tauri";
import { useTauriRuntime } from "../../src/lib/useTauriRuntime";
import type {
  ConversationSummary,
  ConversationWithScore,
  ProjectGroup,
} from "../../src/lib/types";
import { hasAiCredentials, getActiveApiKey } from "../../src/lib/types";

function sourceLabel(provider: string): string {
  const map: Record<string, string> = {
    "cursor-local": "cursor",
    "claude-code-local": "claude-code",
    "claude-web-markdown": "markdown",
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

function scoreColor(score: number | null): string {
  if (score == null) return "text-muted";
  if (score >= 4) return "text-emerald-600";
  if (score >= 2.5) return "text-amber-600";
  return "text-red-500";
}

function shortDate(iso: string | null): string {
  if (!iso) return "—";
  return iso.slice(0, 10);
}

type SortKey = "score" | "date" | "title" | "user";
type SortDir = "asc" | "desc";
type ScoreFilter = "all" | "scored" | "unscored";

interface ConversationFilters {
  scoreStatus: ScoreFilter;
  minScore: string;
  maxScore: string;
  minUserMsgs: string;
  maxUserMsgs: string;
  dateFrom: string;
  dateTo: string;
}

const DEFAULT_FILTERS: ConversationFilters = {
  scoreStatus: "all",
  minScore: "",
  maxScore: "",
  minUserMsgs: "",
  maxUserMsgs: "",
  dateFrom: "",
  dateTo: "",
};

function parseOptionalNumber(value: string, min: number, max: number): number | null {
  if (value.trim() === "") return null;
  const n = Number.parseFloat(value);
  if (Number.isNaN(n)) return null;
  return Math.min(max, Math.max(min, n));
}

function hasActiveFilters(filters: ConversationFilters): boolean {
  return (
    filters.scoreStatus !== "all" ||
    filters.minScore !== "" ||
    filters.maxScore !== "" ||
    filters.minUserMsgs !== "" ||
    filters.maxUserMsgs !== "" ||
    filters.dateFrom !== "" ||
    filters.dateTo !== ""
  );
}

function compareConversations(
  a: ConversationSummary,
  b: ConversationSummary,
  sortKey: SortKey,
  sortDir: SortDir,
): number {
  let cmp = 0;
  if (sortKey === "score") {
    cmp = (a.finalScore ?? -1) - (b.finalScore ?? -1);
  } else if (sortKey === "date") {
    cmp = (a.completedAt ?? "").localeCompare(b.completedAt ?? "");
  } else if (sortKey === "user") {
    cmp = a.userMessageCount - b.userMessageCount;
  } else {
    cmp = a.title.localeCompare(b.title);
  }
  return sortDir === "asc" ? cmp : -cmp;
}

function TopConversationCard({
  rank,
  conv,
  onClick,
}: {
  rank: number;
  conv: ConversationWithScore;
  onClick: () => void;
}) {
  const medals = ["🥇", "🥈", "🥉"];

  return (
    <button
      type="button"
      onClick={onClick}
      className="flex flex-1 min-w-[180px] flex-col gap-1 rounded-xl border border-border bg-background p-4 text-left transition-shadow hover:shadow-md"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-lg">{medals[rank - 1] ?? `#${rank}`}</span>
        <span
          className={`text-xl font-bold tabular-nums ${scoreColor(conv.finalScore)}`}
        >
          {conv.finalScore?.toFixed(2) ?? "—"}
        </span>
      </div>
      <p className="line-clamp-2 text-sm font-medium text-foreground">{conv.title}</p>
      <p className="text-[10px] text-muted">{shortDate(conv.completedAt)}</p>
    </button>
  );
}

function ConversationRow({
  conv,
  onClick,
}: {
  conv: ConversationSummary;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group flex w-full items-center gap-3 border-b border-border px-4 py-3 text-left transition-colors hover:bg-muted/40"
    >
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-foreground group-hover:text-accent">
          {conv.title}
        </p>
        <p className="truncate text-xs text-muted">{sourceLabel(conv.provider)}</p>
      </div>

      <span
        className={`shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium ring-1 ring-inset ${sourceBadgeClass(conv.provider)}`}
      >
        {sourceLabel(conv.provider)}
      </span>

      <div className="flex w-20 shrink-0 items-center gap-2">
        {conv.finalScore != null ? (
          <>
            <div className="h-1 flex-1 rounded-full bg-muted">
              <div
                className="h-1 rounded-full bg-accent"
                style={{ width: `${(conv.finalScore / 5) * 100}%` }}
              />
            </div>
            <span
              className={`w-8 text-right text-xs font-semibold tabular-nums ${scoreColor(conv.finalScore)}`}
            >
              {conv.finalScore.toFixed(1)}
            </span>
          </>
        ) : (
          <span className="w-full text-right text-[10px] text-muted">unscored</span>
        )}
      </div>

      <span className="w-20 shrink-0 text-right text-[10px] tabular-nums text-muted">
        {shortDate(conv.completedAt)}
      </span>

      <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-muted">
        {conv.userMessageCount}
      </span>
    </button>
  );
}

export default function ConversationsPage() {
  const [projects, setProjects] = useState<ProjectGroup[]>([]);
  const [convs, setConvs] = useState<ConversationSummary[]>([]);
  const [topThree, setTopThree] = useState<ConversationWithScore[]>([]);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [scoring, setScoring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [projectSearch, setProjectSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("date");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [filters, setFilters] = useState<ConversationFilters>(DEFAULT_FILTERS);
  const [scoreMenuOpen, setScoreMenuOpen] = useState(false);
  const [requireMinUserMsgs, setRequireMinUserMsgs] = useState(true);
  const [minUserMessagesInput, setMinUserMessagesInput] = useState("10");
  const minUserMessages = useMemo(() => {
    const n = Number.parseInt(minUserMessagesInput, 10);
    if (Number.isNaN(n)) return 0;
    return Math.min(999, Math.max(0, n));
  }, [minUserMessagesInput]);
  const scoreMenuRef = useRef<HTMLDivElement>(null);
  const router = useRouter();
  const isRuntime = useTauriRuntime();

  const reload = useCallback(async () => {
    const [projectData, convData] = await Promise.all([
      listProjects(),
      listConversations(),
    ]);
    setProjects(projectData);
    setConvs(convData);
    setSelectedProject((current) => current ?? projectData[0]?.projectPath ?? null);
    return { projectData, convData };
  }, []);

  const loadTopThree = useCallback(async (projectPath: string) => {
    try {
      const top = await getProjectTopConversations(projectPath, 3);
      setTopThree(top);
    } catch {
      setTopThree([]);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      if (!isRuntime) {
        if (!cancelled) setLoading(false);
        return;
      }
      try {
        await reload();
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [isRuntime, reload]);

  useEffect(() => {
    if (!isRuntime || !selectedProject) {
      return;
    }
    let cancelled = false;
    void (async () => {
      if (cancelled) return;
      await loadTopThree(selectedProject);
    })();
    return () => {
      cancelled = true;
    };
  }, [isRuntime, selectedProject, convs, loadTopThree]);

  // Derive the displayed Top-3 from the active selection so that stale
  // results from a previously-selected project never leak through.
  const displayedTopThree =
    selectedProject &&
    topThree.length > 0 &&
    topThree.every((c) => (c.sourcePath ?? "Unassigned") === selectedProject)
      ? topThree
      : [];

  const filteredProjects = useMemo(() => {
    if (!projectSearch) return projects;
    const q = projectSearch.toLowerCase();
    return projects.filter(
      (p) =>
        p.projectName.toLowerCase().includes(q) ||
        p.projectPath.toLowerCase().includes(q),
    );
  }, [projects, projectSearch]);

  const projectConversationsAll = useMemo(() => {
    if (!selectedProject) return [];
    return convs.filter((c) => (c.sourcePath ?? "Unassigned") === selectedProject);
  }, [convs, selectedProject]);

  const projectConversations = useMemo(() => {
    if (!selectedProject) return [];

    let result = projectConversationsAll;

    if (search) {
      const q = search.toLowerCase();
      result = result.filter(
        (c) =>
          c.title.toLowerCase().includes(q) ||
          c.provider.toLowerCase().includes(q),
      );
    }

    const minScore = parseOptionalNumber(filters.minScore, 0, 5);
    const maxScore = parseOptionalNumber(filters.maxScore, 0, 5);
    const minUserMsgs = parseOptionalNumber(filters.minUserMsgs, 0, 9999);
    const maxUserMsgs = parseOptionalNumber(filters.maxUserMsgs, 0, 9999);

    if (filters.scoreStatus === "scored") {
      result = result.filter((c) => c.finalScore != null);
    } else if (filters.scoreStatus === "unscored") {
      result = result.filter((c) => c.finalScore == null);
    }

    if (minScore != null || maxScore != null) {
      result = result.filter((c) => {
        if (c.finalScore == null) return false;
        if (minScore != null && c.finalScore < minScore) return false;
        if (maxScore != null && c.finalScore > maxScore) return false;
        return true;
      });
    }

    if (minUserMsgs != null) {
      result = result.filter((c) => c.userMessageCount >= minUserMsgs);
    }
    if (maxUserMsgs != null) {
      result = result.filter((c) => c.userMessageCount <= maxUserMsgs);
    }

    if (filters.dateFrom) {
      result = result.filter(
        (c) => c.completedAt != null && c.completedAt.slice(0, 10) >= filters.dateFrom,
      );
    }
    if (filters.dateTo) {
      result = result.filter(
        (c) => c.completedAt != null && c.completedAt.slice(0, 10) <= filters.dateTo,
      );
    }

    result = [...result].sort((a, b) =>
      compareConversations(a, b, sortKey, sortDir),
    );

    return result;
  }, [
    selectedProject,
    projectConversationsAll,
    search,
    sortKey,
    sortDir,
    filters,
  ]);

  const filtersActive = hasActiveFilters(filters);

  function handleSortClick(key: SortKey) {
    if (sortKey === key) {
      setSortDir((dir) => (dir === "asc" ? "desc" : "asc"));
      return;
    }
    setSortKey(key);
    setSortDir(key === "title" ? "asc" : "desc");
  }

  function updateFilter<K extends keyof ConversationFilters>(
    key: K,
    value: ConversationFilters[K],
  ) {
    setFilters((current) => ({ ...current, [key]: value }));
  }

  const selectedProjectMeta = projects.find((p) => p.projectPath === selectedProject);
  const unscoredInProject = useMemo(() => {
    if (!selectedProject) return [];
    return convs.filter(
      (c) =>
        (c.sourcePath ?? "Unassigned") === selectedProject && c.finalScore == null,
    );
  }, [convs, selectedProject]);

  const unscoredCount = unscoredInProject.length;

  const eligibleUnscoredCount = useMemo(() => {
    if (!requireMinUserMsgs) return unscoredCount;
    return unscoredInProject.filter((c) => c.userMessageCount > minUserMessages)
      .length;
  }, [unscoredInProject, unscoredCount, requireMinUserMsgs, minUserMessages]);

  useEffect(() => {
    if (!scoreMenuOpen) return;
    function handlePointerDown(event: MouseEvent) {
      if (
        scoreMenuRef.current &&
        !scoreMenuRef.current.contains(event.target as Node)
      ) {
        setScoreMenuOpen(false);
      }
    }
    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  }, [scoreMenuOpen]);

  async function handleScoreProject() {
    if (!selectedProject || !isRuntime || eligibleUnscoredCount === 0) return;
    setScoring(true);
    setError(null);
    setScoreMenuOpen(false);
    try {
      const settings = await getSettings();
      if (!hasAiCredentials(settings)) {
        setError("Configure OpenAI or Azure credentials in Settings first.");
        return;
      }
      await scoreProject(
        getActiveApiKey(settings),
        selectedProject,
        settings.scoringModel,
        requireMinUserMsgs ? minUserMessages : null,
      );
      await reload();
      await loadTopThree(selectedProject);
    } catch (err) {
      setError(String(err));
    } finally {
      setScoring(false);
    }
  }

  return (
    <Shell
      title="Conversations"
      subtitle="Browse transcripts grouped by project folder."
    >
      <div className="flex flex-col gap-4">
        {error && (
          <div className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        )}

        {isRuntime === false && (
          <div className="rounded-lg border border-border bg-panel px-4 py-3 text-sm text-muted">
            Tauri runtime not detected — launch the desktop app to see conversations.
          </div>
        )}

        <div className="flex min-h-[520px] overflow-hidden rounded-xl border border-border bg-panel shadow-sm">
          <aside className="w-72 shrink-0 border-r border-border bg-background">
            <div className="border-b border-border px-4 py-3">
              <p className="text-xs font-semibold uppercase tracking-wider text-muted">
                Projects
              </p>
              <p className="mt-1 text-xs text-muted">
                {projectSearch
                  ? `${filteredProjects.length} of ${projects.length} folders`
                  : `${projects.length} folder${projects.length === 1 ? "" : "s"}`}
              </p>
              <input
                type="search"
                placeholder="Search projects…"
                value={projectSearch}
                onChange={(e) => setProjectSearch(e.target.value)}
                className="mt-2 w-full rounded-lg border border-border bg-background px-3 py-1.5 text-xs focus:border-accent focus:outline-none"
              />
            </div>
            <div className="max-h-[560px] overflow-y-auto">
              {loading ? (
                <div className="space-y-2 p-3">
                  {[0, 1, 2, 3].map((i) => (
                    <div key={i} className="h-12 animate-pulse rounded-lg bg-muted" />
                  ))}
                </div>
              ) : projects.length === 0 ? (
                <p className="px-4 py-8 text-xs text-muted">
                  No projects yet. Import transcripts from Settings.
                </p>
              ) : filteredProjects.length === 0 ? (
                <p className="px-4 py-8 text-xs text-muted">
                  No projects matching &ldquo;{projectSearch}&rdquo;
                </p>
              ) : (
                filteredProjects.map((project) => {
                  const active = selectedProject === project.projectPath;
                  return (
                    <button
                      key={project.projectPath}
                      type="button"
                      onClick={() => setSelectedProject(project.projectPath)}
                      className={[
                        "flex w-full flex-col gap-0.5 border-b border-border px-4 py-3 text-left transition-colors",
                        active
                          ? "bg-accent/10 border-l-2 border-l-accent"
                          : "hover:bg-muted/50 border-l-2 border-l-transparent",
                      ].join(" ")}
                    >
                      <span className="truncate text-sm font-medium text-foreground">
                        {project.projectName}
                      </span>
                      {project.projectPath !== "Unassigned" && (
                        <span className="truncate text-[10px] text-muted">
                          {project.projectPath}
                        </span>
                      )}
                      <span className="text-[10px] text-muted">
                        {project.conversationCount} chats · {project.scoredCount} scored
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </aside>

          <div className="flex min-w-0 flex-1 flex-col">
            <div className="border-b border-border px-4 py-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h2 className="text-sm font-semibold text-foreground">
                    {selectedProjectMeta?.projectName ?? "Select a project"}
                  </h2>
                  {selectedProjectMeta && (
                    <p className="text-xs text-muted">
                      {projectConversations.length === projectConversationsAll.length
                        ? `${projectConversationsAll.length} conversations`
                        : `${projectConversations.length} of ${projectConversationsAll.length} conversations`}
                      {unscoredCount > 0 ? ` · ${unscoredCount} unscored` : ""}
                    </p>
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  {selectedProject && (
                    <div ref={scoreMenuRef} className="relative">
                      <button
                        type="button"
                        disabled={scoring || !isRuntime || unscoredCount === 0}
                        onClick={() => setScoreMenuOpen((open) => !open)}
                        className="rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent/90 disabled:opacity-50"
                      >
                        {scoring
                          ? "Scoring…"
                          : unscoredCount === 0
                            ? "All scored"
                            : `Score (${eligibleUnscoredCount})`}
                      </button>
                      {scoreMenuOpen && unscoredCount > 0 && (
                        <div className="absolute right-0 top-full z-20 mt-1 w-72 rounded-xl border border-border bg-background p-3 shadow-lg">
                          <p className="mb-2 text-xs font-semibold text-foreground">
                            Scoring filters
                          </p>
                          <label className="flex cursor-pointer items-start gap-2">
                            <input
                              type="checkbox"
                              checked={requireMinUserMsgs}
                              onChange={(e) =>
                                setRequireMinUserMsgs(e.target.checked)
                              }
                              className="mt-0.5"
                            />
                            <span className="text-xs text-foreground">
                              More than{" "}
                              <input
                                type="text"
                                inputMode="numeric"
                                value={minUserMessagesInput}
                                disabled={!requireMinUserMsgs}
                                onChange={(e) => {
                                  const v = e.target.value;
                                  if (v === "" || /^\d+$/.test(v)) {
                                    setMinUserMessagesInput(v.slice(0, 3));
                                  }
                                }}
                                onBlur={() => {
                                  if (minUserMessagesInput === "") {
                                    setMinUserMessagesInput("0");
                                  }
                                }}
                                className="mx-1 w-12 rounded border border-border bg-background px-1 py-0.5 text-center tabular-nums disabled:opacity-50"
                              />{" "}
                              user messages
                            </span>
                          </label>
                          <p className="mt-2 text-[10px] text-muted">
                            {eligibleUnscoredCount} of {unscoredCount} unscored
                            conversations match
                          </p>
                          <button
                            type="button"
                            disabled={
                              scoring ||
                              !isRuntime ||
                              eligibleUnscoredCount === 0
                            }
                            onClick={handleScoreProject}
                            className="mt-3 w-full rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent/90 disabled:opacity-50"
                          >
                            {eligibleUnscoredCount === 0
                              ? "No conversations match"
                              : `Score ${eligibleUnscoredCount} conversation${eligibleUnscoredCount === 1 ? "" : "s"}`}
                          </button>
                        </div>
                      )}
                    </div>
                  )}
                  <input
                    type="search"
                    placeholder="Search in project…"
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    className="rounded-lg border border-border bg-background px-3 py-1.5 text-xs focus:border-accent focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={() => setFiltersOpen((open) => !open)}
                    className={[
                      "rounded-lg border px-2.5 py-1 text-[10px] uppercase tracking-wide",
                      filtersOpen || filtersActive
                        ? "border-accent/30 bg-accent/10 text-accent"
                        : "border-border text-muted hover:text-foreground",
                    ].join(" ")}
                  >
                    Filters{filtersActive ? " ·" : ""}
                  </button>
                  {(["date", "score", "title", "user"] as SortKey[]).map((k) => (
                    <button
                      key={k}
                      type="button"
                      onClick={() => handleSortClick(k)}
                      className={[
                        "rounded-lg px-2.5 py-1 text-[10px] uppercase tracking-wide",
                        sortKey === k
                          ? "bg-accent/10 text-accent"
                          : "text-muted hover:text-foreground",
                      ].join(" ")}
                    >
                      {k}
                      {sortKey === k ? (sortDir === "asc" ? " ↑" : " ↓") : ""}
                    </button>
                  ))}
                </div>
              </div>

              {filtersOpen && selectedProject && (
                <div className="mt-3 flex flex-wrap items-end gap-4 rounded-lg border border-border bg-background p-3">
                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-muted">
                      Score
                    </span>
                    <div className="flex items-center gap-2">
                      <select
                        value={filters.scoreStatus}
                        onChange={(e) =>
                          updateFilter("scoreStatus", e.target.value as ScoreFilter)
                        }
                        className="rounded-lg border border-border bg-background px-2 py-1 text-xs focus:border-accent focus:outline-none"
                      >
                        <option value="all">All</option>
                        <option value="scored">Scored only</option>
                        <option value="unscored">Unscored only</option>
                      </select>
                      <input
                        type="text"
                        inputMode="decimal"
                        placeholder="Min"
                        value={filters.minScore}
                        onChange={(e) => {
                          const v = e.target.value;
                          if (v === "" || /^\d*\.?\d*$/.test(v)) {
                            updateFilter("minScore", v);
                          }
                        }}
                        className="w-14 rounded-lg border border-border bg-background px-2 py-1 text-xs tabular-nums focus:border-accent focus:outline-none"
                      />
                      <span className="text-xs text-muted">–</span>
                      <input
                        type="text"
                        inputMode="decimal"
                        placeholder="Max"
                        value={filters.maxScore}
                        onChange={(e) => {
                          const v = e.target.value;
                          if (v === "" || /^\d*\.?\d*$/.test(v)) {
                            updateFilter("maxScore", v);
                          }
                        }}
                        className="w-14 rounded-lg border border-border bg-background px-2 py-1 text-xs tabular-nums focus:border-accent focus:outline-none"
                      />
                    </div>
                  </div>

                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-muted">
                      User messages
                    </span>
                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        inputMode="numeric"
                        placeholder="Min"
                        value={filters.minUserMsgs}
                        onChange={(e) => {
                          const v = e.target.value;
                          if (v === "" || /^\d+$/.test(v)) {
                            updateFilter("minUserMsgs", v);
                          }
                        }}
                        className="w-16 rounded-lg border border-border bg-background px-2 py-1 text-xs tabular-nums focus:border-accent focus:outline-none"
                      />
                      <span className="text-xs text-muted">–</span>
                      <input
                        type="text"
                        inputMode="numeric"
                        placeholder="Max"
                        value={filters.maxUserMsgs}
                        onChange={(e) => {
                          const v = e.target.value;
                          if (v === "" || /^\d+$/.test(v)) {
                            updateFilter("maxUserMsgs", v);
                          }
                        }}
                        className="w-16 rounded-lg border border-border bg-background px-2 py-1 text-xs tabular-nums focus:border-accent focus:outline-none"
                      />
                    </div>
                  </div>

                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] font-semibold uppercase tracking-wider text-muted">
                      Date
                    </span>
                    <div className="flex items-center gap-2">
                      <input
                        type="date"
                        value={filters.dateFrom}
                        onChange={(e) => updateFilter("dateFrom", e.target.value)}
                        className="rounded-lg border border-border bg-background px-2 py-1 text-xs focus:border-accent focus:outline-none"
                      />
                      <span className="text-xs text-muted">–</span>
                      <input
                        type="date"
                        value={filters.dateTo}
                        onChange={(e) => updateFilter("dateTo", e.target.value)}
                        className="rounded-lg border border-border bg-background px-2 py-1 text-xs focus:border-accent focus:outline-none"
                      />
                    </div>
                  </div>

                  {filtersActive && (
                    <button
                      type="button"
                      onClick={() => setFilters(DEFAULT_FILTERS)}
                      className="rounded-lg px-2.5 py-1 text-xs text-muted hover:text-foreground"
                    >
                      Clear filters
                    </button>
                  )}
                </div>
              )}
            </div>

            {displayedTopThree.length > 0 && (
              <div className="border-b border-border bg-muted/20 px-4 py-4">
                <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted">
                  Top 3 in this folder
                </p>
                <div className="flex flex-wrap gap-3">
                  {displayedTopThree.map((conv, idx) => (
                    <TopConversationCard
                      key={conv.id}
                      rank={idx + 1}
                      conv={conv}
                      onClick={() =>
                        router.push(`/conversations/detail?id=${conv.id}`)
                      }
                    />
                  ))}
                </div>
              </div>
            )}

            <div className="flex items-center gap-3 border-b border-border px-4 py-2 text-[10px] uppercase tracking-wider text-muted">
              <span className="flex-1">Title</span>
              <span className="w-16 shrink-0">Source</span>
              <span className="w-20 shrink-0 text-right">Score</span>
              <span className="w-20 shrink-0 text-right">Date</span>
              <span className="w-10 shrink-0 text-right">User</span>
            </div>

            <div className="flex-1 overflow-y-auto">
              {loading ? (
                <div className="space-y-0">
                  {[0, 1, 2, 3, 4].map((i) => (
                    <div key={i} className="h-14 animate-pulse border-b border-border bg-muted/30" />
                  ))}
                </div>
              ) : !selectedProject ? (
                <p className="px-4 py-12 text-center text-sm text-muted">
                  Select a project folder to view conversations.
                </p>
              ) : projectConversations.length === 0 ? (
                <p className="px-4 py-12 text-center text-sm text-muted">
                  {search || filtersActive
                    ? "No conversations match your search or filters."
                    : "No conversations in this project."}
                </p>
              ) : (
                projectConversations.map((conv) => (
                  <ConversationRow
                    key={conv.id}
                    conv={conv}
                    onClick={() =>
                      router.push(`/conversations/detail?id=${conv.id}`)
                    }
                  />
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </Shell>
  );
}
