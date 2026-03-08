use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

fn config_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("cortex")
        .join("config.toml")
}

pub fn show() -> Result<()> {
    let path = config_file_path();

    if !path.exists() {
        println!("No config file found at {}", path.display());
        println!("Run `cortex install` to create a default config.");
        return Ok(());
    }

    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read config: {}", path.display()))?;

    println!("Config: {}\n", path.display());
    println!("{}", contents);

    Ok(())
}

pub fn edit() -> Result<()> {
    let path = config_file_path();

    if !path.exists() {
        bail!(
            "no config file found at {}. Run `cortex install` first.",
            path.display()
        );
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("failed to open editor: {}", editor))?;

    Ok(())
}
