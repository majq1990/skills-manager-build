use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::time::Duration;

use super::central_repo;

/// How long interactive operations wait for a competing lock holder before
/// giving up. Background rounds (update checks, syncs) can hold the lock for
/// seconds each; failing fast turned that transient contention into
/// user-visible "skills repository is busy" errors on toggles and installs.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(30);

/// Filename used for the central-repository write lock.
///
/// The lock lives in `base_dir()` (the parent of `skills_dir()`), not inside
/// `skills_dir` itself. `skills_dir` gets renamed/recreated during clone and
/// reclone flows, and on Windows mandatory file locking makes it impossible
/// to rename a directory that contains a file held with an exclusive lock —
/// see issue #99 (os error 5 / "Access is denied").
const LOCK_FILE_NAME: &str = ".skills-manager.lock";

#[derive(Debug)]
pub struct RepoLock {
    file: File,
}

impl RepoLock {
    pub fn acquire(operation: &str) -> Result<Self> {
        Self::acquire_waiting(operation, Duration::ZERO)
    }

    /// Acquire the lock, waiting up to `timeout` for a competing holder to
    /// release it. Retries every 100ms; on timeout returns the original
    /// "skills repository is busy: {operation}" error.
    pub fn acquire_waiting(operation: &str, timeout: Duration) -> Result<Self> {
        let deadline = std::time::Instant::now().checked_add(timeout);
        loop {
            match Self::try_acquire(operation) {
                Ok(lock) => return Ok(lock),
                Err(err) => match deadline {
                    Some(deadline) if std::time::Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    _ => return Err(err),
                },
            }
        }
    }

    fn try_acquire(operation: &str) -> Result<Self> {
        let base = central_repo::base_dir();
        std::fs::create_dir_all(&base)?;
        let lock_path = base.join(LOCK_FILE_NAME);
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("failed to open repo lock {}", lock_path.display()))?;

        file.try_lock_exclusive()
            .with_context(|| format!("skills repository is busy: {operation}"))?;

        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(
            file,
            "pid={}\nhostname={}\noperation={}\nstart_time={}",
            std::process::id(),
            std::env::var("HOSTNAME")
                .or_else(|_| std::env::var("COMPUTERNAME"))
                .unwrap_or_else(|_| "unknown".to_string()),
            operation,
            chrono::Utc::now().to_rfc3339()
        )?;
        file.sync_all()?;

        Ok(Self { file })
    }
}

impl Drop for RepoLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Regression test for issue #99: the lock file must not live inside
    /// `skills_dir`. On Windows, an open exclusive lock on a file inside a
    /// directory makes it impossible to rename or remove that directory
    /// (Access is denied / os error 5), which broke the clone-with-backup
    /// flow used by "use existing remote backup".
    #[test]
    fn lock_file_lives_outside_skills_dir() {
        let _guard = central_repo::test_base_dir_lock();
        let tmp = tempdir().unwrap();
        let base = tmp.path().join("base");
        central_repo::set_test_base_dir_override(Some(base.clone()));
        let skills_dir = central_repo::skills_dir();
        std::fs::create_dir_all(&skills_dir).unwrap();

        let lock = RepoLock::acquire("test").unwrap();

        assert!(base.join(LOCK_FILE_NAME).exists());
        assert!(!skills_dir.join(LOCK_FILE_NAME).exists());

        let entries: Vec<_> = std::fs::read_dir(&skills_dir).unwrap().collect();
        assert!(
            entries.is_empty(),
            "skills_dir should remain empty while the lock is held; got {entries:?}"
        );

        drop(lock);
        central_repo::set_test_base_dir_override(None);
    }

    /// Interactive operations must wait out transient contention instead of
    /// failing with "skills repository is busy" (the tray update check used to
    /// hold the lock per skill, so toggles/installs lost the race constantly).
    #[test]
    fn acquire_waiting_blocks_until_release() {
        let _guard = central_repo::test_base_dir_lock();
        let tmp = tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(tmp.path().join("base")));

        let holder = RepoLock::acquire("holder").unwrap();
        let waiter = std::thread::spawn(|| {
            let started = std::time::Instant::now();
            let lock = RepoLock::acquire_waiting("waiter", std::time::Duration::from_secs(5))
                .expect("waiter should acquire after release");
            (started.elapsed(), lock)
        });

        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(holder);

        let (elapsed, lock) = waiter.join().unwrap();
        assert!(
            elapsed >= std::time::Duration::from_millis(200),
            "waiter acquired too early ({elapsed:?}); it did not actually wait"
        );
        drop(lock);
        central_repo::set_test_base_dir_override(None);
    }

    #[test]
    fn acquire_waiting_times_out_while_held() {
        let _guard = central_repo::test_base_dir_lock();
        let tmp = tempdir().unwrap();
        central_repo::set_test_base_dir_override(Some(tmp.path().join("base")));

        let _holder = RepoLock::acquire("holder").unwrap();
        let err = RepoLock::acquire_waiting("waiter", std::time::Duration::from_millis(200))
            .expect_err("waiter must time out while the lock is held");
        assert!(
            err.to_string().contains("skills repository is busy: waiter"),
            "unexpected error: {err}"
        );

        central_repo::set_test_base_dir_override(None);
    }
}
