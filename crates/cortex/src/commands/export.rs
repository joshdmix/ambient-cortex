use anyhow::Result;
use cortex_common::protocol::{Request, Response};

use super::send_request;

pub async fn run(output: &str) -> Result<()> {
    match send_request(Request::Export).await? {
        Response::ExportResult(data) => {
            std::fs::write(output, &data)?;
            println!("Exported data to {}", output);
        }
        Response::Error(e) => {
            eprintln!("Export failed: {}", e);
        }
        _ => {
            eprintln!("Unexpected response from daemon");
        }
    }
    Ok(())
}
