//! Hook-env hygiene for integration tests (testaruda-c64 pattern, generalized).
//!
//! When tests run under a git hook from a linked WORKTREE, git exports
//! `GIT_DIR` (pointing at the outer repo's worktree gitdir) plus friends;
//! lefthook passes them through to commands. Fixtures that `git init` a
//! temp dir then silently fail (git honors GIT_DIR) and every later
//! repo-detection fails with "not inside a git repository". Pushes from
//! main checkouts don't export GIT_DIR — which is why these failures only
//! surface when pushing migration branches from worktrees.
//!
//! The strip runs at binary init (.init_array), before any test thread
//! starts, so it does not race parallel test threads.

#[cfg(unix)]
#[used]
#[link_section = ".init_array"]
static STRIP_HOOK_GIT_ENV: extern "C" fn() = strip_hook_git_env;

#[cfg(unix)]
extern "C" fn strip_hook_git_env() {
    for var in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_PREFIX",
        "GIT_CONFIG_PARAMETERS",
        "GIT_QUARANTINE_PATH",
    ] {
        // Safe: called once at process init, before test threads spawn.
        unsafe { std::env::remove_var(var) };
    }
}
