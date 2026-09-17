#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CreateMutexW;

/// Default session-isolated mutex name for the YTD desktop daemon.
pub const DEFAULT_MUTEX_NAME: &str = r"Local\YTD_Daemon_SingleInstance_Mutex";

/// RAII guard representing ownership of the single-instance mutex and lockfile.
/// When dropped, the underlying kernel mutex handle and lockfile are released.
pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: HANDLE,
    _lock_file: Option<std::fs::File>,
}

#[cfg(windows)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

// Safety: Windows kernel object handles can be safely moved across threads.
unsafe impl Send for SingleInstanceGuard {}
unsafe impl Sync for SingleInstanceGuard {}

/// Attempts to acquire the default single-instance lock for the daemon.
/// Returns `Ok(guard)` if this is the only running instance in the current session.
/// Returns `Err(reason)` if another instance is already running or if creation fails.
pub fn acquire_single_instance() -> Result<SingleInstanceGuard, String> {
    // Exclusive lockfile in %APPDATA%\ytd\ytd.lock prevents low-integrity DoS squatting
    let lock_file = acquire_lockfile()?;
    let mut guard = acquire_named_instance(DEFAULT_MUTEX_NAME)?;
    guard._lock_file = Some(lock_file);
    Ok(guard)
}

/// Helper function to open the default singleton lockfile in the user configuration directory.
pub fn acquire_lockfile() -> Result<std::fs::File, String> {
    let config_dir = dirs::config_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("ytd");
    let _ = std::fs::create_dir_all(&config_dir);
    let lock_path = config_dir.join("ytd.lock");
    acquire_lockfile_at(&lock_path)
}

/// Helper function to open an exclusive lockfile with zero shared access at a specific path.
pub fn acquire_lockfile_at(lock_path: &std::path::Path) -> Result<std::fs::File, String> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        // dwShareMode = 0: Deny all shared access (read, write, delete) to other processes
        options.share_mode(0);

        match options.open(lock_path) {
            Ok(file) => Ok(file),
            Err(e) => {
                if e.raw_os_error() == Some(32) {
                    Err("Another instance of YTD Daemon is already running (locked ytd.lock)".to_string())
                } else {
                    Err(format!("Could not acquire singleton lockfile {:?}: {}", lock_path, e))
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(lock_path)
            .map_err(|e| format!("Failed to open lockfile: {}", e))
    }
}

/// Attempts to acquire a single-instance lock with a specific mutex name.
pub fn acquire_named_instance(mutex_name: &str) -> Result<SingleInstanceGuard, String> {
    #[cfg(windows)]
    {
        let wide_name = crate::config::to_wide(mutex_name);
        let handle = unsafe {
            CreateMutexW(
                std::ptr::null(),
                1, // bInitialOwner = TRUE
                wide_name.as_ptr(),
            )
        };

        if handle.is_null() {
            let err = unsafe { GetLastError() };
            return Err(format!("Failed to create single-instance mutex (error code {})", err));
        }

        let last_err = unsafe { GetLastError() };
        if last_err == ERROR_ALREADY_EXISTS {
            unsafe {
                CloseHandle(handle);
            }
            return Err("Another instance of YTD Daemon is already running in this user session".to_string());
        }

        Ok(SingleInstanceGuard { handle, _lock_file: None })
    }

    #[cfg(not(windows))]
    {
        let _ = mutex_name;
        Ok(SingleInstanceGuard { _lock_file: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance_guard_lifecycle() {
        let test_mutex = r"Local\YTD_Daemon_Unit_Test_Mutex";

        // First acquisition must succeed
        let guard1 = acquire_named_instance(test_mutex);
        assert!(guard1.is_ok(), "First instance acquisition should succeed");

        // Concurrent acquisition with same mutex name must fail
        let guard2 = acquire_named_instance(test_mutex);
        assert!(
            guard2.is_err(),
            "Second instance acquisition should fail while first is active"
        );

        // Dropping guard1 releases the mutex
        drop(guard1);

        // Re-acquiring after drop must succeed
        let guard3 = acquire_named_instance(test_mutex);
        assert!(
            guard3.is_ok(),
            "Acquisition should succeed after the previous guard is dropped"
        );
    }

    #[test]
    fn test_lockfile_acquisition() {
        let temp_dir = std::env::temp_dir();
        let test_lock = temp_dir.join(format!(
            "ytd_unit_test_lock_{}.lock",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));

        let lock1 = acquire_lockfile_at(&test_lock);
        assert!(lock1.is_ok(), "First lockfile acquisition should succeed");

        #[cfg(windows)]
        {
            let lock2 = acquire_lockfile_at(&test_lock);
            assert!(lock2.is_err(), "Concurrent lockfile acquisition should fail");
        }

        drop(lock1);

        let lock3 = acquire_lockfile_at(&test_lock);
        assert!(lock3.is_ok(), "Re-acquiring lockfile after drop should succeed");

        drop(lock3);
        let _ = std::fs::remove_file(&test_lock);
    }
}
