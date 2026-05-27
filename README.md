# Vibe Score

Vibe Score is a macOS desktop application that imports your AI coding-assistant transcripts (Cursor, Claude Code, and Claude web exports), scores each conversation against a multi-dimensional rubric using an LLM, and surfaces analytics — daily/weekly score trends, semantic search across all sessions, and personalised learning suggestions — in a clean native window backed by a local SQLite database.

## Prerequisites

| Tool | Version |
|------|---------|
| Node.js | 18 or later |
| npm | 9 or later (bundled with Node 18) |
| Rust / cargo | stable (1.75+) |
| macOS | 12 Monterey or later |

## Setup

```bash
# Install JavaScript dependencies
npm install

# Build the Rust backend (first run fetches crates — takes ~2 min)
cd src-tauri && cargo build && cd ..
```

## Development

```bash
npm run dev   # launches Next.js + Tauri desktop window
```

The Next.js dev server starts on `http://localhost:3000`. Tauri opens a native window pointing at that URL. Hot-reload works for both the frontend and — after a recompile — the Rust backend.

## Build (macOS DMG)

```bash
npm run build          # production Next.js export + Tauri DMG
# or, equivalently:
cd src-tauri && cargo tauri build
```

The signed `.dmg` is written to `src-tauri/target/release/bundle/dmg/`.

## Running Tests

```bash
cd src-tauri && cargo test
```

## Configuration

1. Open the app and navigate to **Settings**.
2. Enter your **OpenAI API key** — used to score transcripts with `gpt-4o-mini` by default.
3. Set the **source paths** for each transcript type you want to import:
   - *Cursor* — folder containing `cursor-history` JSON exports
   - *Claude Code* — folder containing `.jsonl` session files
   - *Claude Markdown* — folder containing `.md` files from [claude-chat-exporter](https://github.com/nicholasgasior/claude-chat-exporter)

## Workflow

```
Import transcripts  →  Score with LLM  →  View Dashboard  →  Chat Search
```

1. **Import** — click *Import* on any source page to pull new transcripts into the local database.
2. **Score** — click *Score unscored* (or enable auto-score) to send each conversation to the LLM and store the rubric results.
3. **Dashboard** — view daily and weekly vibe scores, dimension breakdowns, and trend charts.
4. **Search** — semantically search all past conversations using embedding-based retrieval.
5. **Suggestions** — review AI-generated learning suggestions derived from your weakest rubric dimensions.

## Project Structure

```
cursor-best-transcript/
├── src/                  # Next.js frontend (App Router)
│   └── app/
│       ├── page.tsx                  # Dashboard
│       ├── conversations/            # Conversation list + detail
│       ├── search/                   # Semantic search
│       ├── suggestions/              # Learning suggestions
│       └── settings/                 # API key + source paths
├── src-tauri/            # Tauri + Rust backend
│   ├── src/
│   │   ├── importers/    # cursor, claude_code, claude_markdown parsers
│   │   ├── scoring/      # LLM scoring rubric + prompt builder
│   │   ├── analytics/    # Aggregation + trend computation
│   │   ├── embeddings/   # Chunking + OpenAI embeddings
│   │   ├── search/       # Cosine-similarity retrieval
│   │   ├── suggestions/  # Learning suggestion generator
│   │   └── db.rs         # SQLite migrations
│   └── tests/
│       └── fixtures/     # Sample transcripts for offline testing
└── README.md
```
