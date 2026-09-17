#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CreateMutexW;

/// Default session-isolated mutex name for the YTD desktop daemon.
pub const DEFAULT_MUTEX_NAME: &str = r"Local\YTD_Daemon_SingleInstance_Mutex";

/// RAII guard representing ownership of the single-instance mutex.
/// When dropped, the underlying kernel mutex handle is closed.
pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: HANDLE,
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
    acquire_named_instance(DEFAULT_MUTEX_NAME)
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

        Ok(SingleInstanceGuard { handle })
    }

    #[cfg(not(windows))]
    {
        let _ = mutex_name;
        Ok(SingleInstanceGuard {})
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
}
