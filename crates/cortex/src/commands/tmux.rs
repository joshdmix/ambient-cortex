use anyhow::Result;

/// Output a short status string suitable for embedding in a tmux status bar.
///
/// Reads `~/.local/share/cortex/current_insight.json` and prints the title
/// (truncated to 40 characters) prefixed with a lightning bolt, or a fallback
/// idle message when no insight is available.
pub fn run() -> Result<()> {
    let path = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".local/share"))
        .join("cortex")
        .join("current_insight.json");

    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) {
            if let Some(title) = value.get("title").and_then(|t| t.as_str()) {
                if !title.is_empty() {
                    let truncated: String = title.chars().take(40).collect();
                    println!("⚡ {truncated}");
                    return Ok(());
                }
            }
        }
    }

    println!("cortex: idle");
    Ok(())
}
