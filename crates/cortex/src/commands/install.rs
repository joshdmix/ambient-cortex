use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

pub fn run() -> Result<()> {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(".local/share"))
        .join("cortex");
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("cortex");

    // Create directories
    println!("Creating directories...");
    fs::create_dir_all(&data_dir)
        .with_context(|| format!("failed to create data dir: {}", data_dir.display()))?;
    println!("  {}", data_dir.display());

    fs::create_dir_all(&config_dir)
        .with_context(|| format!("failed to create config dir: {}", config_dir.display()))?;
    println!("  {}", config_dir.display());

    // Create default config if not exists
    let config_file = config_dir.join("config.toml");
    if !config_file.exists() {
        println!("Creating default config...");
        let default_config = r#"# Cortex configuration

# Directories to watch for file changes
# watch_dirs = ["~/projects"]

# Glob patterns to exclude from watching
# exclude_patterns = ["**/target/**", "**/node_modules/**", "**/.git/**"]

# How many days to retain event data
retention_days = 90

# Claude AI integration (for automatic insights)
claude_enabled = false
# claude_api_key = "sk-..."
claude_max_calls_per_hour = 10

# Minimum relevance score for generating insights
insight_threshold = 0.6

# Debounce time for filesystem events (ms)
debounce_ms = 500
"#;
        fs::write(&config_file, default_config)
            .with_context(|| format!("failed to write config: {}", config_file.display()))?;
        println!("  {}", config_file.display());
    } else {
        println!("Config already exists at {}", config_file.display());
    }

    // Create named pipe directory (pipe created by daemon on startup)
    let pipe_dir = data_dir.clone();
    fs::create_dir_all(&pipe_dir)?;

    // Create shell hook script
    let hook_path = config_dir.join("shell-hook.zsh");
    let hook_script = r#"# Cortex shell hook — source this in your .zshrc
# Sends terminal events to the cortex daemon via named pipe

_cortex_pipe="$HOME/.local/share/cortex/terminal.pipe"

cortex_preexec() {
    if [[ -p "$_cortex_pipe" ]]; then
        printf '{"event_type":"command_run","source":"terminal","payload":{"cmd":"%s","pwd":"%s"}}\n' \
            "${1//\"/\\\"}" "$PWD" > "$_cortex_pipe" 2>/dev/null
    fi
}

cortex_precmd() {
    local exit_code=$?
    if [[ $exit_code -ne 0 ]] && [[ -p "$_cortex_pipe" ]]; then
        printf '{"event_type":"command_fail","source":"terminal","payload":{"exit_code":%d,"pwd":"%s"}}\n' \
            "$exit_code" "$PWD" > "$_cortex_pipe" 2>/dev/null
    fi
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec cortex_preexec
add-zsh-hook precmd cortex_precmd
"#;
    fs::write(&hook_path, hook_script)
        .with_context(|| format!("failed to write shell hook: {}", hook_path.display()))?;
    println!("  Shell hook written to {}", hook_path.display());

    // Install git hooks in repos found under watch_dirs
    println!("\nInstalling git hooks...");
    let watch_dirs = cortexd_config_watch_dirs();
    let repos = find_git_repos(&watch_dirs);
    if repos.is_empty() {
        println!("  No git repos found in watch directories.");
    } else {
        for repo_path in &repos {
            install_git_hooks(repo_path)?;
        }
    }

    // Print setup instructions
    println!("\nInstallation complete!\n");
    println!("To activate the shell hook, add this line to your ~/.zshrc:\n");
    println!("  source \"{}\"", hook_path.display());
    println!();
    println!("Then start the daemon:");
    println!("  cortexd &");
    println!();

    Ok(())
}

/// Read watch_dirs from config, falling back to ~/projects.
fn cortexd_config_watch_dirs() -> Vec<PathBuf> {
    let config_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("cortex")
        .join("config.toml");
    if let Ok(contents) = fs::read_to_string(&config_path) {
        if let Ok(config) = toml::from_str::<toml::Value>(&contents) {
            if let Some(dirs) = config.get("watch_dirs").and_then(|v| v.as_array()) {
                return dirs
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| {
                        if s.starts_with('~') {
                            dirs::home_dir()
                                .unwrap_or_else(|| PathBuf::from("."))
                                .join(&s[2..])
                        } else {
                            PathBuf::from(s)
                        }
                    })
                    .collect();
            }
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    vec![home.join("projects")]
}

fn find_git_repos(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        if dir.join(".git").exists() {
            repos.push(dir.clone());
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".git").exists() {
                    repos.push(path);
                }
            }
        }
    }
    repos
}

fn install_git_hooks(repo_path: &PathBuf) -> Result<()> {
    let hooks_dir = repo_path.join(".git").join("hooks");
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir)?;
    }

    let pipe_path = "${XDG_RUNTIME_DIR:-/tmp}/cortex/git.pipe";

    // Post-commit hook
    let post_commit_path = hooks_dir.join("post-commit");
    let post_commit_script = format!(
        r#"#!/usr/bin/env bash
# Ambient Cortex — git post-commit hook
PIPE="{pipe_path}"
[[ -p "$PIPE" ]] || exit 0

HASH=$(git rev-parse HEAD 2>/dev/null)
MSG=$(git log -1 --format='%s' 2>/dev/null)
FILES=$(git diff-tree --no-commit-id --name-only -r HEAD 2>/dev/null | tr '\n' ',' | sed 's/,$//')
BRANCH=$(git branch --show-current 2>/dev/null)

printf '{{"type":"git_commit","hash":"%s","message":"%s","files":"%s","branch":"%s"}}\n' \
    "$HASH" "${{MSG//\"/\\\"}}""$FILES" "$BRANCH" \
    > "$PIPE" 2>/dev/null
"#
    );

    // Post-checkout hook
    let post_checkout_path = hooks_dir.join("post-checkout");
    let post_checkout_script = format!(
        r#"#!/usr/bin/env bash
# Ambient Cortex — git post-checkout hook
PIPE="{pipe_path}"
[[ -p "$PIPE" ]] || exit 0

PREV_HEAD="$1"
NEW_HEAD="$2"
BRANCH_FLAG="$3"
BRANCH=$(git branch --show-current 2>/dev/null)

printf '{{"type":"git_checkout","prev_head":"%s","new_head":"%s","branch":"%s","branch_flag":"%s"}}\n' \
    "$PREV_HEAD" "$NEW_HEAD" "$BRANCH" "$BRANCH_FLAG" \
    > "$PIPE" 2>/dev/null
"#
    );

    write_hook(&post_commit_path, &post_commit_script, repo_path)?;
    write_hook(&post_checkout_path, &post_checkout_script, repo_path)?;

    Ok(())
}

fn write_hook(hook_path: &PathBuf, script: &str, repo_path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let hook_name = hook_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    if hook_path.exists() {
        let existing = fs::read_to_string(hook_path)?;
        if existing.contains("Ambient Cortex") {
            println!("  {} {}: already installed, updating", repo_path.display(), hook_name);
            fs::write(hook_path, script)?;
        } else {
            // Append to existing hook
            println!(
                "  {} {}: appending to existing hook",
                repo_path.display(),
                hook_name
            );
            let combined = format!("{}\n\n{}", existing.trim_end(), script);
            fs::write(hook_path, combined)?;
        }
    } else {
        println!("  {} {}: installed", repo_path.display(), hook_name);
        fs::write(hook_path, script)?;
    }

    // chmod +x
    let perms = fs::Permissions::from_mode(0o755);
    fs::set_permissions(hook_path, perms)?;

    Ok(())
}
