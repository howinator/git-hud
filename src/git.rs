use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;

pub struct Repository {
    repo: git2::Repository,
}

#[derive(Debug, Clone)]
pub enum StatusCode {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
    Untracked,
    Ignored,
}

impl FromStr for StatusCode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "M" => Ok(StatusCode::Modified),
            "A" => Ok(StatusCode::Added),
            "D" => Ok(StatusCode::Deleted),
            "R" => Ok(StatusCode::Renamed),
            "C" => Ok(StatusCode::Copied),
            "U" => Ok(StatusCode::Unmerged),
            "?" => Ok(StatusCode::Untracked),
            "!" => Ok(StatusCode::Ignored),
            _ => Err(anyhow::anyhow!("Invalid status code: {}", s)),
        }
    }
}

#[derive(Debug)]
pub struct StatusEntry {
    pub path: String,
    pub status: StatusCode,
    pub staged: bool,
    pub original_path: Option<String>,
    pub is_binary: bool,
}

pub struct Status {
    pub entries: Vec<StatusEntry>,
}
impl Repository {
    pub fn open_current_directory() -> Result<Self> {
        let repo = git2::Repository::open(".")?;
        Ok(Self { repo })
    }

    pub fn get_status(&self) -> Result<Status> {
        let mut cmd = std::process::Command::new("git");
        cmd.args(["status", "--porcelain=v2", "-z"]); // -z for handling filenames with spaces
        let output = cmd.output().context("Failed to execute git status")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "git status failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let output =
            String::from_utf8(output.stdout).context("Git status output was not valid UTF-8")?;

        let mut entries = Vec::new();

        for line in output.split('\0') {
            if line.is_empty() {
                continue;
            }

            let entry = self
                .parse_status_line(line)
                .with_context(|| format!("Failed to parse status line: {}", line))?;

            if let Some(entry) = entry {
                // Check if the file is binary
                let is_binary = if !matches!(entry.status, StatusCode::Deleted) {
                    self.is_file_binary(&entry.path)?
                } else {
                    false
                };

                if !is_binary {
                    entries.push(StatusEntry { is_binary, ..entry });
                }
            }
        }

        Ok(Status { entries })
    }

    // Uses the grep heuristic for whether a file is binary
    fn is_file_binary(&self, path: &str) -> Result<bool> {
        // Skip if file doesn't exist (e.g., deleted files)
        if !Path::new(path).exists() {
            return Ok(false);
        }

        let output = Command::new("grep")
            .args(["-Hm1", "^"])
            .arg(path)
            .env("LC_MESSAGES", "C") // Force English output
            .output()
            .context("Failed to execute grep")?;

        // grep will output "Binary file <filename> matches" for binary files
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(stderr.contains("Binary file")
            || String::from_utf8_lossy(&output.stdout).contains("Binary file"))
    }

    fn parse_status_line(&self, line: &str) -> Result<Option<StatusEntry>> {
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.is_empty() {
            return Ok(None);
        }

        match parts[0] {
            // Regular changed entry
            "1" | "2" => {
                if parts.len() < 3 {
                    return Err(anyhow::anyhow!("Invalid status line format"));
                }

                let xy = parts[1];
                // Join remaining parts to handle spaces in filenames
                let path = parts[2..].join(" ");

                let staged = xy.chars().nth(0).map(|c| c != '.').unwrap_or(false);
                let status = if let Some(code) = xy.chars().nth(1) {
                    if code == '.' {
                        xy.chars().nth(0).unwrap().to_string()
                    } else {
                        code.to_string()
                    }
                } else {
                    return Err(anyhow::anyhow!("Invalid status code format"));
                };

                Ok(Some(StatusEntry {
                    path,
                    status: StatusCode::from_str(&status)?,
                    staged,
                    original_path: None,
                    is_binary: false, // Will be set later
                }))
            }

            "R" | "C" => {
                if parts.len() < 4 {
                    return Err(anyhow::anyhow!("Invalid rename/copy line format"));
                }

                let status = if parts[0] == "R" {
                    StatusCode::Renamed
                } else {
                    StatusCode::Copied
                };

                let score = parts[1];
                let original = parts[2..parts.len() - 1].join(" ");
                let new = parts[parts.len() - 1].to_string();

                Ok(Some(StatusEntry {
                    path: new,
                    status,
                    staged: true,
                    original_path: Some(original),
                    is_binary: false, // Will be set later
                }))
            }

            "u" => {
                if parts.len() < 2 {
                    return Err(anyhow::anyhow!("Invalid unmerged line format"));
                }

                Ok(Some(StatusEntry {
                    path: parts[1..].join(" "),
                    status: StatusCode::Unmerged,
                    staged: false,
                    original_path: None,
                    is_binary: false, // Will be set later
                }))
            }

            "?" => {
                Ok(Some(StatusEntry {
                    path: parts[1..].join(" "),
                    status: StatusCode::Untracked,
                    staged: false,
                    original_path: None,
                    is_binary: false, // Will be set later
                }))
            }

            "!" => Ok(None),

            _ => Ok(None),
        }
    }
    pub fn get_diff(&self, entry: &StatusEntry) -> Result<Option<String>> {
        // Skip binary files early
        if entry.is_binary {
            return Ok(None);
        }

        match entry.status {
            StatusCode::Untracked => {
                // For untracked files, show the entire file as added
                let content = std::fs::read_to_string(&entry.path)
                    .context("Failed to read untracked file")?;
                Ok(Some(format!("+{}", content.lines().collect::<Vec<_>>().join("\n+"))))
            }
            StatusCode::Deleted => {
                // For deleted files, show what was deleted using git show
                let output = Command::new("git")
                    .args(["show", &format!("HEAD:{}", entry.path)])
                    .output()
                    .context("Failed to execute git show")?;

                if output.status.success() {
                    let content = String::from_utf8(output.stdout)
                        .context("Invalid UTF-8 in git show output")?;
                    Ok(Some(format!("-{}", content.lines().collect::<Vec<_>>().join("\n-"))))
                } else {
                    Ok(None)
                }
            }
            StatusCode::Renamed | StatusCode::Copied => {
                if let Some(ref old_path) = entry.original_path {
                    let output = Command::new("git")
                        .args([
                            "diff",
                            "--no-color",
                            "--no-prefix",
                            old_path,
                            &entry.path
                        ])
                        .output()
                        .context("Failed to execute git diff for renamed file")?;

                    if output.status.success() {
                        String::from_utf8(output.stdout)
                            .context("Invalid UTF-8 in git diff output")
                            .map(Some)
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            StatusCode::Unmerged => {
                let output = Command::new("git")
                    .args([
                        "diff",
                        "--no-color",
                        "--no-prefix",
                        "--diff-filter=U",
                        &entry.path
                    ])
                    .output()
                    .context("Failed to execute git diff for unmerged file")?;

                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .context("Invalid UTF-8 in git diff output")
                        .map(Some)
                } else {
                    Ok(None)
                }
            }
            _ => {
                // For modified/added files, use git diff with appropriate flags
                let mut args = vec!["diff", "--no-color", "--no-prefix"];

                if entry.staged {
                    args.push("--cached");
                }

                args.push(&entry.path);

                let output = Command::new("git")
                    .args(&args)
                    .output()
                    .context("Failed to execute git diff")?;

                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .context("Invalid UTF-8 in git diff output")
                        .map(Some)
                } else {
                    Ok(None)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::process::Command;
    use tempfile::TempDir;


    pub fn setup_test_repo() -> Result<(TempDir, Repository)> {
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

        let repo = Repository::open_current_directory()?;
        Ok((temp_dir, repo))
    }

    #[test]
    fn test_basic_status() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and add a new file
        fs::write(temp_dir.path().join("new.txt"), "content")?;
        Command::new("git")
            .args(&["add", "new.txt"])
            .current_dir(temp_dir.path())
            .output()?;

        let status = repo.get_status()?;
        let entry = status.entries.first().unwrap();
        assert!(matches!(entry.status, StatusCode::Added));
        assert!(entry.staged);
        assert_eq!(entry.path, "new.txt");

        Ok(())
    }

    #[test]
    fn test_space_in_filename() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create file with spaces
        fs::write(temp_dir.path().join("file with spaces.txt"), "content")?;

        let status = repo.get_status()?;
        let entry = status.entries.first().unwrap();
        assert!(matches!(entry.status, StatusCode::Untracked));
        assert_eq!(entry.path, "file with spaces.txt");

        Ok(())
    }

    #[test]
    fn test_merge_conflict() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create initial file and commit
        fs::write(temp_dir.path().join("conflict.txt"), "master content")?;
        Command::new("git")
            .args(&["add", "conflict.txt"])
            .current_dir(temp_dir.path())
            .output()?;
        Command::new("git")
            .args(&["commit", "-m", "initial"])
            .current_dir(temp_dir.path())
            .output()?;

        // Create and checkout new branch
        Command::new("git")
            .args(&["checkout", "-b", "feature"])
            .current_dir(temp_dir.path())
            .output()?;

        // Modify file in feature branch
        fs::write(temp_dir.path().join("conflict.txt"), "feature content")?;
        Command::new("git")
            .args(&["commit", "-am", "feature change"])
            .current_dir(temp_dir.path())
            .output()?;

        // Go back to master and make conflicting change
        Command::new("git")
            .args(&["checkout", "master"])
            .current_dir(temp_dir.path())
            .output()?;
        fs::write(temp_dir.path().join("conflict.txt"), "master new content")?;
        Command::new("git")
            .args(&["commit", "-am", "master change"])
            .current_dir(temp_dir.path())
            .output()?;

        // Try to merge (this will create a conflict)
        Command::new("git")
            .args(&["merge", "feature"])
            .current_dir(temp_dir.path())
            .output()?;

        let status = repo.get_status()?;
        let entry = status
            .entries
            .iter()
            .find(|e| e.path == "conflict.txt")
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Unmerged));

        Ok(())
    }

    #[test]
    fn test_submodule_changes() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and add a submodule (mock it with a new repo)
        fs::create_dir(temp_dir.path().join("sub"))?;
        Command::new("git")
            .args(&["init"])
            .current_dir(temp_dir.path().join("sub"))
            .output()?;
        Command::new("git")
            .args(&["submodule", "add", "./sub"])
            .current_dir(temp_dir.path())
            .output()?;

        // Modify submodule
        fs::write(temp_dir.path().join("sub/file.txt"), "content")?;
        Command::new("git")
            .args(&["add", "file.txt"])
            .current_dir(temp_dir.path().join("sub"))
            .output()?;

        let status = repo.get_status()?;
        let entry = status.entries.iter().find(|e| e.path == "sub").unwrap();
        assert!(matches!(entry.status, StatusCode::Modified));

        Ok(())
    }

    #[test]
    fn test_parse_status_line() {
        let repo = Repository::open_current_directory().unwrap();

        // Test modified file
        let entry = repo
            .parse_status_line("1 .M N... 100644 100644 100644 file.txt")
            .unwrap()
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Modified));
        assert!(!entry.staged);
        assert_eq!(entry.path, "file.txt");

        // Test staged new file
        let entry = repo
            .parse_status_line("1 A. N... 100644 100644 100644 new.txt")
            .unwrap()
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Added));
        assert!(entry.staged);
        assert_eq!(entry.path, "new.txt");

        // Test renamed file
        let entry = repo
            .parse_status_line("R 100 old.txt new.txt")
            .unwrap()
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Renamed));
        assert!(entry.staged);
        assert_eq!(entry.path, "new.txt");
        assert_eq!(entry.original_path, Some("old.txt".to_string()));

        // Test untracked file
        let entry = repo.parse_status_line("? untracked.txt").unwrap().unwrap();
        assert!(matches!(entry.status, StatusCode::Untracked));
        assert!(!entry.staged);
        assert_eq!(entry.path, "untracked.txt");
    }

    #[test]
    fn test_binary_file() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create a text file
        fs::write(temp_dir.path().join("text.txt"), "Hello, World!\n")?;

        // Create a binary file
        let mut file = File::create(temp_dir.path().join("binary.bin"))?;
        file.write_all(&[0u8, 159u8, 146u8, 150u8])?; // Some binary content including null bytes

        // Test individual files
        assert!(!repo.is_file_binary("text.txt")?);
        assert!(repo.is_file_binary("binary.bin")?);

        // Test that binary files are excluded from status
        let status = repo.get_status()?;
        let binary_files: Vec<_> = status
            .entries
            .iter()
            .filter(|e| e.path == "binary.bin")
            .collect();
        assert!(
            binary_files.is_empty(),
            "Binary files should be excluded from status"
        );

        let text_files: Vec<_> = status
            .entries
            .iter()
            .filter(|e| e.path == "text.txt")
            .collect();
        assert!(
            !text_files.is_empty(),
            "Text files should be included in status"
        );

        Ok(())
    }

    #[test]
    fn test_various_binary_files() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Test various binary file types
        let test_files = [
            ("image.png", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A][..]), // PNG header
            ("image.jpg", &[0xFF, 0xD8, 0xFF, 0xE0][..]), // JPEG header
            ("program.exe", &[0x4D, 0x5A, 0x90, 0x00][..]), // EXE header
            ("archive.zip", &[0x50, 0x4B, 0x03, 0x04][..]), // ZIP header
        ];
        for (filename, content) in test_files.iter() {
            let path = temp_dir.path().join(filename);
            let mut file = File::create(&path)?;
            file.write_all(content)?;
            assert!(
                repo.is_file_binary(filename)?,
                "File {} should be detected as binary",
                filename
            );
        }

        // Test text files with various encodings
        let text_files = [
            ("utf8.txt", "Hello, World!"),
            ("empty.txt", ""),
            ("unicode.txt", "Hello, 世界!"),
            ("numbers.txt", "12345\n67890"),
        ];

        for (filename, content) in text_files.iter() {
            let path = temp_dir.path().join(filename);
            fs::write(&path, content)?;
            assert!(
                !repo.is_file_binary(filename)?,
                "File {} should be detected as text",
                filename
            );
        }

        Ok(())
    }

    #[test]
    fn test_edge_cases() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Test file with only newlines
        fs::write(temp_dir.path().join("newlines.txt"), "\n\n\n")?;
        assert!(!repo.is_file_binary("newlines.txt")?);

        // Test file with spaces and special characters in name
        let filename = "special file (with spaces) アイウエオ.txt";
        fs::write(temp_dir.path().join(filename), "content")?;
        assert!(!repo.is_file_binary(filename)?);

        // Test very large text file
        let large_text = "A".repeat(100_000);
        fs::write(temp_dir.path().join("large.txt"), large_text)?;
        assert!(!repo.is_file_binary("large.txt")?);

        // Test file with null bytes in middle
        let mut file = File::create(temp_dir.path().join("mixed.bin"))?;
        file.write_all(b"Start")?;
        file.write_all(&[0u8, 0u8])?;
        file.write_all(b"End")?;
        assert!(repo.is_file_binary("mixed.bin")?);

        Ok(())
    }

    #[test]
    fn test_diff_modified_file() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and commit initial file
        fs::write(temp_dir.path().join("test.txt"), "initial content\n")?;
        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(temp_dir.path())
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp_dir.path())
            .output()?;

        // Modify file
        fs::write(temp_dir.path().join("test.txt"), "modified content\n")?;

        let status = repo.get_status()?;
        let entry = status.entries.first().unwrap();
        let diff = repo.get_diff(entry)?.unwrap();

        assert!(diff.contains("-initial content"));
        assert!(diff.contains("+modified content"));

        Ok(())
    }

    #[test]
    fn test_diff_staged_changes() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and stage a new file
        fs::write(temp_dir.path().join("staged.txt"), "staged content\n")?;
        Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(temp_dir.path())
            .output()?;

        let status = repo.get_status()?;
        let entry = status.entries.first().unwrap();
        let diff = repo.get_diff(entry)?.unwrap();

        assert!(diff.contains("+staged content"));

        Ok(())
    }

    #[test]
    fn test_diff_untracked_file() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create untracked file
        fs::write(temp_dir.path().join("untracked.txt"), "new content\n")?;

        let status = repo.get_status()?;
        let entry = status.entries.first().unwrap();
        let diff = repo.get_diff(entry)?.unwrap();

        assert!(diff.contains("+new content"));

        Ok(())
    }

    #[test]
    fn test_diff_renamed_file() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and commit initial file
        fs::write(temp_dir.path().join("old.txt"), "content\n")?;
        Command::new("git")
            .args(["add", "old.txt"])
            .current_dir(temp_dir.path())
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp_dir.path())
            .output()?;

        // Rename file
        Command::new("git")
            .args(["mv", "old.txt", "new.txt"])
            .current_dir(temp_dir.path())
            .output()?;

        let status = repo.get_status()?;
        let entry = status.entries.first().unwrap();
        let diff = repo.get_diff(entry)?.unwrap();

        assert!(diff.contains("renamed from 'old.txt'"));
        assert!(diff.contains("renamed to 'new.txt'"));

        Ok(())
    }

    #[test]
    fn test_diff_merge_conflict() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and commit initial file
        fs::write(temp_dir.path().join("conflict.txt"), "initial\n")?;
        Command::new("git")
            .args(["add", "conflict.txt"])
            .current_dir(temp_dir.path())
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp_dir.path())
            .output()?;

        // Create and checkout new branch
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(temp_dir.path())
            .output()?;

        // Modify in feature branch
        fs::write(temp_dir.path().join("conflict.txt"), "feature change\n")?;
        Command::new("git")
            .args(["commit", "-am", "feature"])
            .current_dir(temp_dir.path())
            .output()?;

        // Back to master and change
        Command::new("git")
            .args(["checkout", "master"])
            .current_dir(temp_dir.path())
            .output()?;
        fs::write(temp_dir.path().join("conflict.txt"), "master change\n")?;
        Command::new("git")
            .args(["commit", "-am", "master"])
            .current_dir(temp_dir.path())
            .output()?;

        // Try to merge
        let merge_output = Command::new("git")
            .args(["merge", "feature"])
            .current_dir(temp_dir.path())
            .output()?;
        assert!(!merge_output.status.success(), "Merge should create conflict");

        let status = repo.get_status()?;
        let entry = status.entries.iter().find(|e| e.path == "conflict.txt").unwrap();
        let diff = repo.get_diff(entry)?.unwrap();

        assert!(diff.contains("<<<<<<< HEAD"));
        assert!(diff.contains("master change"));
        assert!(diff.contains("======="));
        assert!(diff.contains("feature change"));
        assert!(diff.contains(">>>>>>>"));

        Ok(())
    }

    #[test]
    fn test_diff_deleted_file() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Create and commit initial file
        fs::write(temp_dir.path().join("delete.txt"), "content to delete\n")?;
        Command::new("git")
            .args(["add", "delete.txt"])
            .current_dir(temp_dir.path())
            .output()?;
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(temp_dir.path())
            .output()?;

        // Delete file
        fs::remove_file(temp_dir.path().join("delete.txt"))?;
        Command::new("git")
            .args(["rm", "delete.txt"])
            .current_dir(temp_dir.path())
            .output()?;

        let status = repo.get_status()?;
        let entry = status.entries.first().unwrap();
        let diff = repo.get_diff(entry)?.unwrap();

        assert!(diff.contains("-content to delete"));

        Ok(())
    }
}
