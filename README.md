# Description

`git hud` will give you a brief description of each changed file in your tree.
I developed this because I would frequently get interrupted by something and try to recover my flow by looking at
`git status`.
`git status` is great, but it tells you nothing about what the changes are in each file.

## Example Output

```text
On branch main
Your branch is not tracking a remote branch.

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
        modified: Cargo.toml (Updated dependencies to newer versions.)
        modified: src/display.rs (The changes improve the formatting and readability of the output for the git status command.)
        modified: src/git.rs (The diff adds support for parsing Git status output with spaces in file paths and handles binary file diffs more robustly.)
        modified: src/main.rs (Added error handling and exit code to main function.)
        modified: src/summary.rs (Summary: The code updates the API call to Anthropic's Claude model, improving the request format and error handling.)

no changes added to commit (use "git add" and/or "git commit -a")

Process finished with exit code 0


```

# Install

1. Install the crate then copy the binary to `/usr/local/bin` or some other dir on your path.
2. Set an environment variable called `ANTHROPIC_API_KEY` with an API key from Anthropic.
3. Set a git alias with `git config --global alias.hud '!git-hud'`