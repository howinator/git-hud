use anyhow::Result;
use futures::future::try_join_all;
use std::time::Instant;

mod display;
mod error;
mod git;
mod log;
mod strings;
mod summary;

use crate::summary::Summarizer;
use git::StatusCode;
use summary::ClaudeSummarizer;

struct FileWithSummary {
    path: String,
    status: StatusCode,
    staged: bool,
    original_path: Option<String>,
    summary: Option<String>,
}

#[tokio::main]
async fn run() -> Result<()> {
    // Ensure we have the API key
    let _api_key = std::env::var(strings::ANTHROPIC_API_KEY)
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY environment variable not set"))?;

    let t0 = Instant::now();
    // Initialize repositories and services
    let repo = git::Repository::open_current_directory(None)?;
    log::log_duration("Open repo", &t0.elapsed());
    let t1 = Instant::now();
    let status = repo.get_status()?;
    log::log_duration("Get status", &t1.elapsed());
    let summarizer = ClaudeSummarizer::new()?;

    let t3 = Instant::now();
    // Process each file and generate summaries
    let summary_futures: Vec<_> = status
        .entries
        .iter()
        .map(|entry| async {
            let summary = match entry.is_binary {
                true => None,
                false => match repo.get_diff(entry)? {
                    Some(diff) => Some(summarizer.summarize(&diff).await?),
                    None => None,
                },
            };
            Ok::<_, anyhow::Error>(FileWithSummary {
                path: entry.display_path.clone(),
                status: entry.status.clone(),
                staged: entry.staged,
                original_path: entry.original_path.clone(),
                summary,
            })
        })
        .collect();
    log::log_duration("Create requests", &t3.elapsed());

    let t4 = Instant::now();
    let files_with_summaries = try_join_all(summary_futures).await?;
    log::log_duration("Join requests", &t4.elapsed());

    let t5 = Instant::now();
    // Display the results
    let formatter = display::StatusFormatter::new();
    formatter.display_with_summaries(&files_with_summaries)?;

    log::log_duration("Display", &t5.elapsed());
    Ok(())
}

fn main() -> Result<()> {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
