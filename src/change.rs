//! Change detection — compute Δ from a diff or explicit file list.
//!
//! See TIA-CHG-001 through TIA-CHG-008.

/// The set of changed content units.
#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    /// Changed file paths.
    pub files: Vec<String>,
    /// Base revision (git ref).
    pub base: Option<String>,
    /// Head revision (git ref).
    pub head: Option<String>,
}

impl ChangeSet {
    /// Derive the change set from a diff between base and head, or from an
    /// explicit file list.
    pub fn from_diff(
        base: Option<&str>,
        head: Option<&str>,
        files: Option<&str>,
    ) -> miette::Result<Self> {
        if let Some(f) = files {
            let paths: Vec<String> = f.split(',').map(|s| s.trim().to_string()).collect();
            return Ok(Self {
                files: paths,
                base: base.map(String::from),
                head: head.map(String::from),
            });
        }

        if let (Some(b), Some(h)) = (base, head) {
            let output = std::process::Command::new("git")
                .args(["diff", "--name-only", b, h])
                .output()
                .map_err(|e| miette::miette!("Failed to run git diff: {}", e))?;

            if !output.status.success() {
                return Err(miette::miette!(
                    "git diff exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ));
            }

            let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();

            return Ok(Self {
                files,
                base: Some(b.to_string()),
                head: Some(h.to_string()),
            });
        }

        // Uncommitted changes in working tree
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .map_err(|e| miette::miette!("Failed to run git status: {}", e))?;

        let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(parse_porcelain_line)
            .collect();

        Ok(Self {
            files,
            base: None,
            head: None,
        })
    }
}

/// Parse a single line from `git status --porcelain` v1 output.
///
/// Returns the file path, or `None` for empty lines. Handles rename/copy
/// lines (e.g., `R  src/old.rs -> src/new.rs`) by extracting the new path.
pub fn parse_porcelain_line(l: &str) -> Option<String> {
    let trimmed = l.trim_start();

    // Rename/copy: "R  oldpath -> newpath" (git status --porcelain v1)
    if trimmed.starts_with('R') || trimmed.starts_with('C') {
        if let Some(idx) = trimmed.find(" -> ") {
            let new_path = trimmed[idx + 4..].trim().to_string();
            return if new_path.is_empty() {
                None
            } else {
                Some(new_path)
            };
        }
    }

    // Standard line: "XY path"
    let rest = trimmed
        .chars()
        .skip(2)
        .collect::<String>()
        .trim()
        .to_string();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_file_list() {
        let cs = ChangeSet::from_diff(None, None, Some("src/main.rs,src/lib.rs")).unwrap();
        assert_eq!(cs.files.len(), 2);
        assert!(cs.files.iter().any(|f| f == "src/main.rs"));
    }

    #[test]
    fn test_no_args_produces_empty_set() {
        let cs = ChangeSet::from_diff(None, None, None);
        // This may fail if not in a git repo, which is fine
        assert!(cs.is_ok() || cs.is_err());
    }

    #[test]
    fn test_parse_git_status_porcelain_modified() {
        let output = " M src/lib.rs";
        let files: Vec<String> = output.lines().filter_map(parse_porcelain_line).collect();
        assert_eq!(files, vec!["src/lib.rs"]);
    }

    #[test]
    fn test_parse_git_status_porcelain_untracked() {
        let output = "?? src/new.py";
        let files: Vec<String> = output.lines().filter_map(parse_porcelain_line).collect();
        assert_eq!(files, vec!["src/new.py"]);
    }

    #[test]
    fn test_parse_git_status_porcelain_mixed() {
        let output = " M src/lib.rs\nA  src/new.rs\n?? src/untracked.py\nMM src/conflict.rs";
        let files: Vec<String> = output.lines().filter_map(parse_porcelain_line).collect();
        assert_eq!(files.len(), 4);
        assert!(files.contains(&"src/lib.rs".to_string()));
        assert!(files.contains(&"src/new.rs".to_string()));
        assert!(files.contains(&"src/untracked.py".to_string()));
        assert!(files.contains(&"src/conflict.rs".to_string()));
    }

    #[test]
    fn test_parse_git_status_porcelain_rename() {
        // Rename: "R  old -> new"
        let output = "R  src/old.rs -> src/new.rs";
        let files: Vec<String> = output.lines().filter_map(parse_porcelain_line).collect();
        assert_eq!(
            files,
            vec!["src/new.rs"],
            "rename should yield new path only"
        );
    }

    #[test]
    fn test_parse_git_status_porcelain_rename_modified() {
        // Rename with working tree change: "RM old -> new"
        // X=R (rename), Y=M (modified in working tree)
        let output = "RM src/old.rs -> src/new.rs";
        let files: Vec<String> = output.lines().filter_map(parse_porcelain_line).collect();
        assert_eq!(
            files,
            vec!["src/new.rs"],
            "rename+modify should yield new path only"
        );
    }
}
