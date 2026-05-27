# Vibe Score

**Find your best AI coding sessions — objectively.**

Vibe Score is a macOS desktop app that imports your AI coding transcripts (Cursor, Claude Code, Claude web), scores every conversation with an LLM rubric, and surfaces the sessions that show your strongest engineering instincts.

As vibe coding becomes a signal recruiters and hiring managers actively look for, Vibe Score gives you an objective way to surface, compare, and share your best sessions — and a data-driven feedback loop to improve over time.

---

## Why it exists

Vibe coding is increasingly used as a hiring signal. Candidates are asked to share transcripts, and recruiters are starting to evaluate them. The problem: without an objective measure, you have no idea which of your hundreds of sessions actually demonstrates your best work.

Vibe Score solves this by:

- **Automatically importing** all your Cursor and Claude Code sessions
- **Scoring every conversation** across a rubric designed to surface engineering judgment, not just "did the code work"
- **Ranking sessions** so you can instantly find the top 3 that are worth sharing with a recruiter or including in a portfolio
- **Tracking your trend** over time so you can see whether your vibe coding is actually improving

---

## How it works

```
Import transcripts  →  Score with LLM  →  View Dashboard
```

### 1. Import

Vibe Score reads transcript files directly from your local machine — nothing is uploaded to a server. It supports three sources:

| Source | What it reads |
|--------|--------------|
| **Cursor** | Agent session files from `~/.cursor/` |
| **Claude Code** | `.jsonl` session files from `~/.claude/` |
| **Claude web** | `.md` exports via [claude-chat-exporter](https://github.com/nicholasgasior/claude-chat-exporter) |

### 2. Score

Each conversation is sent to an LLM (via Azure OpenAI or a direct OpenAI API key) and graded across six dimensions on a 0–5 scale:

| Dimension | What it measures |
|-----------|-----------------|
| **Task Completion** | Did the session reach its actual goal and get verified? |
| **Technical Correctness** | Is the code, reasoning, and tooling actually right? |
| **Workflow Quality** | Was the engineering process disciplined — read first, plan, verify? |
| **Tool Use & Context** | Did the agent use search, files, and existing patterns well? |
| **Communication Clarity** | Was the session focused and easy to follow? |
| **Learning Leverage** | Does the session surface reusable patterns or insights? |

A **weakest-link penalty** prevents high scores on five dimensions from masking a serious weakness on the sixth. One mediocre axis pulls the headline number down.

The rubric is calibrated to be strict: the most common honest score is 2–3. A 5 is genuinely exceptional.

### 3. Dashboard

- **Today's score** — weighted average of sessions from today
- **Weekly trend** — last 8 weeks of daily averages as a chart
- **Top 3 sessions** — your highest-scoring conversations across all time
- **Weakest dimensions** — the 3 rubric axes where your sessions score lowest over the last 30 days, so you know where to improve

---

## Installation (macOS)

Download the latest `.dmg` from the [Releases](../../releases) page, open it, and drag **Vibe Score** into your Applications folder. No dependencies required — the app is self-contained.

> **First launch:** macOS may show a security prompt. Go to **System Settings → Privacy & Security** and click **Open Anyway**.

---

## Building from source

### Prerequisites

| Tool | Version |
|------|---------|
| Node.js | 18 or later |
| npm | 9 or later |
| Rust / cargo | stable 1.75+ |
| macOS | 12 Monterey or later |

### Setup

```bash
# Install JavaScript dependencies
npm install

# Build the Rust backend (first run fetches crates — ~2 min)
cd src-tauri && cargo build && cd ..
```

### Development

```bash
npm run dev   # launches Next.js + Tauri desktop window
```

The Next.js dev server starts on `http://localhost:3000`. Tauri opens a native window pointing at that URL. Hot-reload works for the frontend; the Rust backend recompiles on change.

### Production build (macOS DMG)

```bash
npm run build
```

The `.dmg` is written to `src-tauri/target/release/bundle/dmg/`.

### Tests

```bash
cd src-tauri && cargo test
```

---

## Configuration

1. Open the app and go to **Settings**.
2. Enter your **OpenAI API key** — used for LLM scoring (`gpt-4.1-mini` by default, configurable).
3. Set source paths for each transcript type you want to import:
   - **Cursor** — path to your Cursor data directory (auto-detected by default)
   - **Claude Code** — path to your Claude Code sessions folder
   - **Claude Markdown** — folder containing `.md` exports from claude-chat-exporter

Azure OpenAI is also supported: set `AZURE_OPENAI_ENDPOINT` and `AZURE_OPENAI_API_KEY` in a `.env` file at the project root, or configure them in Settings.

---

## Privacy

All transcript data stays on your machine. The only network request Vibe Score makes is to the OpenAI (or Azure OpenAI) API endpoint you configure — your conversation text is sent there for scoring and nowhere else. Scores and metadata are stored in a local SQLite database inside the app's data directory.

---

## Project structure

```
cursor-best-transcript/
├── app/                      # Next.js frontend (App Router)
│   ├── page.tsx              # Dashboard
│   ├── conversations/        # Conversation list + detail view
│   ├── rules/                # Project rules analysis
│   └── settings/             # API key + source paths
├── components/               # Shared UI components
├── src/
│   └── lib/
│       ├── types.ts          # Shared TypeScript types
│       └── tauri.ts          # Tauri API bridge
└── src-tauri/                # Tauri + Rust backend
    ├── src/
    │   ├── importers/        # cursor, claude_code, claude_markdown parsers
    │   ├── scoring/          # LLM rubric, prompt builder, scorer
    │   ├── analytics/        # Daily/weekly aggregation + trend computation
    │   ├── commands.rs       # Tauri command handlers (IPC)
    │   └── db.rs             # SQLite setup + migrations
    └── migrations/           # SQL schema migrations
```

---

## GitHub Releases (publishing a new version)

> This section is for maintainers.

1. **Build the DMG** on a Mac:
   ```bash
   npm run build
   # Output: src-tauri/target/release/bundle/dmg/Vibe Score_<version>_aarch64.dmg
   ```
2. **Tag the release** and push:
   ```bash
   git tag v0.1.0 && git push origin v0.1.0
   ```
3. **Create a GitHub Release** and attach the `.dmg` file. Users can then download it from the Releases page and install with drag-and-drop — no Homebrew or command line required.

For automated releases via GitHub Actions, see the [Tauri GitHub Action](https://github.com/tauri-apps/tauri-action) which can build and publish `.dmg` artifacts on every tagged push.

---

## License

MIT
