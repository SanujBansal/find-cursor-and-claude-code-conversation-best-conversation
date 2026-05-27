"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { Shell } from "@/components/Shell";
import {
  getSettings,
  saveSettings,
  importAll,
  getImportStatus,
  getDefaultCursorPath,
} from "../../src/lib/tauri";
import { useTauriRuntime } from "../../src/lib/useTauriRuntime";
import type { AppSettings, ImportAllResult, JobStatusRecord } from "../../src/lib/types";
import { DEFAULT_SETTINGS } from "../../src/lib/types";

function StatusBadge({ status }: { status: string }) {
  const colours: Record<string, string> = {
    pending: "text-muted",
    running: "text-accent",
    completed: "text-[#4ade80]",
    failed: "text-[#f87171]",
  };
  return (
    <span className={colours[status] ?? "text-muted"}>
      {status}
    </span>
  );
}

function SectionHeading({ children }: { children: React.ReactNode }) {
  return (
    <p className="text-xs uppercase tracking-[0.16em] text-accent mb-3">
      {children}
    </p>
  );
}

function FieldRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs text-muted">{label}</label>
      {children}
    </div>
  );
}

function TextInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="w-full rounded border border-panel-border bg-panel px-3 py-1.5 text-sm text-foreground placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-accent"
    />
  );
}

function PathRow({
  label,
  value,
  onChange,
  onBrowse,
  browseLabel = "Browse",
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  onBrowse?: () => void;
  browseLabel?: string;
}) {
  return (
    <FieldRow label={label}>
      <div className="flex gap-2">
        <TextInput value={value} onChange={onChange} placeholder="/path/to/folder" />
        {onBrowse && (
          <button
            type="button"
            onClick={onBrowse}
            className="shrink-0 rounded border border-panel-border bg-panel px-3 py-1.5 text-xs text-muted hover:text-foreground hover:border-accent transition-colors"
          >
            {browseLabel}
          </button>
        )}
      </div>
    </FieldRow>
  );
}

// ── main component ───────────────────────────────────────────────────────────

type ImportState = {
  running: boolean;
  result: ImportAllResult | null;
  error: string | null;
};

const IDLE: ImportState = { running: false, result: null, error: null };

function ImportResultPill({ state }: { state: ImportState }) {
  if (state.running) {
    return <span className="text-xs text-accent animate-pulse">Importing…</span>;
  }
  if (state.error) {
    return <span className="text-xs text-[#f87171]">{state.error}</span>;
  }
  if (state.result) {
    const { cursor, claudeCode, claudeMarkdown, cleared } = state.result;
    const imported =
      cursor.imported + claudeCode.imported + claudeMarkdown.imported;
    const skipped = cursor.skipped + claudeCode.skipped + claudeMarkdown.skipped;
    const errors =
      cursor.errors.length +
      claudeCode.errors.length +
      claudeMarkdown.errors.length;
    return (
      <span className="text-xs text-muted">
        {cleared ? "Cleared existing · " : ""}
        {imported} imported, {skipped} skipped
        {errors > 0 && (
          <span className="text-[#f87171] ml-1">({errors} errors)</span>
        )}
      </span>
    );
  }
  return null;
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const [importState, setImportState] = useState<ImportState>(IDLE);

  const [jobs, setJobs] = useState<JobStatusRecord[]>([]);
  const [lastImportAt, setLastImportAt] = useState<string | null>(null);

  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const isRuntime = useTauriRuntime();

  // ── load settings ──────────────────────────────────────────────────────────
  useEffect(() => {
    if (!isRuntime) return;
    getSettings()
      .then((s) => setSettings(s))
      .catch(() => {});
  }, [isRuntime]);

  // ── poll import status while an import is running ─────────────────────────
  const anyRunning = importState.running;

  useEffect(() => {
    if (!isRuntime) return;

    if (anyRunning) {
      pollRef.current = setInterval(() => {
        getImportStatus()
          .then(setJobs)
          .catch(() => {});
      }, 1500);
    } else if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
      // Final fetch after all imports settle
      getImportStatus()
        .then(setJobs)
        .catch(() => {});
    }

    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [anyRunning, isRuntime]);

  // ── field helpers ──────────────────────────────────────────────────────────
  const update = useCallback(
    <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
      setSettings((prev) => ({ ...prev, [key]: value }));
      setDirty(true);
    },
    []
  );

  // ── save/cancel ────────────────────────────────────────────────────────────
  async function handleSave() {
    if (!isRuntime) return;
    setSaving(true);
    setSaveError(null);
    try {
      await saveSettings(settings);
      setDirty(false);
    } catch (e) {
      setSaveError(String(e));
    } finally {
      setSaving(false);
    }
  }

  function handleCancel() {
    if (!isRuntime) return;
    getSettings()
      .then((s) => {
        setSettings(s);
        setDirty(false);
      })
      .catch(() => {});
  }

  // ── auto-detect ────────────────────────────────────────────────────────────
  async function autoDetectCursor() {
    if (!isRuntime) return;
    try {
      const path = await getDefaultCursorPath();
      update("cursorDataPath", path);
    } catch {
      /* best-effort */
    }
  }

  async function handleImportAll() {
    if (!isRuntime) return;

    setImportState({ running: true, result: null, error: null });
    try {
      const result = await importAll({
        cursorDataPath: settings.cursorDataPath || undefined,
        claudeCodePath: settings.claudeCodePath || undefined,
        claudeMarkdownPath: settings.claudeMarkdownPath || undefined,
        clearExisting: true,
      });
      setImportState({ running: false, result, error: null });
      setLastImportAt(new Date().toLocaleTimeString());
    } catch (e) {
      setImportState({ running: false, result: null, error: String(e) });
    }
  }

  // ── render ─────────────────────────────────────────────────────────────────
  return (
    <Shell
      title="Settings"
      subtitle="Provider keys, transcript paths, and scoring configuration."
    >
      <div className="grid gap-6 lg:grid-cols-2">
        {/* ── OpenAI ── */}
        <section className="rounded border border-panel-border bg-panel p-5 flex flex-col gap-4">
          <SectionHeading>OpenAI</SectionHeading>
          <p className="text-xs text-muted">
            When an OpenAI API key is set, it takes priority over Azure for scoring
            and other AI tasks. You can also set{" "}
            <code className="text-foreground">OPENAI_API_KEY</code> in{" "}
            <code className="text-foreground">{settings.azureEnvPath || ".env"}</code>.
          </p>
          {settings.openAiConfigured ? (
            <p className="text-xs text-[#4ade80]">
              OpenAI configured — used for scoring and AI tasks
            </p>
          ) : (
            <p className="text-xs text-muted">
              No OpenAI key configured. Azure will be used if available.
            </p>
          )}
          <FieldRow label="API Key">
            <input
              type="password"
              value={settings.openAiApiKey}
              onChange={(e) => update("openAiApiKey", e.target.value)}
              placeholder="Leave blank to use OPENAI_API_KEY from .env"
              className="w-full rounded border border-panel-border bg-panel px-3 py-1.5 text-sm text-foreground placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </FieldRow>
          <FieldRow label="Model / Deployment">
            <TextInput
              value={settings.scoringModel}
              onChange={(v) => update("scoringModel", v)}
              placeholder="gpt-4.1-mini"
            />
          </FieldRow>
        </section>

        {/* ── Azure OpenAI ── */}
        <section className="rounded border border-panel-border bg-panel p-5 flex flex-col gap-4">
          <SectionHeading>Azure OpenAI</SectionHeading>
          <p className="text-xs text-muted">
            Used when no OpenAI key is configured. Credentials load from{" "}
            <code className="text-foreground">{settings.azureEnvPath || ".env"}</code>
            . Fields below override the endpoint, API key, and deployment names.
          </p>
          {settings.azureConfigured ? (
            <p className="text-xs text-[#4ade80]">
              {settings.azureEndpoint
                ? `Configured for ${settings.azureEndpoint}`
                : "Azure credentials configured"}
            </p>
          ) : (
            <p className="text-xs text-[#f87171]">
              Azure credentials not found. Add AZURE_OPENAI_ENDPOINT and
              AZURE_OPENAI_API_KEY to `.env`, or enter them below.
            </p>
          )}
          <FieldRow label="Deployment URL">
            <TextInput
              value={settings.azureEndpoint}
              onChange={(v) => update("azureEndpoint", v)}
              placeholder="https://your-resource.openai.azure.com"
            />
          </FieldRow>
          <FieldRow label="API Key">
            <input
              type="password"
              value={settings.azureApiKey}
              onChange={(e) => update("azureApiKey", e.target.value)}
              placeholder="Leave blank to use AZURE_OPENAI_API_KEY from .env"
              className="w-full rounded border border-panel-border bg-panel px-3 py-1.5 text-sm text-foreground placeholder:text-muted focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </FieldRow>
        </section>

        {/* ── Transcript Sources ── */}
        <section className="rounded border border-panel-border bg-panel p-5 flex flex-col gap-4">
          <SectionHeading>Transcript Sources</SectionHeading>
          <PathRow
            label="Cursor Data Path"
            value={settings.cursorDataPath}
            onChange={(v) => update("cursorDataPath", v)}
            onBrowse={autoDetectCursor}
            browseLabel="Auto-detect"
          />
          <PathRow
            label="Claude Code Transcripts Path"
            value={settings.claudeCodePath}
            onChange={(v) => update("claudeCodePath", v)}
          />
          <PathRow
            label="Claude Markdown Exports Folder"
            value={settings.claudeMarkdownPath}
            onChange={(v) => update("claudeMarkdownPath", v)}
          />
        </section>

        {/* ── Import ── */}
        <section className="rounded border border-panel-border bg-panel p-5 flex flex-col gap-4 lg:col-span-2">
          <SectionHeading>Import</SectionHeading>
          <p className="text-xs text-muted">
            Clears existing transcripts and imports all conversations per project
            (with at least one user message). Score conversations separately from
            the Conversations page.
          </p>

          <div className="flex flex-wrap items-center gap-4">
            <button
              type="button"
              disabled={anyRunning || !isRuntime}
              onClick={handleImportAll}
              className="rounded border border-accent px-4 py-1.5 text-sm text-accent hover:bg-accent hover:text-panel transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              {anyRunning ? "Importing…" : "Clear & Import All"}
            </button>

            {lastImportAt && (
              <span className="text-xs text-muted">
                Last import: {lastImportAt}
              </span>
            )}
          </div>

          <div className="flex flex-col gap-1 text-sm">
            <ImportResultPill state={importState} />
          </div>

          {/* Job queue status */}
          {jobs.length > 0 && (
            <div className="mt-2">
              <p className="text-xs uppercase tracking-[0.14em] text-muted mb-2">
                Job Queue
              </p>
              <div className="flex flex-col gap-1 max-h-40 overflow-y-auto text-xs font-mono">
                {jobs.map((job) => (
                  <div key={job.id} className="flex gap-3 items-center text-muted">
                    <span className="w-6 text-right shrink-0">{job.id}</span>
                    <span className="w-14 shrink-0">{job.jobType}</span>
                    <StatusBadge status={job.status} />
                    {job.progress !== null && job.progress > 0 && (
                      <span>{Math.round(job.progress * 100)}%</span>
                    )}
                    {job.errorMessage && (
                      <span className="text-[#f87171] truncate">{job.errorMessage}</span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}
        </section>
      </div>

      {/* ── Save / Cancel ── */}
      <div className="mt-6 flex items-center gap-3">
        <button
          type="button"
          disabled={!dirty || saving || !isRuntime}
          onClick={handleSave}
          className="rounded border border-accent px-4 py-1.5 text-sm text-accent hover:bg-accent hover:text-panel transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {saving ? "Saving…" : "Save"}
        </button>
        <button
          type="button"
          disabled={!dirty || !isRuntime}
          onClick={handleCancel}
          className="rounded border border-panel-border px-4 py-1.5 text-sm text-muted hover:text-foreground hover:border-accent transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
        >
          Cancel
        </button>
        {saveError && (
          <span className="text-xs text-[#f87171]">{saveError}</span>
        )}
        {!dirty && !saving && !saveError && (
          <span className="text-xs text-muted">All changes saved</span>
        )}
      </div>
    </Shell>
  );
}
