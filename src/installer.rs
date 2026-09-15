use std::path::Path;
use std::process::Command;

/// Utilities for applying updates via external installers or in-place binary swapping.
pub struct UpdateInstaller;

impl UpdateInstaller {
    /// Launches a Windows installer (e.g. Inno Setup `*-Setup.exe`) detached in the background.
    ///
    /// Recommended flags for Inno Setup: `["/SILENT", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS"]`
    pub fn apply_installer(
        installer_path: &Path,
        args: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x00000008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

            let mut cmd = Command::new(installer_path);
            cmd.args(args);
            cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
            cmd.spawn()?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            Command::new(installer_path).args(args).spawn()?;
        }

        Ok(())
    }

    /// Swaps the currently executing binary on disk with `new_binary_path` using the `self-replace` crate.
    /// Safely handles Windows OS file locking.
    pub fn apply_in_place_binary(
        new_binary_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self_replace::self_replace(new_binary_path)?;
        Ok(())
    }

    /// Spawns a new instance of the current executable and terminates the current process.
    pub fn restart_process() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let current_exe = std::env::current_exe()?;
        Command::new(current_exe).spawn()?;
        std::process::exit(0);
    }
}
