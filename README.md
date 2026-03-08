# Ambient Cortex

A local-first daemon that passively observes your development activity and surfaces contextual insights. Built in Rust.

## What it does

Cortex watches your terminal commands, file changes, and git activity, builds a knowledge graph of your workflow patterns, and proactively surfaces insights like:

- "You usually edit `config.rs` when you change `main.rs`"
- "This branch was last active 12 days ago — you were fixing the payment flow"
- "You've been iterating on this file for 15 minutes with failures"
- "You usually run `cargo test` after editing this file"

## Architecture

```
CLI (cortex)  <-- Unix socket -->  Daemon (cortexd)
                                     |
                    +--------+-------+-------+--------+
                    |        |       |       |        |
                 Terminal   FS     Git    Editor   Inference
                 Watcher  Watcher Watcher Watcher  Engine
                                     |
                              Knowledge Graph
                            (SQLite + embeddings)
```

Three crates: `cortex-common` (shared types), `cortexd` (daemon), `cortex` (CLI).

## Install

```bash
cargo build --release
# Start the daemon
./target/release/cortexd &
# Install shell hooks and git hooks
./target/release/cortex install
```

Add to your `.zshrc`:
```bash
source ~/.config/cortex/cortex-preexec.zsh
```

## Usage

```bash
cortex status              # daemon health
cortex tui                 # interactive dashboard
cortex query src/main.rs   # file context and relationships
cortex history             # recent activity
cortex search "auth bug"   # semantic search
cortex export --output backup.json
cortex import --input backup.json
cortex tmux-status         # one-liner for tmux status bar
cortex config              # view config
```

### TUI keybindings

| Key | Action |
|-----|--------|
| `j/k` | scroll |
| `Tab` | switch panel |
| `g` | file relationship graph |
| `d` | dismiss insight |
| `r` | refresh |
| `q` | quit |

## Configuration

`~/.config/cortex/config.toml`:

```toml
watch_dirs = ["~/projects"]
retention_days = 90
claude_enabled = false          # opt-in Claude API for richer insights
notifications_enabled = true    # macOS notifications for high-relevance insights
```

## Privacy

- All data stored locally (`~/.local/share/cortex/`)
- Embeddings computed locally (fastembed, no network)
- Claude API is opt-in and only receives summaries, never raw file contents
- No telemetry, no cloud sync
