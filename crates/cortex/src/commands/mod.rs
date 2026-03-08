pub mod config;
pub mod history;
pub mod install;
pub mod query;
pub mod search;
pub mod status;
pub mod tui;

use anyhow::{Context, Result};
use cortex_common::protocol::{Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Connect to the cortexd daemon over the Unix socket, send a request, and read the response.
pub async fn send_request(request: Request) -> Result<Response> {
    let socket_path = cortexd_socket_path();

    let stream = UnixStream::connect(&socket_path)
        .await
        .with_context(|| {
            format!(
                "failed to connect to cortexd at {}. Is the daemon running? Try `cortexd &`",
                socket_path.display()
            )
        })?;

    let (reader, mut writer) = stream.into_split();

    let mut payload = serde_json::to_vec(&request)?;
    payload.push(b'\n');
    writer.write_all(&payload).await?;
    writer.shutdown().await?;

    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();
    buf_reader
        .read_line(&mut line)
        .await
        .context("failed to read response from cortexd")?;

    let response: Response =
        serde_json::from_str(line.trim()).context("failed to parse daemon response")?;

    Ok(response)
}

fn cortexd_socket_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".local/share"))
        .join("cortex")
        .join("cortexd.sock")
}
