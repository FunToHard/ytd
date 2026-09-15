use crate::models::AssetInfo;

/// Utilities to match release assets according to system architecture and packaging format.
pub struct AssetMatcher;

impl AssetMatcher {
    /// Identifies the best update asset from the list of release assets.
    ///
    /// If `prefer_installer` is true, prioritizes Windows setup/installer executables (e.g. `*-Setup.exe`).
    /// Otherwise, prioritizes portable archives (e.g. `*windows-x86_64.zip`) or raw binaries.
    pub fn find_target_asset<'a>(
        assets: &'a [AssetInfo],
        prefer_installer: bool,
    ) -> Option<&'a AssetInfo> {
        if prefer_installer {
            if let Some(installer) = Self::find_installer(assets) {
                return Some(installer);
            }
        }

        // Fallback or preferred portable archive
        if let Some(portable) = Self::find_portable_zip(assets) {
            return Some(portable);
        }

        // If not found yet and prefer_installer was false, try installer as secondary
        if !prefer_installer {
            if let Some(installer) = Self::find_installer(assets) {
                return Some(installer);
            }
        }

        // As last resort, look for any Windows .exe or .zip asset
        assets.iter().find(|a| {
            let lower = a.name.to_lowercase();
            (lower.ends_with(".exe") || lower.ends_with(".zip"))
                && !lower.contains("checksum")
                && !lower.ends_with(".sha256")
        })
    }

    /// Finds a Windows Setup executable asset.
    pub fn find_installer<'a>(assets: &'a [AssetInfo]) -> Option<&'a AssetInfo> {
        assets.iter().find(|a| {
            let lower = a.name.to_lowercase();
            lower.ends_with(".exe")
                && (lower.contains("setup") || lower.contains("installer") || lower.contains("install"))
        })
    }

    /// Finds a portable Windows ZIP asset.
    pub fn find_portable_zip<'a>(assets: &'a [AssetInfo]) -> Option<&'a AssetInfo> {
        assets.iter().find(|a| {
            let lower = a.name.to_lowercase();
            lower.ends_with(".zip")
                && (lower.contains("windows") || lower.contains("win64") || lower.contains("x86_64"))
        })
    }

    /// Finds companion checksum asset (e.g. `SHA256SUMS.txt`, `checksums.txt`).
    pub fn find_checksum_asset<'a>(assets: &'a [AssetInfo]) -> Option<&'a AssetInfo> {
        assets.iter().find(|a| {
            let lower = a.name.to_lowercase();
            lower == "sha256sums.txt"
                || lower == "checksums.txt"
                || lower.ends_with(".sha256")
                || lower.contains("checksum")
        })
    }
}
