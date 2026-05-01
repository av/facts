/// Advisory file locking for read-modify-write operations.
///
/// Uses atomic file creation (`create_new`) as a simple cross-platform mutex.
/// A `.facts.lock` file in the project root prevents concurrent modifications.
/// Stale locks (older than 60 seconds) are automatically broken.
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};

/// Maximum age of a lock file before it is considered stale.
const STALE_LOCK_SECS: u64 = 60;

/// How long to wait between lock acquisition retries.
const RETRY_INTERVAL: Duration = Duration::from_millis(50);

/// Maximum total time to wait for the lock before giving up.
const MAX_WAIT: Duration = Duration::from_secs(10);

/// RAII guard that holds a `.facts.lock` file. The lock is released (file
/// deleted) when this value is dropped.
pub struct FileLock {
    lock_path: PathBuf,
    // Keep the file handle open so the OS knows something holds it.
    _file: File,
}

impl FileLock {
    /// Acquire an advisory lock for the project rooted at `root`.
    ///
    /// Creates `<root>/.facts.lock`. If another process already holds the lock
    /// we retry for up to [`MAX_WAIT`]. Stale locks older than
    /// [`STALE_LOCK_SECS`] are removed automatically.
    pub fn acquire(root: &Path) -> Result<Self> {
        let lock_path = root.join(".facts.lock");
        let deadline = std::time::Instant::now() + MAX_WAIT;

        loop {
            // Try atomic exclusive creation.
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(file) => {
                    return Ok(FileLock {
                        lock_path,
                        _file: file,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Check for stale lock.
                    if is_stale(&lock_path) {
                        // Best-effort removal; if it fails another process may
                        // have already cleaned it up.
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }

                    if std::time::Instant::now() >= deadline {
                        anyhow::bail!(
                            "timed out waiting for lock file {}; \
                             if no other facts process is running, \
                             remove the file manually",
                            lock_path.display()
                        );
                    }
                    thread::sleep(RETRY_INTERVAL);
                }
                Err(e) => {
                    return Err(e).context(format!(
                        "failed to create lock file {}",
                        lock_path.display()
                    ));
                }
            }
        }
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

/// Returns `true` if the lock file is older than [`STALE_LOCK_SECS`].
fn is_stale(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age.as_secs() > STALE_LOCK_SECS)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_acquire_and_release() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".facts.lock");

        {
            let _lock = FileLock::acquire(dir.path()).unwrap();
            assert!(lock_path.exists(), "lock file should exist while held");
        }

        assert!(!lock_path.exists(), "lock file should be removed on drop");
    }

    #[test]
    fn test_second_create_new_fails_while_locked() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".facts.lock");

        // Hold a lock.
        let _lock = FileLock::acquire(dir.path()).unwrap();

        // A second atomic create should fail because the file exists.
        let result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path);
        assert!(result.is_err(), "lock file should block second create_new");
    }

    #[test]
    fn test_reacquire_after_release() {
        let dir = TempDir::new().unwrap();

        {
            let _lock = FileLock::acquire(dir.path()).unwrap();
        }

        // Should succeed now that the first lock is dropped.
        let lock = FileLock::acquire(dir.path());
        assert!(lock.is_ok(), "should be able to reacquire after drop");
    }
}
