use anyhow::Result;

mod cache;
mod display;
mod error;
mod git;
mod summary;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

async fn run() -> Result<()> {
    let repo = git::Repository::open_current_directory()?;
    let status = repo.get_status()?;

    let formatter = display::StatusFormatter::new();
    formatter.display(&status)?;

    Ok(())
}
