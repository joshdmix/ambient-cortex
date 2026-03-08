# Ambient Dev Cortex — Implementation Plan

## 1. Architecture Overview

Multi-process, local-first daemon architecture with five layers:

```
                                    +---------------------+
                                    |   UI / Notification |
                                    |   Layer (TUI/tmux)  |
                                    +----------+----------+
                                               |
                                    +----------v----------+
                                    |   Inference Engine   |
                                    | (Claude API + local  |
                                    |  reasoning rules)    |
                                    +----------+----------+
                                               |
                                    +----------v----------+
                                    |   Knowledge Graph    |
                                    | (SQLite + local      |
                                    |  vector embeddings)  |
                                    +----------+----------+
                                               |
                                    +----------v----------+
                                    |   Event Bus (mpsc)   |
                                    +----------+----------+
                                               |
                       +------------+----------+----------+------------+
                       |            |                      |            |
                  +----v----+ +----v-----+          +-----v----+ +----v----+
                  | Terminal | |   FS     |          |   Git    | | Editor  |
                  | Watcher | | Watcher  |          |  Watcher | | Watcher |
                  +---------+ +----------+          +----------+ +---------+
```

**Single daemon process** (`cortexd`) runs all watchers as async tasks within a tokio runtime. No microservices, no IPC complexity. One binary, one process, multiple async tasks communicating over internal channels.

A separate **CLI binary** (`cortex`) communicates with the daemon over a Unix domain socket for queries, configuration, and the TUI.

## 2. Tech Stack

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| Language | **Rust** | Low resource footprint for always-on daemon |
| Async runtime | **tokio** | Full-featured async runtime |
| FS watching | **notify 7** | Mature file system notification library |
| TUI | **ratatui 0.29 + crossterm** | Standard Rust TUI stack |
| Structured storage | **SQLite via rusqlite** | Zero-config, single-file, perfect for local-first |
| Vector storage | **SQLite** (brute-force scan for MVP) | Upgrade to sqlite-vss or embedded qdrant later |
| Local embeddings | **fastembed-rs** (ONNX-based, BGE-small-en-v1.5) | No Python dependency, runs in-process, 384-dim embeddings, fast on CPU |
| Claude API | **reqwest** with streaming | For reasoning over context, not embeddings |
| Serialization | **serde + serde_json** | Standard Rust serialization |
| CLI framework | **clap 4 (derive)** | Standard Rust CLI |
| IPC | **Unix domain socket** | Daemon-to-CLI communication |
| Shell integration | **Custom zsh/bash preexec/precmd hooks** | Captures commands and their context |
| Git integration | **git2-rs** (libgit2 bindings) | Parse repo state without shelling out |
| Error handling | **anyhow + thiserror** | Standard Rust error handling |

## 3. Module Breakdown / Directory Structure

```
ambient-cortex/
  Cargo.toml                          # [workspace] members = ["crates/*"]
  README.md
  LICENSE
  .gitignore
  hooks/
    cortex-preexec.zsh               # ~40 lines, zsh integration
    cortex-preexec.bash              # ~40 lines, bash integration
    cortex-git-post-commit           # ~15 lines, writes JSON to pipe
  crates/
    cortex-common/
      Cargo.toml
      src/
        lib.rs                       # Re-exports
        protocol.rs                  # IPC request/response types
        events.rs                    # CortexEvent enum, EventType enum
        models.rs                    # FileNode, Pattern, Insight structs
    cortexd/
      Cargo.toml
      src/
        main.rs                      # Daemon entry: tokio::main, signal handling, socket server
        config.rs                    # Config struct, TOML loading, defaults
        bus.rs                       # tokio::sync::broadcast channel wrapper
        server.rs                    # Unix socket server, handles CLI requests
        watchers/
          mod.rs                     # WatcherManager: spawns and manages all watchers
          terminal.rs                # Reads from named pipe, parses shell events
          filesystem.rs              # notify-based, debounced, gitignore-aware
          git.rs                     # git2-rs polling + hook event receiver
          editor.rs                  # Phase 2 stub
        graph/
          mod.rs                     # KnowledgeGraph facade
          store.rs                   # SQLite ops: insert_event, query_file_history, etc.
          embeddings.rs              # fastembed model loading, embed(), similarity_search()
          models.rs                  # DB row structs, From<> impls
          migrations.rs              # Schema versioning
        engine/
          mod.rs                     # InferenceEngine: orchestrates trigger + ranker
          trigger.rs                 # TriggerEvaluator: decides when to generate insights
          ranker.rs                  # InsightRanker: scores and filters insights
          claude.rs                  # ClaudeClient: API calls, prompt templates, rate limiting
          rules.rs                   # LocalRules: heuristic pattern matching
    cortex/
      Cargo.toml
      src/
        main.rs                      # CLI entry: clap derive, subcommand dispatch
        commands/
          mod.rs
          status.rs                  # cortex status: shows daemon health, stats
          query.rs                   # cortex query <file>: file history + insights
          history.rs                 # cortex history: browse sessions
          tui.rs                     # cortex tui: full ratatui dashboard
          install.rs                 # cortex install: sets up hooks, creates dirs
          config.rs                  # cortex config: edit settings
          search.rs                  # cortex search <query>: semantic search
```

## 4. Data Model (SQLite Schema)

### events — Raw event stream
```sql
CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL,          -- ISO 8601
    event_type  TEXT NOT NULL,          -- 'file_open', 'file_save', 'command_run', 'git_commit', 'git_checkout', 'error_encountered', 'build_result'
    source      TEXT NOT NULL,          -- 'terminal', 'filesystem', 'git', 'editor'
    project     TEXT,                   -- absolute path to project root
    file_path   TEXT,                   -- file involved, if any
    payload     TEXT NOT NULL,          -- JSON blob with source-specific data
    session_id  TEXT                    -- groups events into work sessions
);
```

### file_nodes — Accumulated knowledge per file
```sql
CREATE TABLE file_nodes (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    path          TEXT UNIQUE NOT NULL,
    project       TEXT NOT NULL,
    first_seen    TEXT NOT NULL,
    last_touched  TEXT NOT NULL,
    touch_count   INTEGER DEFAULT 0,
    total_time_s  INTEGER DEFAULT 0,   -- estimated time spent in file
    tags          TEXT                  -- JSON array of inferred tags
);
```

### file_relations — Graph edges (files commonly edited together)
```sql
CREATE TABLE file_relations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    file_a      INTEGER REFERENCES file_nodes(id),
    file_b      INTEGER REFERENCES file_nodes(id),
    relation    TEXT NOT NULL,          -- 'co_edited', 'imports', 'breaks_when_changed', 'test_for'
    strength    REAL DEFAULT 1.0,       -- incremented each time relation is observed
    last_seen   TEXT NOT NULL,
    UNIQUE(file_a, file_b, relation)
);
```

### patterns — Recurring behaviors
```sql
CREATE TABLE patterns (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern_type  TEXT NOT NULL,        -- 'edit_revert', 'repeated_error', 'debug_cycle', 'context_switch', 'always_co_edit'
    description   TEXT NOT NULL,
    file_paths    TEXT NOT NULL,        -- JSON array
    first_seen    TEXT NOT NULL,
    last_seen     TEXT NOT NULL,
    occurrence_count INTEGER DEFAULT 1,
    confidence    REAL DEFAULT 0.5      -- 0.0 to 1.0
);
```

### insights — Proactive insights ready to surface
```sql
CREATE TABLE insights (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at    TEXT NOT NULL,
    trigger_event INTEGER REFERENCES events(id),
    insight_type  TEXT NOT NULL,        -- 'warning', 'reminder', 'suggestion', 'history'
    title         TEXT NOT NULL,        -- short one-liner
    body          TEXT NOT NULL,        -- detailed explanation
    relevance     REAL NOT NULL,        -- 0.0 to 1.0 score
    surfaced      INTEGER DEFAULT 0,   -- 0 = pending, 1 = shown
    dismissed     INTEGER DEFAULT 0,
    file_path     TEXT,
    project       TEXT
);
```

### embeddings — Vector store for semantic search
```sql
CREATE TABLE embeddings (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    source_type TEXT NOT NULL,          -- 'event', 'insight', 'pattern', 'commit_message'
    source_id   INTEGER NOT NULL,
    vector      BLOB NOT NULL,         -- 384-dim f32 vector (BGE-small)
    text        TEXT NOT NULL           -- the text that was embedded
);
```

## 5. How Watchers Work

### Terminal Watcher

Shell hook scripts (installed by `cortex install`) use zsh `preexec`/`precmd` to capture command text, exit code, duration, working directory, and git branch. Writes JSON lines to a named pipe at `$XDG_RUNTIME_DIR/cortex/terminal.pipe`.

```zsh
# cortex-preexec.zsh (simplified)
__cortex_preexec() {
    __cortex_cmd="$1"
    __cortex_start=$EPOCHREALTIME
}
__cortex_precmd() {
    local exit_code=$?
    [[ -n "$__cortex_cmd" ]] || return
    local duration=$(( EPOCHREALTIME - __cortex_start ))
    printf '{"cmd":"%s","exit":%d,"dur":%.3f,"cwd":"%s","branch":"%s"}\n' \
        "$__cortex_cmd" "$exit_code" "$duration" "$PWD" "$(git branch --show-current 2>/dev/null)" \
        > "${XDG_RUNTIME_DIR}/cortex/terminal.pipe" 2>/dev/null
    unset __cortex_cmd
}
autoload -Uz add-zsh-hook
add-zsh-hook preexec __cortex_preexec
add-zsh-hook precmd __cortex_precmd
```

### Filesystem Watcher

Uses `notify` crate. Watches configured project directories. Debounces events (500ms). Tracks file open/save sequences to estimate time-in-file. Detects rapid save-then-revert patterns. Ignores `target/`, `node_modules/`, `.git/objects/`, and configurable exclude patterns. Uses `ignore` crate for gitignore-aware filtering.

### Git Watcher

Dual approach:
1. **Git hooks** (`post-commit`, `post-checkout`, `post-merge`) write JSON events to named pipe
2. **Polling fallback** — every 5s, check HEAD ref and diff-stat via `git2-rs`

Captures: commit hash, message, changed files with diff stats, branch switches, merges, rebases.

### Editor Watcher (Phase 2)

VS Code: reads recently-opened files from `~/.config/Code/User/globalStorage/state.vscdb`. Neovim: small Lua plugin writes events to named pipe.

## 6. Inference / Proactive Surfacing Engine

### Tier 1: Local Rules (no API call, instant)

| Rule | Trigger | Insight |
|------|---------|---------|
| `edit_revert_detector` | File saved, then reverted within 5 min | "You reverted changes to X. Last time this happened you were debugging Y." |
| `co_edit_reminder` | File A opened, A has strong `co_edited` relation with B | "You usually edit B when you change A." |
| `error_pattern` | Terminal command fails with same error seen before | "You hit this error 3 times before. Last fix was: [commit message]" |
| `stale_branch` | Git checkout to branch not touched in >7 days | "This branch was last active 12 days ago. You were working on: [summary]" |
| `long_debug_cycle` | Same file saved >5 times in 10 min with test failures between | "You've been iterating on X for 15 min. Related past fix: [link]" |
| `context_switch` | Working directory changes to different project | "Last time in this project (3 days ago): you were fixing the payment flow." |

### Tier 2: Claude-Powered Reasoning (batched, rate-limited)

Triggered only when:
1. **File-open with history**: File has >3 patterns or >10 past events — batch history, ask Claude for relevant context summary
2. **Error correlation**: Retrieve 5 most similar past errors by embedding, ask Claude to identify relevant fixes
3. **Session summary**: Every 30 min of active dev, generate "session so far" summary

Config: `claude-sonnet-4-6`, max 10 calls/hour, local response caching.

### Ranking (`engine/ranker.rs`)

Relevance score (0.0-1.0) computed from:
- **Recency**: exponential decay
- **Frequency**: log scale
- **Severity**: errors/reverts score higher than informational
- **User feedback**: dismissed insights reduce future scores
- **Embedding similarity**: semantic closeness to current context

Threshold: 0.6 (configurable, adapts based on dismiss rate).

## 7. UI Layer

### Primary: Shell Prompt (Phase 1)

Daemon writes to `~/.local/share/cortex/current_insight.json`. Shell prompt hook displays one-line notification:

```
cortex  Last time you touched auth.rs, the middleware broke. Fix was in commit a3f2b1c.
$
```

### Secondary: TUI Dashboard (`cortex tui`) — Phase 1

ratatui-based TUI with panels: activity stream, active insights, file graph, searchable history.

### Tertiary: tmux Status Bar — Phase 2

`cortex tmux-status` outputs short string for `set -g status-right '#(cortex tmux-status)'`.

### Quaternary: macOS Notifications — Phase 3

Using `osascript` for high-importance insights (relevance >0.9).

## 8. Privacy

- All data stored locally in `~/.local/share/cortex/`
- Embeddings computed locally via fastembed-rs (no network)
- Claude API calls opt-in, can be fully disabled (`claude_enabled = false`)
- Only summaries sent to Claude, never raw file contents
- Command sanitization: env vars, tokens, passwords replaced with `[REDACTED]`
- Configurable file exclusions (`.env*`, `*secret*`, `*.pem`, `*.key`)
- No telemetry, no cloud sync
- Auto-prune events older than `retention_days` (default: 90)

## 9. Cargo Dependencies

### Cargo.toml (workspace root)
```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
```

### cortexd/Cargo.toml key dependencies
```toml
[dependencies]
cortex-common = { path = "../cortex-common" }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
notify = "7"
rusqlite = { version = "0.32", features = ["bundled"] }
fastembed = "4"
reqwest = { version = "0.12", features = ["json", "stream"] }
git2 = "0.19"
ignore = "0.4"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
ratatui = "0.29"
crossterm = "0.28"
dirs = "6"
toml = "0.8"
```

## 10. Phase Breakdown

### Phase 1: MVP — Passive Memory

Goal: Record developer activity and surface basic insights on file open.

- Project scaffolding (workspace, crates, CI)
- Config system (`~/.config/cortex/config.toml`)
- Daemon skeleton with tokio runtime and signal handling
- SQLite storage layer with migrations
- Filesystem watcher (notify-based)
- Terminal watcher with zsh hook script
- Basic git watcher (polling HEAD, reading commit messages)
- Event ingestion pipeline (watcher → bus → store)
- File co-edit detection (basic `file_relations` tracking)
- Local embedding with fastembed-rs
- Tier 1 rules: `co_edit_reminder`, `context_switch`
- Shell prompt integration for insight display
- `cortex status` and `cortex install` CLI commands
- `cortex query <file>` — show history for a file

### Phase 2: Intelligence Layer

Goal: Pattern detection and Claude-powered reasoning.

- Pattern detection: `edit_revert_detector`, `error_pattern`, `long_debug_cycle`
- Semantic search over past events using embeddings
- Claude API integration for insight generation
- Insight ranking and relevance scoring
- User feedback loop (dismiss/upvote insights)
- TUI dashboard with ratatui
- Git hooks (post-commit, post-checkout)
- Session detection and summarization

### Phase 3: Polish and Ecosystem

Goal: Editor integration, richer graph, better UX.

- Editor watcher (VS Code extension or Neovim plugin)
- tmux status bar integration
- macOS notification support
- File relationship visualization in TUI
- `cortex search <query>` — semantic search over all history
- Data export/import
- Retention policies and storage management

### Phase 4: Advanced Features (future)

- Cross-project pattern detection
- Team knowledge sharing (opt-in, encrypted)
- Predictive suggestions ("you usually run tests after changing this file")
- Integration with Claude Code agent sessions
