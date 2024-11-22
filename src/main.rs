use anyhow::Result;

mod cache;
mod display;
mod error;
mod git;
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
    is_binary: bool,
    summary: Option<String>,
}

#[tokio::main]
async fn run() -> Result<()> {
    // Ensure we have the API key
    let api_key = std::env::var(strings::ANTHROPIC_API_KEY)
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY environment variable not set"))?;

    // Initialize repositories and services
    let repo = git::Repository::open_current_directory(None)?;
    let status = repo.get_status()?;
    let summarizer = ClaudeSummarizer::new()?;

    // Process each file and generate summaries
    let mut files_with_summaries = Vec::new();

    for entry in status.entries {
        let summary = if !entry.is_binary {
            if let Some(diff) = repo.get_diff(&entry)? {
                Some(summarizer.summarize(&diff).await?)
            } else {
                None
            }
        } else {
            None
        };

        files_with_summaries.push(FileWithSummary {
            path: entry.display_path.clone(),
            status: entry.status,
            staged: entry.staged,
            original_path: entry.original_path,
            is_binary: entry.is_binary,
            summary,
        });
    }

    // Display the results
    let formatter = display::StatusFormatter::new();
    formatter.display_with_summaries(&files_with_summaries)?;

    Ok(())
}

fn main() -> Result<()> {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
