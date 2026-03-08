# Ambient Dev Cortex — Team Instructions for Next Agent

## Project Location
`/Users/joshuamix/projects/ambient-cortex/`

## Context
This is a Rust workspace with 3 crates: `cortex-common` (shared types), `cortexd` (daemon), `cortex` (CLI). Phase 1 (passive memory) and Phase 2 (intelligence layer) are implemented but have gaps. Phase 3 (polish/ecosystem) is not started. Your job is to complete everything.

**Repo**: https://github.com/joshdmix/ambient-cortex
**Branch**: main
**Build**: `cargo build` (compiles clean, ~4 dead_code warnings expected)
**Git config**: joshuadmix@gmail.com / Joshua Mix — NEVER add Co-Authored-By lines

## Architecture Quick Reference
- **cortexd** (daemon): tokio async runtime, SQLite via rusqlite, notify for FS watching, git2 for git, fastembed for embeddings, reqwest for Claude API
- **cortex** (CLI): clap derive, ratatui TUI, communicates with daemon over Unix domain socket (JSON protocol)
- **cortex-common**: shared types — CortexEvent, Request/Response enums, model structs
- **IPC**: Unix socket at `~/.local/share/cortex/cortexd.sock`, newline-delimited JSON
- **Config**: `~/.config/cortex/config.toml` via `crates/cortexd/src/config.rs`

## What You Need To Do

There are **3 categories** of work: (A) fix gaps from Phase 1/2, (B) implement Phase 3, (C) implement Phase 4. Run a team with parallel workstreams.

---

### CATEGORY A: Fix Phase 1/2 Gaps (HIGH PRIORITY — do these first)

#### A1. Fix TUI to use GetInsights endpoint
**File**: `crates/cortex/src/commands/tui.rs`
- The TUI currently uses `Search { query: "*" }` as a hacky workaround to display insights
- Change it to call `Request::GetInsights` and parse `Response::InsightsResult`
- The protocol types already exist in `cortex-common/src/protocol.rs`
- The server handler already exists in `cortexd/src/server.rs`

#### A2. Implement `current_insight.json` writing in daemon
**Files**: `crates/cortexd/src/main.rs`, possibly new file `crates/cortexd/src/insight_writer.rs`
- The shell hook at `hooks/cortex-preexec.zsh` has a `__cortex_prompt()` function that reads `~/.local/share/cortex/current_insight.json`
- But the daemon NEVER writes this file
- Add a background task that runs every 5 seconds, queries pending insights from KnowledgeGraph, and writes the top insight to `current_insight.json` as `{"title":"...","body":"...","type":"..."}`
- Also fix the zsh hook: `__cortex_prompt` is defined but never added to precmd hooks — add it

#### A3. Install git hooks via `cortex install`
**File**: `crates/cortex/src/commands/install.rs`
- Currently only creates directories and shell hook
- Add: scan `config.watch_dirs` (or current dir) for git repos, copy `cortex-git-post-commit` to `.git/hooks/post-commit`, create a `post-checkout` hook that writes JSON to the named pipe
- Make hooks executable (chmod +x)
- Ask user before overwriting existing hooks (append to existing or skip)

#### A4. Wire git hooks into daemon (dual-mode git watcher)
**File**: `crates/cortexd/src/watchers/git.rs`
- Currently uses polling-only approach (5s interval via git2)
- The PLAN says dual approach: hooks write JSON to named pipe + polling fallback
- Add a named pipe listener (like terminal watcher) that reads JSON from `cortex/git.pipe`
- Update the git hooks to write to this pipe
- Keep the polling fallback for repos without hooks installed

#### A5. Add `stale_branch` rule
**File**: `crates/cortexd/src/engine/rules.rs`
- Missing from Tier 1 rules. The PLAN specifies:
  - Trigger: `GitCheckout` event
  - Logic: Query events for the branch being checked out. If last event was >7 days ago, generate insight
  - Message: "This branch was last active {N} days ago. You were working on: {summary from last events}"
- Also add to `evaluate()` call chain

#### A6. Implement session summarization
**Files**: `crates/cortexd/src/engine/mod.rs`, `crates/cortexd/src/graph/store.rs`
- Session detection exists (30-min gap) but summarization is a placeholder
- `SessionSummary.summary` is hardcoded to `"{N} events"` in store.rs
- When a session rotates (gap detected), if Claude is enabled, call `ClaudeClient::generate_insight()` with `PromptType::SessionSummary` passing a summary of the session's events
- Store the summary text back into the events table or a new session summary field
- If Claude is disabled, generate a local summary: "Worked on {files} in {project}, {N} commands, {M} commits"

#### A7. Add CI (GitHub Actions)
**File**: `.github/workflows/ci.yml`
- Basic Rust CI: checkout, install Rust stable, `cargo check`, `cargo test`, `cargo clippy`
- Run on push to main and PRs
- Cache cargo registry and target dir

---

### CATEGORY B: Phase 3 — Polish and Ecosystem

#### B1. Editor watcher — Neovim plugin
**Files**: `crates/cortexd/src/watchers/editor.rs` (currently a stub), new file `hooks/cortex.nvim` or `hooks/cortex-nvim.lua`
- Create a small Neovim Lua plugin that writes JSON events to the named pipe on BufEnter, BufWrite, BufDelete
- Format: `{"type":"file_open","path":"/abs/path","timestamp":"ISO8601"}` etc.
- Update `editor.rs` to read from `cortex/editor.pipe` named pipe (same pattern as terminal watcher)
- Update `WatcherManager` to spawn the editor watcher

#### B2. tmux status bar integration
**File**: new subcommand in `crates/cortex/src/commands/tmux.rs`
- `cortex tmux-status` outputs a short one-line string for tmux status bar
- Reads `current_insight.json` (from A2) and formats: `"⚡ {title}"` (truncated to 40 chars)
- If no insight, output `"cortex: idle"`
- Add the subcommand to `main.rs`
- Document usage: `set -g status-right '#(cortex tmux-status)'`

#### B3. macOS notifications
**File**: new module `crates/cortexd/src/notifier.rs`
- For insights with relevance > 0.9, send macOS notification via `osascript -e 'display notification ...'`
- Run as a background task in the daemon, polls pending insights every 10 seconds
- Only notify once per insight (check `surfaced` flag, mark as surfaced after notification)
- Config toggle: `notifications_enabled = true` in config.toml (add to CortexConfig)

#### B4. File relationship visualization in TUI
**File**: `crates/cortex/src/commands/tui.rs`
- Add a new tab/panel (press `g` for "graph") showing file relationships
- Query related files for the currently selected file in the activity stream
- Display as a simple text-based tree/list:
  ```
  src/main.rs
  ├── co_edited (5.0) src/config.rs
  ├── co_edited (3.0) src/server.rs
  └── test_for  (1.0) tests/main_test.rs
  ```
- Use `Request::Query { file_path }` to get related files

#### B5. Data export/import
**File**: new subcommands `crates/cortex/src/commands/export.rs` and `crates/cortex/src/commands/import.rs`
- `cortex export --output cortex-backup.json` — dumps all events, insights, patterns, file_nodes to JSON
- `cortex import --input cortex-backup.json` — loads from JSON backup
- Add new protocol types: `Request::Export` and `Request::Import { data: String }`
- Server handler reads from Store and serializes, or deserializes and bulk inserts
- Add subcommands to main.rs

#### B6. Retention policies and storage management
**Files**: `crates/cortexd/src/graph/store.rs`, `crates/cortexd/src/main.rs`
- Add `prune_old_events(retention_days: u64) -> Result<u64>` to Store — deletes events older than N days, returns count deleted
- Also prune orphaned embeddings (where source event no longer exists)
- Run pruning on daemon startup and daily (spawn a background task with 24h interval)
- Log how many events/embeddings were pruned
- `config.retention_days` already exists in CortexConfig (default 90)

---

### CATEGORY C: Phase 4 — Advanced Features

#### C1. Cross-project pattern detection
**File**: `crates/cortexd/src/engine/rules.rs`
- New rule: `cross_project_pattern`
- When switching projects (context_switch fires), check if the same types of files are being edited across projects
- Example: "You've been editing Cargo.toml across 3 projects today — dependency update day?"
- Track project switches in a simple in-memory buffer

#### C2. Predictive suggestions
**File**: `crates/cortexd/src/engine/rules.rs`
- New rule: `predictive_action`
- Learn from patterns: if after editing file X, the user always runs command Y within 5 minutes, suggest Y
- Track (file_save → command_run) pairs in a frequency table
- When file X is saved and pair strength > 5, generate: "You usually run '{cmd}' after editing {file}"

#### C3. Integration with Claude Code agent sessions
**File**: `crates/cortexd/src/watchers/terminal.rs`
- Detect when `claude` CLI is running (command starts with "claude")
- Track the session: start time, duration, files mentioned in output
- Generate insight on completion: "Claude Code session lasted {N}m, touched {files}"
- Store as a special event type (add `ClaudeSession` to EventType enum if needed)

---

## Team Structure Recommendation

### Team 1: "Gaps & Core" (Category A — sequential, touches shared files)
- A1, A2, A3, A4, A5, A6, A7
- These touch core daemon files (main.rs, server.rs, store.rs, rules.rs) so should be done by one workstream to avoid conflicts
- Do A7 (CI) first in parallel since it's independent

### Team 2: "UX & Polish" (Category B — mostly parallel)
- B1, B2, B3 can run in parallel (different files)
- B4 depends on TUI being fixed (A1)
- B5, B6 can run in parallel with each other

### Team 3: "Advanced" (Category C — parallel, mostly rules.rs)
- C1, C2 both touch rules.rs so do sequentially
- C3 touches terminal.rs (independent)

## Build & Test Commands
```bash
cargo build                    # full workspace build
cargo check -p cortexd         # check daemon only
cargo check -p cortex          # check CLI only
cargo check -p cortex-common   # check shared types only
cargo run --bin cortexd &      # start daemon
cargo run --bin cortex -- status   # test CLI
cargo run --bin cortex -- tui      # test TUI
```

## Key Files Reference
```
crates/cortex-common/src/
  protocol.rs          # Request/Response enums (IPC types)
  events.rs            # CortexEvent, EventType, EventSource
  models.rs            # FileNode, Insight, Pattern, etc.

crates/cortexd/src/
  main.rs              # Daemon entry, task spawning
  config.rs            # CortexConfig, paths
  bus.rs               # EventBus (broadcast channel)
  server.rs            # Unix socket server, request handlers
  watchers/
    mod.rs             # WatcherManager
    filesystem.rs      # notify-based FS watcher
    terminal.rs        # Named pipe reader for shell events
    git.rs             # git2 polling watcher
    editor.rs          # STUB — needs implementation
  graph/
    mod.rs             # KnowledgeGraph facade
    store.rs           # SQLite operations
    embeddings.rs      # fastembed BGE-small integration
    migrations.rs      # Schema creation
    models.rs          # Row conversion helpers
  engine/
    mod.rs             # InferenceEngine orchestrator
    trigger.rs         # TriggerEvaluator
    rules.rs           # LocalRules (Tier 1)
    ranker.rs          # InsightRanker
    claude.rs          # ClaudeClient (Tier 2)

crates/cortex/src/
  main.rs              # CLI entry, clap
  commands/
    mod.rs             # send_request() helper
    status.rs          # cortex status
    query.rs           # cortex query <file>
    history.rs         # cortex history
    search.rs          # cortex search <query>
    install.rs         # cortex install
    tui.rs             # cortex tui (ratatui)
    config.rs          # cortex config

hooks/
  cortex-preexec.zsh   # Shell integration
  cortex-preexec.bash  # Shell integration
  cortex-git-post-commit  # Git hook script
```

## Rules
- NEVER add Co-Authored-By to commits
- Git: joshuadmix@gmail.com / Joshua Mix
- Commit after each logical unit of work
- Push to main when done
- `cargo build` must pass clean after every change
