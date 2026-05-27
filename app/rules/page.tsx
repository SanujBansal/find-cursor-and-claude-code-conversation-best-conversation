"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { Shell } from "@/components/Shell";
import {
  getSettings,
  listProjects,
  scanProjectRules,
  scoreProjectRules,
} from "../../src/lib/tauri";
import { useTauriRuntime } from "../../src/lib/useTauriRuntime";
import type {
  ProjectGroup,
  ProjectRulesScore,
  ProjectRulesView,
  RuleFile,
  RuleKind,
  TechStack,
} from "../../src/lib/types";
import { hasAiCredentials, RULE_KIND_LABELS } from "../../src/lib/types";

// ── Helpers ───────────────────────────────────────────────────────────────────

function scoreColor(score: number | null | undefined): string {
  if (score == null) return "text-muted";
  if (score >= 4) return "text-emerald-600";
  if (score >= 2.5) return "text-amber-600";
  return "text-red-500";
}

function scoreBarColor(score: number | null | undefined): string {
  if (score == null) return "bg-muted-light";
  if (score >= 4) return "bg-emerald-500";
  if (score >= 2.5) return "bg-amber-400";
  return "bg-red-400";
}

function ruleKindBadgeClass(kind: RuleKind): string {
  const map: Record<RuleKind, string> = {
    agents: "bg-violet-50 text-violet-700 ring-violet-200",
    claude: "bg-orange-50 text-orange-700 ring-orange-200",
    gemini: "bg-sky-50 text-sky-700 ring-sky-200",
    "cursor-legacy": "bg-amber-50 text-amber-700 ring-amber-200",
    "cursor-rule": "bg-emerald-50 text-emerald-700 ring-emerald-200",
    windsurf: "bg-cyan-50 text-cyan-700 ring-cyan-200",
    copilot: "bg-slate-100 text-slate-700 ring-slate-200",
    aider: "bg-rose-50 text-rose-700 ring-rose-200",
    other: "bg-gray-50 text-gray-600 ring-gray-200",
  };
  return map[kind] ?? "bg-gray-50 text-gray-600 ring-gray-200";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

function formatScoredAt(iso: string | null | undefined): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// ── Small components ──────────────────────────────────────────────────────────

function TagPill({
  label,
  tone = "neutral",
}: {
  label: string;
  tone?: "neutral" | "accent" | "lang";
}) {
  const cls =
    tone === "accent"
      ? "bg-accent-light text-accent ring-accent/20"
      : tone === "lang"
        ? "bg-indigo-50 text-indigo-700 ring-indigo-200"
        : "bg-muted-light text-foreground/80 ring-border";
  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-[11px] font-medium ring-1 ring-inset ${cls}`}
    >
      {label}
    </span>
  );
}

function TechStackPanel({ stack }: { stack: TechStack }) {
  if (!stack.detected) {
    return (
      <p className="text-xs text-muted">
        Couldn&rsquo;t detect a tech stack — no manifest files (package.json,
        Cargo.toml, pyproject.toml, …) found in the project root.
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-2">
      {stack.languages.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-[10px] uppercase tracking-wider text-muted">
            Languages
          </span>
          {stack.languages.map((l) => (
            <TagPill key={l} label={l} tone="lang" />
          ))}
        </div>
      )}
      {stack.frameworks.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-[10px] uppercase tracking-wider text-muted">
            Frameworks
          </span>
          {stack.frameworks.map((f) => (
            <TagPill key={f} label={f} tone="accent" />
          ))}
        </div>
      )}
      {stack.tooling.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-[10px] uppercase tracking-wider text-muted">
            Tooling
          </span>
          {stack.tooling.map((t) => (
            <TagPill key={t} label={t} />
          ))}
        </div>
      )}
    </div>
  );
}

function DimensionBar({
  label,
  value,
  hint,
}: {
  label: string;
  value: number;
  hint?: string;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-baseline justify-between">
        <div>
          <p className="text-sm font-medium text-foreground">{label}</p>
          {hint && <p className="text-[11px] text-muted">{hint}</p>}
        </div>
        <span className={`text-base font-semibold tabular-nums ${scoreColor(value)}`}>
          {value.toFixed(1)}
        </span>
      </div>
      <div className="h-1.5 rounded-full bg-muted-light">
        <div
          className={`h-1.5 rounded-full transition-all ${scoreBarColor(value)}`}
          style={{ width: `${(value / 5) * 100}%` }}
        />
      </div>
    </div>
  );
}

function ScoreCard({ score }: { score: ProjectRulesScore }) {
  return (
    <div className="flex flex-col gap-4 rounded-xl border border-panel-border bg-panel p-5 shadow-sm">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-[11px] uppercase tracking-wider text-muted">
            Rules score
          </p>
          <p className={`mt-1 text-4xl font-bold tabular-nums ${scoreColor(score.overallScore)}`}>
            {score.overallScore.toFixed(2)}
          </p>
          <p className="mt-1 text-[11px] text-muted">
            {score.modelId} · rubric {score.rubricVersion} ·{" "}
            {formatScoredAt(score.scoredAt)}
          </p>
        </div>
        <div className="flex flex-col items-end gap-1">
          <span className="text-[10px] uppercase tracking-wider text-muted">
            Out of 5.00
          </span>
          <div className="h-2 w-24 rounded-full bg-muted-light">
            <div
              className={`h-2 rounded-full ${scoreBarColor(score.overallScore)}`}
              style={{ width: `${(score.overallScore / 5) * 100}%` }}
            />
          </div>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <DimensionBar
          label="Coverage"
          value={score.coverage}
          hint="All major surfaces addressed?"
        />
        <DimensionBar
          label="Stack alignment"
          value={score.stackAlignment}
          hint="Speaks to the actual frameworks?"
        />
        <DimensionBar
          label="Specificity"
          value={score.specificity}
          hint="Concrete vs. vague?"
        />
        <DimensionBar
          label="Actionability"
          value={score.actionability}
          hint="Can the agent actually follow it?"
        />
      </div>

      {score.summary && (
        <div className="rounded-lg border border-border bg-background p-4 text-sm leading-relaxed text-foreground/80">
          {score.summary}
        </div>
      )}

      {score.suggestions.length > 0 && (
        <div className="flex flex-col gap-2">
          <p className="text-[11px] uppercase tracking-wider text-muted">
            Suggestions
          </p>
          <ul className="flex flex-col gap-2">
            {score.suggestions.map((s, i) => (
              <li
                key={i}
                className="flex items-start gap-2 rounded-lg border border-border bg-background p-3 text-sm"
              >
                <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent-light text-[11px] font-semibold text-accent">
                  {i + 1}
                </span>
                <span className="text-foreground/85">{s}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function RuleFileCard({ file }: { file: RuleFile }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="overflow-hidden rounded-lg border border-border bg-background">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-muted/40"
      >
        <span
          className={`inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-[10px] font-medium ring-1 ring-inset ${ruleKindBadgeClass(file.kind)}`}
        >
          {RULE_KIND_LABELS[file.kind]}
        </span>
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">
          {file.relativePath}
        </span>
        <span className="shrink-0 text-[11px] text-muted">
          {formatBytes(file.bytes)}
          {file.truncated ? " · truncated" : ""}
        </span>
        <svg
          className={`h-4 w-4 shrink-0 text-muted transition-transform ${open ? "rotate-90" : ""}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={1.75}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" />
        </svg>
      </button>
      {open && (
        <pre className="max-h-96 overflow-auto border-t border-border bg-muted/30 p-4 text-[12px] leading-relaxed text-foreground whitespace-pre-wrap">
          {file.content || "(empty file)"}
        </pre>
      )}
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function ProjectRulesPage() {
  const [projects, setProjects] = useState<ProjectGroup[]>([]);
  const [projectSearch, setProjectSearch] = useState("");
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [view, setView] = useState<ProjectRulesView | null>(null);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [scoring, setScoring] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isRuntime = useTauriRuntime();

  const loadProjects = useCallback(async () => {
    const data = await listProjects();
    setProjects(data);
    setSelectedProject((current) => {
      if (current) return current;
      const firstReal = data.find((p) => p.projectPath !== "Unassigned");
      return firstReal?.projectPath ?? data[0]?.projectPath ?? null;
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (!isRuntime) {
        if (!cancelled) setLoading(false);
        return;
      }
      try {
        await loadProjects();
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isRuntime, loadProjects]);

  useEffect(() => {
    if (!isRuntime || !selectedProject) {
      return;
    }
    let cancelled = false;
    void (async () => {
      if (cancelled) return;
      setScanning(true);
      setError(null);
      try {
        const result = await scanProjectRules(selectedProject);
        if (!cancelled) setView(result);
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        if (!cancelled) setScanning(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isRuntime, selectedProject]);

  // Treat `view` as belonging only to the currently selected project.
  // This avoids a synchronous setView(null) inside the effect above and lets
  // React clear stale data via derivation on the next render.
  const activeView =
    view && view.report.projectPath === selectedProject ? view : null;

  const filteredProjects = useMemo(() => {
    if (!projectSearch) return projects;
    const q = projectSearch.toLowerCase();
    return projects.filter(
      (p) =>
        p.projectName.toLowerCase().includes(q) ||
        p.projectPath.toLowerCase().includes(q),
    );
  }, [projects, projectSearch]);

  async function handleScore() {
    if (!selectedProject || !activeView) return;
    setScoring(true);
    setError(null);
    try {
      const settings = await getSettings();
      if (!hasAiCredentials(settings)) {
        setError("Configure Azure OpenAI credentials in Settings first.");
        return;
      }
      const fresh = await scoreProjectRules(
        settings.openaiApiKey,
        selectedProject,
        settings.scoringModel,
      );
      setView({ ...activeView, score: fresh, stale: false });
    } catch (err) {
      setError(String(err));
    } finally {
      setScoring(false);
    }
  }

  async function handleRescan() {
    if (!selectedProject) return;
    setScanning(true);
    setError(null);
    try {
      const fresh = await scanProjectRules(selectedProject);
      setView(fresh);
    } catch (err) {
      setError(String(err));
    } finally {
      setScanning(false);
    }
  }

  const report = activeView?.report ?? null;
  const score = activeView?.score ?? null;
  const stale = activeView?.stale ?? false;
  const hasRules = (report?.ruleFiles.length ?? 0) > 0;

  return (
    <Shell
      title="Project Rules"
      subtitle="AI instruction files (AGENTS.md, CLAUDE.md, Cursor rules) graded against each project's tech stack."
    >
      <div className="flex flex-col gap-4">
        {error && (
          <div className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
            {error}
          </div>
        )}

        {isRuntime === false && (
          <div className="rounded-lg border border-border bg-panel px-4 py-3 text-sm text-muted">
            Tauri runtime not detected — launch the desktop app to scan project
            rules.
          </div>
        )}

        <div className="flex min-h-[560px] overflow-hidden rounded-xl border border-border bg-panel shadow-sm">
          {/* Project list */}
          <aside className="w-72 shrink-0 border-r border-border bg-background">
            <div className="border-b border-border px-4 py-3">
              <p className="text-xs font-semibold uppercase tracking-wider text-muted">
                Projects
              </p>
              <p className="mt-1 text-xs text-muted">
                {projectSearch
                  ? `${filteredProjects.length} of ${projects.length}`
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
            <div className="max-h-[640px] overflow-y-auto">
              {loading ? (
                <div className="space-y-2 p-3">
                  {[0, 1, 2, 3].map((i) => (
                    <div
                      key={i}
                      className="h-12 animate-pulse rounded-lg bg-muted"
                    />
                  ))}
                </div>
              ) : projects.length === 0 ? (
                <p className="px-4 py-8 text-xs text-muted">
                  No projects yet. Import transcripts from Settings first.
                </p>
              ) : filteredProjects.length === 0 ? (
                <p className="px-4 py-8 text-xs text-muted">
                  No projects matching &ldquo;{projectSearch}&rdquo;
                </p>
              ) : (
                filteredProjects.map((project) => {
                  const active = selectedProject === project.projectPath;
                  const unassigned = project.projectPath === "Unassigned";
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
                      {!unassigned && (
                        <span className="truncate text-[10px] text-muted">
                          {project.projectPath}
                        </span>
                      )}
                      <span className="text-[10px] text-muted">
                        {project.conversationCount} chat
                        {project.conversationCount === 1 ? "" : "s"}
                      </span>
                    </button>
                  );
                })
              )}
            </div>
          </aside>

          {/* Detail panel */}
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="border-b border-border px-5 py-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="truncate text-sm font-semibold text-foreground">
                    {report?.projectName ??
                      (selectedProject ? "Loading…" : "Select a project")}
                  </h2>
                  {report?.projectPath && report.projectPath !== "Unassigned" && (
                    <p className="truncate text-xs text-muted">
                      {report.projectPath}
                    </p>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={handleRescan}
                    disabled={!selectedProject || scanning || !isRuntime}
                    className="rounded-lg border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-muted-light disabled:opacity-50"
                  >
                    {scanning ? "Scanning…" : "Rescan"}
                  </button>
                  <button
                    type="button"
                    onClick={handleScore}
                    disabled={
                      !selectedProject ||
                      !hasRules ||
                      scoring ||
                      !isRuntime ||
                      (report != null && !report.exists)
                    }
                    className="rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-white hover:bg-accent/90 disabled:opacity-50"
                    title={
                      !hasRules
                        ? "No instruction files to grade in this project"
                        : stale
                          ? "Rules changed since last score — re-grade"
                          : "Grade the rule files against the tech stack"
                    }
                  >
                    {scoring
                      ? "Scoring…"
                      : score == null
                        ? "Score rules"
                        : stale
                          ? "Re-score (stale)"
                          : "Re-score"}
                  </button>
                </div>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-5">
              {!selectedProject ? (
                <p className="py-16 text-center text-sm text-muted">
                  Select a project to view its rules.
                </p>
              ) : scanning && !activeView ? (
                <div className="space-y-3">
                  <div className="h-20 animate-pulse rounded-xl bg-muted/30" />
                  <div className="h-40 animate-pulse rounded-xl bg-muted/30" />
                  <div className="h-32 animate-pulse rounded-xl bg-muted/30" />
                </div>
              ) : !report ? (
                <p className="py-16 text-center text-sm text-muted">
                  Could not scan this project.
                </p>
              ) : !report.exists ? (
                <div className="rounded-xl border border-amber-200 bg-amber-50 p-4 text-sm text-amber-800">
                  <p className="font-semibold">Project folder not accessible</p>
                  <p className="mt-1">
                    The path{" "}
                    <code className="rounded bg-white/60 px-1.5 py-0.5 text-xs">
                      {report.projectPath}
                    </code>{" "}
                    is no longer a directory on this machine. Move the folder or
                    re-import to refresh the path.
                  </p>
                </div>
              ) : (
                <div className="flex flex-col gap-5">
                  {/* Tech stack */}
                  <section className="rounded-xl border border-panel-border bg-panel p-5 shadow-sm">
                    <div className="mb-3 flex items-center justify-between">
                      <h3 className="text-sm font-semibold text-foreground">
                        Detected tech stack
                      </h3>
                      {report.techStack.detected && (
                        <span className="text-[11px] text-muted">
                          {report.techStack.signalFiles.length} signal
                          {report.techStack.signalFiles.length === 1 ? "" : "s"}
                        </span>
                      )}
                    </div>
                    <TechStackPanel stack={report.techStack} />
                  </section>

                  {/* Score */}
                  {score ? (
                    <>
                      {stale && (
                        <div className="rounded-lg border border-amber-200 bg-amber-50 px-4 py-2 text-xs text-amber-800">
                          Rule files have changed since this score was
                          computed — re-score for an up-to-date grade.
                        </div>
                      )}
                      <ScoreCard score={score} />
                    </>
                  ) : hasRules ? (
                    <div className="flex flex-col items-start gap-2 rounded-xl border border-dashed border-border bg-background p-5">
                      <p className="text-sm font-medium text-foreground">
                        Not graded yet
                      </p>
                      <p className="text-xs text-muted">
                        Click <span className="font-medium">Score rules</span>{" "}
                        to grade the {report.ruleFiles.length} instruction
                        file{report.ruleFiles.length === 1 ? "" : "s"} against
                        the detected tech stack.
                      </p>
                    </div>
                  ) : (
                    <div className="rounded-xl border border-dashed border-border bg-background p-5">
                      <p className="text-sm font-medium text-foreground">
                        No instruction files found
                      </p>
                      <p className="mt-1 text-xs text-muted">
                        Add an <code>AGENTS.md</code>, <code>CLAUDE.md</code>,
                        <code> .cursorrules</code>, or files under{" "}
                        <code>.cursor/rules/</code> in the project root, then
                        click <span className="font-medium">Rescan</span>.
                      </p>
                    </div>
                  )}

                  {/* File list */}
                  <section className="rounded-xl border border-panel-border bg-panel p-5 shadow-sm">
                    <div className="mb-3 flex items-center justify-between">
                      <h3 className="text-sm font-semibold text-foreground">
                        Instruction files
                      </h3>
                      <span className="text-[11px] text-muted">
                        {report.ruleFiles.length} file
                        {report.ruleFiles.length === 1 ? "" : "s"} ·{" "}
                        {formatBytes(report.totalBytes)} total
                      </span>
                    </div>
                    {report.ruleFiles.length === 0 ? (
                      <p className="text-xs text-muted">
                        Nothing to show — see suggestions above.
                      </p>
                    ) : (
                      <div className="flex flex-col gap-2">
                        {report.ruleFiles.map((f) => (
                          <RuleFileCard
                            key={f.absolutePath || f.relativePath}
                            file={f}
                          />
                        ))}
                      </div>
                    )}
                  </section>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </Shell>
  );
}
