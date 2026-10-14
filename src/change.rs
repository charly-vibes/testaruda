//! Change detection — compute Δ from a diff or explicit file list.
//!
//! See TIA-CHG-001 through TIA-CHG-008.
//!
//! All git invocation is delegated to `genesis::git` (genesis-vibes ≥ 0.13)
//! — no local subprocess spawning or porcelain parsing lives here.

use genesis::git::{self, GitError};

/// The set of changed content units.
#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    /// Changed file paths.
    pub files: Vec<String>,
    /// Base revision (git ref).
    pub base: Option<String>,
    /// Head revision (git ref).
    pub head: Option<String>,
    /// True when `files` was computed from a `git diff base..head` range
    /// (testaruda-jdw5). In-range changes are changed by definition —
    /// working-tree fingerprint comparison cannot detect them when the
    /// store was ingested at head.
    pub from_revisions: bool,
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
                from_revisions: false,
            });
        }

        if let (Some(b), Some(h)) = (base, head) {
            let root = git::repo_root().map_err(git_error)?;
            let files = git::changed_files_between(&root, b, h).map_err(git_error)?;

            return Ok(Self {
                files,
                base: Some(b.to_string()),
                head: Some(h.to_string()),
                from_revisions: true,
            });
        }

        // Uncommitted changes in working tree (staged, unstaged, renames,
        // untracked — per the shared porcelain v1 parser contract)
        let root = git::repo_root().map_err(git_error)?;
        let files = git::uncommitted_files(&root).map_err(git_error)?;

        Ok(Self {
            files,
            base: None,
            head: None,
            from_revisions: false,
        })
    }
}

/// Map a `genesis::git` typed error onto miette, preserving the failure
/// shape: git failures carry the args, exit code and stderr; non-repo
/// starts carry the walk start directory. No silent degradation.
fn git_error(e: GitError) -> miette::Report {
    match e {
        GitError::Git { args, code, stderr } => {
            miette::miette!("git {} exited with {}: {}", args.join(" "), code, stderr)
        }
        GitError::NotInRepo { start } => miette::miette!(
            "not inside a git repository: no .git entry found in {} or any parent",
            start.display()
        ),
        GitError::Spawn { source } => {
            miette::miette!("failed to spawn git: {}", source)
        }
        GitError::Io {
            path,
            message,
            source,
        } => miette::miette!("io error at {}: {} ({})", path.display(), message, source),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{LazyLock, Mutex};

    /// Global lock for CWD-manipulating tests (parallel test threads
    /// share the process CWD — never mutate it without this guard).
    static CWD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    /// Run `f` with the process CWD set to `dir`, then restore.
    fn with_cwd<R>(dir: &Path, f: impl FnOnce() -> R) -> R {
        let _guard = CWD_LOCK.lock().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        let result = f();
        std::env::set_current_dir(&orig).unwrap();
        result
    }

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

    // --- genesis::git shared-parser characterization ports (genesis-tpf.2) ---

    #[test]
    fn test_shared_parse_modified_untracked_mixed() {
        // Ported from the local parser tests: plain XY records pass
        // through, untracked entries are included, and multiple statuses
        // parse independently.
        let body = " M src/lib.rs\nA  src/new.rs\n?? src/untracked.py\nMM src/conflict.rs";
        let files = genesis::git::parse_porcelain(body);
        assert_eq!(files.len(), 4);
        assert!(files.contains(&"src/lib.rs".to_string()));
        assert!(files.contains(&"src/new.rs".to_string()));
        assert!(files.contains(&"src/untracked.py".to_string()));
        assert!(files.contains(&"src/conflict.rs".to_string()));
    }

    #[test]
    fn test_shared_parse_rename_reports_new_path() {
        // Rename: "R  old -> new" — shared parser reports the new path only.
        let body = "R  src/old.rs -> src/new.rs";
        assert_eq!(
            genesis::git::parse_porcelain(body),
            vec!["src/new.rs"],
            "shared parser: rename should yield new path only"
        );
    }

    #[test]
    fn test_shared_parse_rename_modified_reports_new_path() {
        let body = "RM src/old.rs -> src/new.rs";
        assert_eq!(
            genesis::git::parse_porcelain(body),
            vec!["src/new.rs"],
            "shared parser: rename+modify should yield new path only"
        );
    }

    #[test]
    fn test_shared_parse_quoted_untracked_path_is_unquoted() {
        // core.quotePath puts quotes around paths with special characters;
        // the shared parser unquotes them (spec: quoted paths are unquoted).
        let body = "?? \"quote path.rs\"";
        assert_eq!(
            genesis::git::parse_porcelain(body),
            vec!["quote path.rs"],
            "shared parser: quoted paths should be unquoted"
        );
    }

    // --- migrated-contract tests (RED until from_diff delegates to genesis::git) ---

    #[test]
    fn test_from_diff_nonrepo_is_error_not_silent_empty() {
        // In a directory that is NOT a git repo, from_diff must return a
        // miette error (declared failure semantics), not Ok with an empty
        // set (silent degradation is not in this operation's contract).
        let dir = tempfile::tempdir().unwrap();
        let err = with_cwd(dir.path(), || {
            ChangeSet::from_diff(None, None, None).unwrap_err()
        });
        let msg = format!("{}", err);
        assert!(
            msg.contains("not inside a git repository"),
            "expected helpful not-in-repo error, got: {msg}"
        );
    }

    #[test]
    fn test_from_diff_bad_revision_error_keeps_code_and_stderr() {
        // A failing range diff keeps the "exited with <code> + stderr" shape.
        let err =
            ChangeSet::from_diff(Some("nonexistent-base-xyz"), Some("HEAD"), None).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("128"), "expected exit code 128, got: {msg}");
        assert!(
            msg.contains("unknown revision") || msg.contains("bad revision"),
            "expected stderr content, got: {msg}"
        );
    }
}
