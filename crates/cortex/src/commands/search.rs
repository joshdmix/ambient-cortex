use anyhow::{bail, Result};
use cortex_common::protocol::{Request, Response};

use super::send_request;

pub async fn run(query: &str) -> Result<()> {
    let response = send_request(Request::Search {
        query: query.to_string(),
    })
    .await?;

    match response {
        Response::SearchResult(hits) => {
            if hits.is_empty() {
                println!("No results found for \"{}\".", query);
                return Ok(());
            }

            println!("Search results for \"{}\":\n", query);
            println!(
                "{:<10} {:<14} Text",
                "Relevance", "Type"
            );
            println!("{}", "-".repeat(60));

            for hit in &hits {
                println!(
                    "{:<10.2} {:<14} {}",
                    hit.relevance, hit.source_type, hit.text,
                );
            }

            println!("\n{} result(s) found.", hits.len());
        }
        Response::Error(msg) => bail!("daemon error: {}", msg),
        _ => bail!("unexpected response from daemon"),
    }

    Ok(())
}
