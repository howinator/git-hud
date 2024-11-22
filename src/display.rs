use crate::git::{Status, StatusCode};
use crate::FileWithSummary;
use anyhow::Result;
use colored::*;
use std::process::Command;

pub struct StatusFormatter;

impl StatusFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn display(&self, status: &Status) -> Result<()> {
        // Get branch information
        self.print_branch_status()?;

        let mut has_staged = false;
        let mut has_unstaged = false;
        let mut has_untracked = false;

        // Categorize changes
        for entry in &status.entries {
            match entry.status {
                StatusCode::Untracked => has_untracked = true,
                _ if entry.staged => has_staged = true,
                _ => has_unstaged = true,
            }
        }

        // Print sections in git's order
        if has_staged {
            self.print_staged_changes(status)?;
        }

        if has_unstaged {
            self.print_unstaged_changes(status)?;
        }

        if has_untracked {
            self.print_untracked_files(status)?;
        }

        // Print summary line if needed
        if !has_staged && has_unstaged {
            println!("\nno changes added to commit (use \"git add\" and/or \"git commit -a\")");
        }

        Ok(())
    }

    fn print_branch_status(&self) -> Result<()> {
        // Get current branch name
        let branch_output = Command::new("git")
            .args(["branch", "--show-current"])
            .output()?;

        let branch_name = String::from_utf8(branch_output.stdout)?.trim().to_string();

        println!("On branch {}", branch_name);

        // Get remote tracking info
        let remote_output = Command::new("git").args(["status", "-sb"]).output()?;

        let remote_status = String::from_utf8(remote_output.stdout)?;

        // Parse remote status line
        if let Some(remote_line) = remote_status.lines().next() {
            if remote_line.contains("[") {
                let parts: Vec<&str> = remote_line.splitn(2, "[").collect();
                if let Some(remote_info) = parts.get(1) {
                    let remote_status = remote_info.trim_end_matches(']');
                    println!("Your branch is {}", remote_status);
                }
            } else if !branch_name.is_empty() {
                println!("Your branch is not tracking a remote branch.");
            }
        }

        println!();
        Ok(())
    }

    fn print_staged_changes(&self, status: &Status) -> Result<()> {
        println!("Changes to be committed:");
        println!("  (use \"git restore --staged <file>...\" to unstage)");

        for entry in &status.entries {
            if entry.staged {
                let status_text = self.format_status(&entry.status);
                let path = format!("{}", entry.display_path);

                if let Some(orig_path) = &entry.original_path {
                    println!("\t{}: {} -> {}", status_text.green(), orig_path, path);
                } else {
                    println!("\t{}: {}", status_text.green(), path);
                }
            }
        }
        println!();
        Ok(())
    }

    fn print_unstaged_changes(&self, status: &Status) -> Result<()> {
        println!("Changes not staged for commit:");
        println!("  (use \"git add <file>...\" to update what will be committed)");
        println!("  (use \"git restore <file>...\" to discard changes in working directory)");

        for entry in &status.entries {
            if !entry.staged && !matches!(entry.status, StatusCode::Untracked) {
                let status_text = self.format_status(&entry.status);
                let path = format!("{}", entry.display_path);

                // Here we'd add the summary when implemented
                println!("\t{}: {}", status_text.red(), path);
            }
        }
        println!();
        Ok(())
    }

    fn print_untracked_files(&self, status: &Status) -> Result<()> {
        let untracked: Vec<_> = status
            .entries
            .iter()
            .filter(|e| matches!(e.status, StatusCode::Untracked))
            .collect();

        if !untracked.is_empty() {
            println!("Untracked files:");
            println!("  (use \"git add <file>...\" to include in what will be committed)");

            for entry in untracked {
                println!("\t{}", entry.display_path.red());
            }
            println!();
        }
        Ok(())
    }

    fn format_status(&self, status: &StatusCode) -> &'static str {
        match status {
            StatusCode::Modified => "modified",
            StatusCode::Added => "new file",
            StatusCode::Deleted => "deleted",
            StatusCode::Renamed => "renamed",
            StatusCode::Copied => "copied",
            StatusCode::Unmerged => "unmerged",
            StatusCode::Untracked => "untracked",
            StatusCode::Ignored => "ignored",
        }
    }

    pub fn display_with_summaries(&self, files: &[FileWithSummary]) -> Result<()> {
        self.print_branch_status()?;

        let mut has_staged = false;
        let mut has_unstaged = false;
        let mut has_untracked = false;

        for file in files {
            match file.status {
                StatusCode::Untracked => has_untracked = true,
                _ if file.staged => has_staged = true,
                _ => has_unstaged = true,
            }
        }

        if has_staged {
            println!("Changes to be committed:");
            println!("  (use \"git restore --staged <file>...\" to unstage)");

            for file in files {
                if file.staged {
                    let status_text = self.format_status(&file.status);

                    if let Some(ref orig_path) = file.original_path {
                        print!("\t{}: {} -> {}", status_text.green(), orig_path, file.path);
                    } else {
                        print!("\t{}: {}", status_text.green(), file.path);
                    }

                    // Add summary if available
                    if let Some(ref summary) = file.summary {
                        println!(" ({})", summary);
                    } else {
                        println!();
                    }
                }
            }
            println!();
        }

        if has_unstaged {
            println!("Changes not staged for commit:");
            println!("  (use \"git add <file>...\" to update what will be committed)");
            println!("  (use \"git restore <file>...\" to discard changes in working directory)");

            for file in files {
                if !file.staged && !matches!(file.status, StatusCode::Untracked) {
                    let status_text = self.format_status(&file.status);
                    print!("\t{}: {}", status_text.red(), file.path);

                    // Add summary if available
                    if let Some(ref summary) = file.summary {
                        println!(" ({})", summary);
                    } else {
                        println!();
                    }
                }
            }
            println!();
        }

        if has_untracked {
            println!("Untracked files:");
            println!("  (use \"git add <file>...\" to include in what will be committed)");

            for file in files {
                if matches!(file.status, StatusCode::Untracked) {
                    println!("\t{}", file.path.red());
                    if let Some(ref summary) = file.summary {
                        println!("\t  ({})", summary);
                    }
                }
            }
            println!();
        }

        if !has_staged && has_unstaged {
            println!("no changes added to commit (use \"git add\" and/or \"git commit -a\")");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Repository;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_repo() -> Result<(TempDir, Repository)> {
        let temp_dir = TempDir::new()?;

        // Initialize git repo
        Command::new("git")
            .args(&["init"])
            .current_dir(temp_dir.path())
            .output()?;

        // Configure git user for commits
        Command::new("git")
            .args(&["config", "user.name", "test"])
            .current_dir(temp_dir.path())
            .output()?;
        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(temp_dir.path())
            .output()?;

        let repo = Repository::open_current_directory(temp_dir.path().to_str())?;
        Ok((temp_dir, repo))
    }

    #[test]
    fn test_status_display() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create some files with different states
        fs::write(temp_dir.path().join("staged.txt"), "staged content\n")?;
        fs::write(temp_dir.path().join("unstaged.txt"), "unstaged content\n")?;
        fs::write(temp_dir.path().join("untracked.txt"), "untracked content\n")?;

        // Stage one file
        Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(temp_dir.path())
            .output()?;

        let formatter = StatusFormatter::new();
        let status = repo.get_status()?;

        // Redirect stdout to capture output
        let mut output = Vec::new();
        {
            use std::io::Write;
            let mut cursor = std::io::Cursor::new(&mut output);
            std::io::stdout().flush()?;

            formatter.display(&status)?;
        }

        let output = String::from_utf8(output)?;

        // Verify output format
        assert!(output.contains("On branch"));
        assert!(output.contains("Changes to be committed:"));
        assert!(output.contains("new file:   staged.txt"));
        assert!(output.contains("Untracked files:"));
        assert!(output.contains("untracked.txt"));

        Ok(())
    }

    #[test]
    fn test_branch_status() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and commit a file
        fs::write(temp_dir.path().join("test.txt"), "content\n")?;
        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(temp_dir.path())
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(temp_dir.path())
            .output()?;

        let formatter = StatusFormatter::new();

        // Capture output
        let mut output = Vec::new();
        {
            let mut cursor = std::io::Cursor::new(&mut output);
            formatter.print_branch_status()?;
        }

        let output = String::from_utf8(output)?;

        // Verify branch information
        assert!(output.contains("On branch"));
        // Note: We don't check for specific branch name as it might vary

        Ok(())
    }
}
