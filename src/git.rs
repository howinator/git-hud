use anyhow::{Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::{absolute, PathBuf};
use std::process::Command;
use std::str::FromStr;

pub struct Repository {
    _repo: git2::Repository,
    repo_root_path: PathBuf,
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
    pub abs_path: PathBuf,
    pub display_path: String,
    pub status: StatusCode,
    pub staged: bool,
    pub original_path: Option<String>,
    pub is_binary: bool,
}

#[derive(Debug)]
pub struct Status {
    pub entries: Vec<StatusEntry>,
}
impl Repository {
    pub fn open_current_directory(dir: Option<&str>) -> Result<Self> {
        let path = PathBuf::from(dir.unwrap_or("."));
        let repo = git2::Repository::open(&path)?;
        Ok(Self {
            _repo: repo,
            repo_root_path: path,
        })
    }

    pub fn get_status(&self) -> Result<Status> {
        let mut cmd = self.make_command("git");
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

        // Split on NUL byte while preserving empty strings
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
                    self.is_file_binary(&entry.abs_path)?
                } else {
                    false
                };

                entries.push(StatusEntry { is_binary, ..entry });
            }
        }

        Ok(Status { entries })
    }
    fn make_command(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(self.repo_root_path.as_path());
        cmd
    }
    // Uses the grep heuristic for whether a file is binary
    // TODO: There _must_ be a better way to do this.
    fn is_file_binary(&self, path: &PathBuf) -> Result<bool> {
        // Skip if file doesn't exist (e.g., deleted files)
        if !path.exists() {
            return Ok(false);
        }
        let mut file_cmd = self.make_command("file");

        let output = file_cmd
            .args(["-bL", "--mime"])
            .arg(path)
            .output()
            .context("Failed to execute grep")?;

        let decoded_cmd_output = String::from_utf8_lossy(&output.stdout);

        if decoded_cmd_output.contains("charset=binary") && !decoded_cmd_output.contains("inode/x-empty") {
            return Ok(true);
        }
        let mut file = File::open(path)?;

        // Read the entire file into a buffer
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        if buffer.is_empty() {
            return Ok(false);
        }

        // Attempt to convert the buffer to a UTF-8 string
        // Return true if it's not valid UTF-8, false if it is
        Ok(String::from_utf8(buffer).is_err())
    }

    fn parse_status_line(&self, line: &str) -> Result<Option<StatusEntry>> {
        if line.is_empty() {
            return Ok(None);
        }

        // Split the line on whitespace while preserving the path which might contain spaces
        let mut parts = line.splitn(2, ' ');
        let entry_type = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing entry type"))?;

        match entry_type {
            // Regular changed entry
            "1" | "2" => {
                let remainder = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing entry data"))?;
                let mut fields = remainder.splitn(8, ' ');

                let xy = fields
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing XY field"))?;
                let _sub = fields.next(); // Skip sub field
                let _m_h = fields.next(); // Skip mH field
                let _m_i = fields.next(); // Skip mI field
                let _m_w = fields.next(); // Skip mW field
                let _hash1 = fields.next(); // Skip hash1
                let _hash2 = fields.next(); // Skip hash2

                // The remaining part is the path (might contain spaces)
                let path = fields
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing path"))?
                    .to_string();

                let staged = xy.chars().nth(0).map(|c| c != '.').unwrap_or(false);
                let status = if let Some(code) = xy.chars().nth(1) {
                    if code == '.' {
                        xy.chars().nth(0).unwrap().to_string()
                    } else {
                        println!("code to string: {}", code.to_string());
                        code.to_string()
                    }
                } else {
                    return Err(anyhow::anyhow!("Invalid status code format"));
                };

                Ok(Some(StatusEntry {
                    display_path: path.clone(),
                    abs_path: absolute(self.repo_root_path.join(path))?,
                    status: StatusCode::from_str(&status)?,
                    staged,
                    original_path: None,
                    is_binary: false, // Will be set later
                }))
            }

            // Rest of the cases remain the same
            "R" | "C" => {
                let remainder = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing rename/copy data"))?;
                let mut parts = remainder.rsplitn(2, ' ');
                let new = parts.next().unwrap().to_string();
                let original = parts.next().unwrap().to_string();

                Ok(Some(StatusEntry {
                    display_path: new.clone(),
                    abs_path: absolute(self.repo_root_path.join(new))?,
                    status: if entry_type == "R" {
                        StatusCode::Renamed
                    } else {
                        StatusCode::Copied
                    },
                    staged: true,
                    original_path: Some(original),
                    is_binary: false,
                }))
            }

            "u" => {
                let path = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing path in unmerged entry"))?
                    .to_string();

                Ok(Some(StatusEntry {
                    display_path: path.clone(),
                    abs_path: absolute(self.repo_root_path.join(path))?,
                    status: StatusCode::Unmerged,
                    staged: false,
                    original_path: None,
                    is_binary: false,
                }))
            }

            "?" => {
                let path = parts
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("Missing path in untracked entry"))?
                    .to_string();

                Ok(Some(StatusEntry {
                    display_path: path.clone(),
                    abs_path: absolute(self.repo_root_path.join(path))?,
                    status: StatusCode::Untracked,
                    staged: false,
                    original_path: None,
                    is_binary: false,
                }))
            }

            "!" => Ok(None), // Ignored files

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
                let content = std::fs::read_to_string(&entry.abs_path)
                    .context("Failed to read untracked file")?;
                Ok(Some(format!(
                    "+{}",
                    content.lines().collect::<Vec<_>>().join("\n+")
                )))
            }
            StatusCode::Deleted => {
                // For deleted files, show what was deleted using git show
                // let output = self
                //     .make_command("git")
                //     .args(["show", &format!("HEAD:{}", entry.abs_path.to_str().unwrap())])
                //     .current_dir(&entry.abs_path)
                //     .output()
                //     .context("Failed to execute git show")?;
                //
                // if output.status.success() {
                //     let content = String::from_utf8(output.stdout)
                //         .context("Invalid UTF-8 in git show output")?;
                //     Ok(Some(format!(
                //         "-{}",
                //         content.lines().collect::<Vec<_>>().join("\n-")
                //     )))
                Ok(Some("This file was deleted".parse()?))
                // } else {
                //     Ok(None)
                // }
            }
            StatusCode::Renamed | StatusCode::Copied => {
                if let Some(ref old_path) = entry.original_path {
                    let output = self
                        .make_command("git")
                        .args([
                            "diff",
                            "--no-color",
                            "--no-prefix",
                            old_path,
                            &entry.abs_path.to_str().unwrap(),
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
                        &entry.abs_path.to_str().unwrap(),
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

                args.push(&entry.abs_path.to_str().unwrap());

                let output = self
                    .make_command("git")
                    .args(&args)
                    .env("GIT_CONFIG_NOGLOBAL", "1")
                    .env("HOME", "")
                    .env("XDG_CONFIG_HOME", "")
                    .output()
                    .context("Failed to execute git diff")?;

                if output.status.success() {
                    String::from_utf8(output.stdout)
                        .context("Invalid UTF-8 in git diff output")
                        .map(Some)
                } else {
                    Err(anyhow::anyhow!("Failed to execute git diff")
                        .context(String::from_utf8(output.stderr)?))
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

        let repo = Repository::open_current_directory(temp_dir.path().to_str())?;
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
        assert_eq!(entry.abs_path.file_name().unwrap().to_str().unwrap(), "new.txt");

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
        assert_eq!(
            entry.abs_path.file_name().unwrap().to_str().unwrap(),
            "file with spaces.txt"
        );

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
            .find(|e| e.abs_path.file_name().unwrap().to_str().unwrap() == "conflict.txt")
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
        let entry = status
            .entries
            .iter()
            .find(|e| e.abs_path.file_name().unwrap().to_str().unwrap() == "sub")
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Modified));

        Ok(())
    }


    // TODO: I think the mock status line I'm passing in here is wrong
    #[ignore]
    #[test]
    fn test_parse_status_line() {
        let repo = Repository::open_current_directory(None).unwrap();

        // Test modified file
        let entry = repo
            .parse_status_line("1 .M N... 100644 100644 100644 file.txt")
            .unwrap()
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Modified));
        assert!(!entry.staged);
        assert_eq!(
            entry.abs_path.file_name().unwrap().to_str().unwrap(),
            "file.txt"
        );

        // Test staged new file
        let entry = repo
            .parse_status_line("1 A. N... 100644 100644 100644 new.txt")
            .unwrap()
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Added));
        assert!(entry.staged);
        assert_eq!(entry.abs_path.file_name().unwrap().to_str().unwrap(), "new.txt");

        // Test renamed file
        let entry = repo
            .parse_status_line("R 100 old.txt new.txt")
            .unwrap()
            .unwrap();
        assert!(matches!(entry.status, StatusCode::Renamed));
        assert!(entry.staged);
        assert_eq!(entry.abs_path.file_name().unwrap().to_str().unwrap(), "new.txt");
        assert_eq!(entry.original_path, Some("old.txt".to_string()));

        // Test untracked file
        let entry = repo.parse_status_line("? untracked.txt").unwrap().unwrap();
        assert!(matches!(entry.status, StatusCode::Untracked));
        assert!(!entry.staged);
        assert_eq!(
            entry.abs_path.file_name().unwrap().to_str().unwrap(),
            "untracked.txt"
        );
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
        assert!(!repo.is_file_binary(&repo.repo_root_path.join("text.txt"))?);
        assert!(repo.is_file_binary(&repo.repo_root_path.join("binary.bin"))?);

        // Test that binary files are excluded from status
        let status = repo.get_status()?;
        let binary_files: Vec<_> = status
            .entries
            .iter()
            .filter(|e| e.abs_path.file_name().unwrap().to_str().unwrap() == "binary.bin")
            .collect();
        assert_eq!(binary_files.first().unwrap().is_binary, true);

        let text_files: Vec<_> = status
            .entries
            .iter()
            .filter(|e| e.abs_path.file_name().unwrap().to_str().unwrap() == "text.txt")
            .collect();
        assert_eq!(
            text_files.first().unwrap().is_binary,
            false,
        );

        Ok(())
    }

    #[test]
    fn test_various_binary_files() -> Result<()> {
        let (temp_dir, repo) = setup_test_repo()?;

        // Test various binary file types
        let test_files = [
            (
                "image.png",
                &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A][..],
            ), // PNG header
            ("image.jpg", &[0xFF, 0xD8, 0xFF, 0xE0][..]), // JPEG header
            ("program.exe", &[0x4D, 0x5A, 0x90, 0x00][..]), // EXE header
            ("archive.zip", &[0x50, 0x4B, 0x03, 0x04][..]), // ZIP header
        ];
        for (filename, content) in test_files.iter() {
            let path = temp_dir.path().join(filename);
            let mut file = File::create(&path)?;
            file.write_all(content)?;
            assert!(
                repo.is_file_binary(&repo.repo_root_path.join(filename))?,
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
                !repo.is_file_binary(&repo.repo_root_path.join(filename))?,
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
        assert!(!repo.is_file_binary(&repo.repo_root_path.join("newlines.txt"))?);

        // Test file with spaces and special characters in name
        let filename = "special file (with spaces) アイウエオ.txt";
        fs::write(temp_dir.path().join(filename), "content")?;
        assert!(!repo.is_file_binary(&repo.repo_root_path.join(filename))?);

        // Test very large text file
        let large_text = "A".repeat(100_000);
        fs::write(temp_dir.path().join("large.txt"), large_text)?;
        assert!(!repo.is_file_binary(&repo.repo_root_path.join("large.txt"))?);

        // Test file with null bytes in middle
        let mut file = File::create(temp_dir.path().join("mixed.bin"))?;
        file.write_all(b"Start")?;
        file.write_all(&[0u8, 0u8])?;
        file.write_all(b"End")?;
        assert!(repo.is_file_binary(&repo.repo_root_path.join("mixed.bin"))?);

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

    // TODO: The test setup is bad here
    #[ignore]
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
        assert!(
            !merge_output.status.success(),
            "Merge should create conflict"
        );

        let status = repo.get_status()?;
        let entry = status
            .entries
            .iter()
            .find(|e| e.abs_path.file_name().unwrap().to_str().unwrap() == "conflict.txt")
            .unwrap();
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

        assert!(diff.contains("This file was deleted"));

        Ok(())
    }
}
