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
