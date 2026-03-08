# Ambient Cortex

Local-first daemon that watches your dev activity and surfaces contextual insights. Built in Rust.

Watches terminal commands, file changes, and git activity. Builds a knowledge graph. Tells you things like "you usually edit config.rs when you change main.rs" or "this branch was last active 12 days ago."

## Quick start

```bash
cargo build --release
./target/release/cortexd &          # start daemon
./target/release/cortex install     # install shell/git hooks
source ~/.config/cortex/cortex-preexec.zsh
```

## Commands

```
cortex status          cortex tui             cortex query <file>
cortex history         cortex search <query>  cortex config
cortex export          cortex import          cortex tmux-status
```

## Config

`~/.config/cortex/config.toml` — watch dirs, retention days, optional Claude API, macOS notifications.

All data stays local. No telemetry.
