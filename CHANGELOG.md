# Changelog

## [1.0.11](https://github.com/FunToHard/ytd/compare/v1.0.10...v1.0.11) (2026-09-19)


### Bug Fixes

* **desktop:** embed PE metadata and synchronize StartupApproved registry for Windows Settings ([7909e1e](https://github.com/FunToHard/ytd/commit/7909e1e5408609dd1e68ffe81ea67103c1c0ef02))

## [1.0.10](https://github.com/FunToHard/ytd/compare/v1.0.9...v1.0.10) (2026-09-18)


### Bug Fixes

* **extension:** prioritize page context menu so Edge shows Send Current Page on images ([1e14701](https://github.com/FunToHard/ytd/commit/1e14701b813fcd320bc57ab83e45d5e4f003576a))

## [1.0.9](https://github.com/FunToHard/ytd/compare/v1.0.8...v1.0.9) (2026-09-17)

### Features

* **desktop:** add "Download Single Track by Default" tray menu checkbox and config persistence
* **desktop,extension:** add download cancellation and real-time streaming progress in Windows tray menu and browser extension popup

### Bug Fixes

* **extension:** support context menus on all page elements including images and album art ([f6f0664](https://github.com/FunToHard/ytd/commit/f6f0664))

## [1.0.8](https://github.com/FunToHard/ytd/compare/v1.0.7...v1.0.8) (2026-09-17)


### Bug Fixes

* **desktop:** generate multi-resolution anti-aliased app-icon.ico and embed up to 256x256 in daemon PE
* **extension:** support image and media context menus on YouTube and YouTube Music ([4af65c2](https://github.com/FunToHard/ytd/commit/4af65c267803f368828ef52ef3a918dff25b5591))

### Documentation

* add MIT license, setup installer license agreement, and update documentation ([40ce47a](https://github.com/FunToHard/ytd/commit/40ce47ad1f957dbf0c8c54be7381e71809fd4398))

## [1.0.7](https://github.com/FunToHard/ytd/compare/v1.0.6...v1.0.7) (2026-09-17)


### Bug Fixes

* **security:** resolve High severity audit vulnerabilities SEC-01, SEC-02, SEC-03 ([b214341](https://github.com/FunToHard/ytd/commit/b21434157992f1cf03fbdd2dc52daeb51ac97c69))
* **security:** resolve Low severity findings SEC-09, SEC-10, BUG-04, BUG-05, BUG-06, BUG-07 ([4ff87d4](https://github.com/FunToHard/ytd/commit/4ff87d460e997047c4ee28633b525226360cea76))
* **security:** resolve Medium severity findings SEC-04, SEC-05, SEC-06, SEC-07, SEC-08, BUG-01, BUG-02 ([e4fc70a](https://github.com/FunToHard/ytd/commit/e4fc70a7d454bb3a699acd562c35c597ee9caf2e))

## [1.0.6](https://github.com/FunToHard/ytd/compare/v1.0.5...v1.0.6) (2026-09-16)


### Bug Fixes

* enforce singleton daemon instance and configure release-please CI automation ([70611e5](https://github.com/FunToHard/ytd/commit/70611e5f727d0fe0e70b959ff34e99e45ce289b6))
* use RegCreateKeyExW to ensure Run registry key exists on clean Windows/CI environments ([a7ae89c](https://github.com/FunToHard/ytd/commit/a7ae89cfdbb3046a24a9e30ae13e885f84495acc))
