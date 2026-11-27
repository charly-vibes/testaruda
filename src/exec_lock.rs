//! Purpose: Guard `testaruda exec` against a previously wedged exec via an
//! exclusive lockfile in the store directory (testaruda-7rz7).
//! Responsibilities:
//! - Acquire `.testaruda/exec.lock` holding the holding process's pid and
//!   start time (create-exclusive, so two concurrent execs cannot both win).
//! - Classify a pre-existing lock: live foreign pid → abort with an operator-
//!   actionable message; dead pid / corrupt file / own recycled pid → reclaim.
//! - Release the lock on drop so every exec exit path (including early errors)
//!   leaves no lock behind.
//!
//! Rationale: the timeout+process-group kill (cd4u) removed the wedge cause;
//! this covers operator visibility of leftovers from pre-fix runs. Same-pid
//! locks are always reclaimable — pids are unique among live processes, so a
//! lock with our pid cannot belong to a different live exec — and the
//! recorded start time exists to inform the operator, not for disambiguation.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use miette::miette;

/// Name of the lockfile inside the store directory.
const LOCK_FILE: &str = "exec.lock";

/// Bounded retries while two execs race on reclaim/create. A short sleep
/// between attempts yields instead of hot-spinning on the
/// create/reclaim window.
const MAX_ATTEMPTS: usize = 8;
/// Inter-attempt yield (EDGE-004: bounded retry without a hot loop).
const RETRY_SLEEP: Duration = Duration::from_millis(2);

/// Details of the exec currently holding the lock, as read from the file.
pub struct LockInfo {
    pub pid: u32,
    /// Unix epoch seconds when the holder started (written at acquire).
    pub start_epoch_secs: u64,
}

/// Held exec lock; removing the file on drop is the release path.
pub struct ExecLock {
    path: PathBuf,
}

/// Global record of a held lock so it can still be released after
/// `std::process::exit`, which skips destructors. exec reaches such exits on
/// the CI paths (`select`'s outcome-code and runner-failure exits,
/// testaruda-7rz7); unix atexit covers them, elsewhere a leftover lock is
/// reclaimable as stale on the next run.
#[cfg(unix)]
static HELD_LOCK: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(unix)]
extern "C" fn release_held_lock_at_exit() {
    if let Ok(mut held) = HELD_LOCK.lock() {
        if let Some(path) = held.take() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

#[cfg(unix)]
fn record_held_lock(path: &Path) {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    if let Ok(mut held) = HELD_LOCK.lock() {
        REGISTER.call_once(|| unsafe {
            libc::atexit(release_held_lock_at_exit);
        });
        *held = Some(path.to_path_buf());
    }
}

#[cfg(unix)]
fn clear_held_lock() {
    if let Ok(mut held) = HELD_LOCK.lock() {
        *held = None;
    }
}

impl ExecLock {
    /// Acquire the exec lock at `store_dir/exec.lock`.
    ///
    /// Returns `Err` (exit non-zero via the caller) when the lock is held by a
    /// different, alive process — a wedged exec from an earlier run. A lock
    /// whose pid is dead (or unreadable, or our own recycled pid) is reclaimed.
    pub fn acquire(store_dir: &Path) -> miette::Result<ExecLock> {
        std::fs::create_dir_all(store_dir)
            .map_err(|e| miette!("Failed to create store directory: {}", e))?;
        let path = store_dir.join(LOCK_FILE);

        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(RETRY_SLEEP);
            }
            // Atomic fast path: exclusive-create means only one exec can win.
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    let acquired = write_lock(&mut file, &path);
                    #[cfg(unix)]
                    if acquired.is_ok() {
                        record_held_lock(&path);
                    }
                    return acquired.map(|()| ExecLock { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(miette!(
                        "Failed to create exec lock at {}: {}",
                        path.display(),
                        e
                    ));
                }
            }

            // Lock exists — classify the holder.
            match read_lock(&path) {
                Ok(info) => {
                    if info.pid == std::process::id() {
                        // Our own pid: pids are unique among live processes, so
                        // this cannot be another live exec — always a leftover
                        // from an earlier invocation (recycled pid or this very
                        // process). Unconditionally reclaimable.
                        eprintln!(
                            "ℹ️  Reclaimed exec lock held by our own pid {} (started {})",
                            info.pid,
                            format_start(info.start_epoch_secs)
                        );
                        let _ = std::fs::remove_file(&path);
                    } else {
                        match pid_state(info.pid) {
                            PidState::Dead => {
                                // Stale lock: the previous exec died without cleanup.
                                eprintln!(
                                    "ℹ️  Reclaimed stale exec lock (pid {} started {} died without cleanup)",
                                    info.pid,
                                    format_start(info.start_epoch_secs)
                                );
                                let _ = std::fs::remove_file(&path);
                            }
                            _ => {
                                let holder = match pid_state(info.pid) {
                                    PidState::ForeignUser =>
                                        " (owned by another user — you may not be able to kill it yourself)",
                                    _ => "",
                                };
                                return Err(miette!(
                                    "✗ Another `testaruda exec` appears to be running or wedged: pid {}, started {}{} (lock: {}). If that process is a leftover from a pre-timeout-kill run that never released its lock, terminate it (`kill {}`) or remove the lock file, then re-run. Exec aborted to avoid corrupting store state.",
                                    info.pid,
                                    format_start(info.start_epoch_secs),
                                    holder,
                                    path.display(),
                                    info.pid
                                ));
                            }
                        }
                    }
                }
                Err(_) => {
                    // Corrupt/empty lock file: treat as stale and reclaim.
                    eprintln!("ℹ️  Reclaimed unreadable exec lock at {}", path.display());
                    let _ = std::fs::remove_file(&path);
                }
            }
        }

        Err(miette!(
            "✗ Could not acquire exec lock at {} after {} attempts; another exec is contending for it.",
            path.display(),
            MAX_ATTEMPTS
        ))
    }

    /// The lockfile path this guard owns.
    pub fn lock_path(&self) -> &Path {
        &self.path
    }

    /// Explicitly release the lock (same as letting the guard drop).
    pub fn release(self) {
        drop(self);
    }
}

impl Drop for ExecLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        clear_held_lock();
        // Best-effort: a missing file is already released; other errors leave
        // a reclaimable stale lock for the next run rather than aborting exec.
        let _ = std::fs::remove_file(&self.path);
    }
}

fn write_lock(file: &mut std::fs::File, path: &Path) -> miette::Result<()> {
    let (pid, start) = (
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    let contents = format!("pid={pid}\nstart={start}\n");
    if let Err(e) = file.write_all(contents.as_bytes()) {
        // Don't leave an empty/corrupt lock behind.
        let _ = std::fs::remove_file(path);
        return Err(miette!("Failed to write exec lock: {}", e));
    }
    Ok(())
}

/// Parse a lockfile written by [`write_lock`].
fn read_lock(path: &Path) -> miette::Result<LockInfo> {
    let raw =
        std::fs::read_to_string(path).map_err(|e| miette!("Failed to read exec lock: {}", e))?;
    let mut pid = None;
    let mut start = None;
    for line in raw.lines() {
        if let Some(v) = line.strip_prefix("pid=") {
            pid = v.trim().parse::<u32>().ok();
        } else if let Some(v) = line.strip_prefix("start=") {
            start = v.trim().parse::<u64>().ok();
        }
    }
    match (pid, start) {
        (Some(pid), Some(start_epoch_secs)) => Ok(LockInfo {
            pid,
            start_epoch_secs,
        }),
        _ => Err(miette!("Unreadable exec lock at {}", path.display())),
    }
}

/// Liveness classification for a pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PidState {
    /// A process with this pid exists and we may signal it.
    Alive,
    /// No process has this pid (`ESRCH`) — stale holder.
    Dead,
    /// A process has this pid but belongs to another user (`EPERM`): we
    /// cannot signal it, and the operator must be told before being told
    /// to `kill` it (EDGE-002).
    ForeignUser,
    #[cfg(not(any(unix, windows)))]
    /// No zero-dependency liveness syscall is available; report alive so
    /// exec never double-runs against a possibly-live holder.
    Unknown,
}

fn pid_state(pid: u32) -> PidState {
    #[cfg(unix)]
    {
        let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if rc == 0 {
            return PidState::Alive;
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EPERM) => PidState::ForeignUser,
            _ => PidState::Dead,
        }
    }
    #[cfg(windows)]
    {
        // `tasklist` with a pid filter reports a CSV row for a live pid and
        // an INFO line otherwise.
        std::process::Command::new("tasklist")
            .args(["/NH", "/FO", "CSV", "/FI", &format!("PID eq {pid}")])
            .output()
            .map(|o| {
                let out = String::from_utf8_lossy(&o.stdout);
                // CSV rows quote the pid, so match it quoted to avoid
                // substring collisions between short pids.
                let quoted = format!("\"{pid}\"");
                let alive = out
                    .lines()
                    .any(|l| l.contains(&quoted) && !l.starts_with("INFO:"));
                if alive {
                    PidState::Alive
                } else {
                    PidState::Dead
                }
            })
            .unwrap_or(PidState::Alive)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        PidState::Unknown
    }
}

/// Format stored epoch seconds as an ISO-8601 UTC timestamp for the operator.
fn format_start(secs: u64) -> String {
    // A recorded 0 means the wall clock was unreadable at write time
    // (write_lock records unwrap_or(0)); rendering it as 1970-01-01 would
    // misinform the operator (CORR-003).
    if secs == 0 {
        return "unknown".to_string();
    }
    time::OffsetDateTime::from_unix_timestamp(secs as i64)
        .ok()
        .and_then(|t| {
            t.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| format!("epoch+{secs}s"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::TestUnwrap;
    use std::process::{Child, Command, Stdio};

    fn lock_path(dir: &Path) -> PathBuf {
        dir.join(LOCK_FILE)
    }

    /// Spawn a child that exits immediately, reap it, and return its (dead)
    /// pid for stale-lock tests.
    fn reaped_child_pid() -> u32 {
        #[cfg(unix)]
        let mut child = Command::new("true").spawn().unwrap_or_panic();
        #[cfg(windows)]
        let mut child = Command::new("cmd")
            .args(["/C", "exit 0"])
            .spawn()
            .unwrap_or_panic();
        let pid = child.id();
        child.wait().unwrap_or_panic();
        pid
    }

    /// Spawn a child that blocks until stdin closes, for a live foreign pid.
    fn blocking_child() -> Child {
        #[cfg(unix)]
        let child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_panic();
        #[cfg(windows)]
        let child = Command::new("cmd")
            .args(["/C", "more"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_panic();
        child
    }

    fn write_preexisting_lock(dir: &Path, pid: u32, start: u64) {
        std::fs::write(lock_path(dir), format!("pid={pid}\nstart={start}\n")).unwrap_or_panic();
    }

    /// Unwrap an `Err`, panicking when the lock was (unexpectedly) acquired.
    fn expect_acquire_err(result: miette::Result<ExecLock>) -> miette::Report {
        match result {
            Ok(_) => panic!("expected acquire to fail"),
            Err(e) => e,
        }
    }

    #[test]
    fn fresh_acquire_writes_pid_and_start() {
        let dir = tempfile::tempdir().unwrap_or_panic();
        let lock = ExecLock::acquire(dir.path()).unwrap_or_panic();

        let info = read_lock(&lock_path(dir.path())).unwrap_or_panic();
        assert_eq!(info.pid, std::process::id());
        assert!(info.start_epoch_secs > 0, "start time recorded");
        assert_eq!(
            lock.lock_path().to_path_buf(),
            lock_path(dir.path()),
            "guard owns the store-dir lockfile"
        );

        lock.release();
        assert!(!lock_path(dir.path()).exists(), "released on release()");
    }

    #[test]
    fn dead_pid_reclaims_stale_lock() {
        let dir = tempfile::tempdir().unwrap_or_panic();
        let dead = reaped_child_pid();
        write_preexisting_lock(dir.path(), dead, 1_700_000_000);

        let lock = ExecLock::acquire(dir.path()).unwrap_or_panic();
        // Reacquired under our own identity.
        let info = read_lock(&lock_path(dir.path())).unwrap_or_panic();
        assert_eq!(info.pid, std::process::id());
        drop(lock);
    }

    #[test]
    fn live_foreign_pid_aborts_and_leaves_lock_intact() {
        let dir = tempfile::tempdir().unwrap_or_panic();
        let mut child = blocking_child();
        write_preexisting_lock(dir.path(), child.id(), 1_700_000_000);

        let err = expect_acquire_err(ExecLock::acquire(dir.path()));
        let msg = format!("{err:?}");
        assert!(
            msg.contains(&child.id().to_string()),
            "report names the holding pid: {msg}"
        );
        assert!(
            msg.contains("testaruda exec"),
            "actionable operator message: {msg}"
        );
        // The live holder's lock must not be touched.
        let info = read_lock(&lock_path(dir.path())).unwrap_or_panic();
        assert_eq!(info.pid, child.id());

        child.kill().unwrap_or_panic();
        child.wait().unwrap_or_panic();
    }

    #[test]
    fn own_pid_lock_is_reclaimed_not_deadlocked() {
        let dir = tempfile::tempdir().unwrap_or_panic();
        write_preexisting_lock(dir.path(), std::process::id(), 1_700_000_000);

        let lock = ExecLock::acquire(dir.path()).unwrap_or_panic();
        drop(lock);
        assert!(!lock_path(dir.path()).exists());
    }

    #[test]
    fn lock_released_after_completion_allows_reacquire() {
        let dir = tempfile::tempdir().unwrap_or_panic();
        {
            let _lock = ExecLock::acquire(dir.path()).unwrap_or_panic();
            assert!(lock_path(dir.path()).exists());
        } // normal completion: guard drops, lock released
        assert!(!lock_path(dir.path()).exists(), "lock removed on drop");

        // The released lock is immediately available again.
        let lock2 = ExecLock::acquire(dir.path()).unwrap_or_panic();
        drop(lock2);
    }

    #[test]
    fn corrupt_lock_is_reclaimed() {
        let dir = tempfile::tempdir().unwrap_or_panic();
        std::fs::write(lock_path(dir.path()), "not a lock file\n").unwrap_or_panic();

        let lock = ExecLock::acquire(dir.path()).unwrap_or_panic();
        let info = read_lock(&lock_path(dir.path())).unwrap_or_panic();
        assert_eq!(info.pid, std::process::id());
        drop(lock);
    }
}
