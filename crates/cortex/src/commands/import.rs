use anyhow::Result;
use cortex_common::protocol::{Request, Response};

use super::send_request;

pub async fn run(input: &str) -> Result<()> {
    let data = std::fs::read_to_string(input)?;

    match send_request(Request::Import { data }).await? {
        Response::Ok => {
            println!("Imported data from {}", input);
        }
        Response::Error(e) => {
            eprintln!("Import failed: {}", e);
        }
        _ => {
            eprintln!("Unexpected response from daemon");
        }
    }
    Ok(())
}
