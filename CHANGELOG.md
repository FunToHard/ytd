# Changelog

## [1.1.0](https://github.com/FunToHard/ytd/compare/v1.0.6...v1.1.0) (2026-09-16)


### Features

* add click-to-open download folder on completion notifications and bump to v1.0.2 ([4383e4c](https://github.com/FunToHard/ytd/commit/4383e4c8fdae01eff28ec7dde871db7590a804c6))
* add Deno runtime installation and automated dependency updating (v1.0.3) ([c475fee](https://github.com/FunToHard/ytd/commit/c475fee46c49f66196547c6e93c70901eb76cb13))
* initial release of github-auto-updater (Rust port of GitHubAutoUpdater.NET) ([d92ed70](https://github.com/FunToHard/ytd/commit/d92ed70f34428824f23a1cea4484f2989ed15cfc))
* initial release of YTD desktop daemon, browser extension, and zero-CLI setup ([6366698](https://github.com/FunToHard/ytd/commit/6366698c60b161f0c682840d07996446f60586a5))
* integrate github-auto-updater from standalone remote repository ([879d47e](https://github.com/FunToHard/ytd/commit/879d47ec5bc9788e6a1f75ceb897bec6cb03413a))


### Bug Fixes

* auto-update application closure and file locking issues (v1.0.4) ([32d492b](https://github.com/FunToHard/ytd/commit/32d492bba375b55c8f226b8576cb196dc24220a5))
* enforce singleton daemon instance and configure release-please CI automation ([70611e5](https://github.com/FunToHard/ytd/commit/70611e5f727d0fe0e70b959ff34e99e45ce289b6))
* migrate auto-startup and notification identity to native Win32 Registry APIs (v1.0.5) ([911174a](https://github.com/FunToHard/ytd/commit/911174a1e36e64e7c13fe0d40e8e20baee1d5612))
* use RegCreateKeyExW to ensure Run registry key exists on clean Windows/CI environments ([a7ae89c](https://github.com/FunToHard/ytd/commit/a7ae89cfdbb3046a24a9e30ae13e885f84495acc))

## [1.0.6](https://github.com/FunToHard/ytd/compare/v1.0.5...v1.0.6) (2026-09-16)


### Bug Fixes

* enforce singleton daemon instance and configure release-please CI automation ([70611e5](https://github.com/FunToHard/ytd/commit/70611e5f727d0fe0e70b959ff34e99e45ce289b6))
* use RegCreateKeyExW to ensure Run registry key exists on clean Windows/CI environments ([a7ae89c](https://github.com/FunToHard/ytd/commit/a7ae89cfdbb3046a24a9e30ae13e885f84495acc))
