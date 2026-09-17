# YTD Security & Bug Audit Report

**Date:** 2026-09-16  
**Scope:** `daemon/src/` (Rust desktop daemon), `extension/` (Browser extension), and client-daemon integration.

---

## 1. Vulnerability & Bug Matrix

| ID | Severity | Component | Issue | File Location |
|---|---|---|---|---|
| **SEC-01** | **High** | Daemon API | Permissive CORS allows any website to control daemon | `daemon/src/server.rs:77` |
| **SEC-02** | **High** | Daemon / Ext | Unsanitized URL schemes (`file:///`, `javascript:`) passed to `yt-dlp` | `daemon/src/sanitizer.rs:46`, `extension/background.js:36` |
| **SEC-03** | **High** | Daemon Deps | Executables downloaded without cryptographic hash verification | `daemon/src/deps.rs:114, 149, 206` |
| **SEC-04** | **Medium** | Daemon Downloader | Unbounded concurrency in download tasks (Resource Exhaustion / DoS) | `daemon/src/server.rs:124`, `daemon/src/downloader.rs:22` |
| **SEC-05** | **Medium** | Daemon API | Unauthenticated `/helper/open-extension` triggers clipboard write & process spawns | `daemon/src/server.rs:218`, `daemon/src/deps.rs:410` |
| **BUG-01** | **Medium** | Daemon Lifecycle | Silent failure on port bind creates zombie tray process | `daemon/src/main.rs:87-101` |
| **BUG-02** | **Medium** | Daemon Singleton | Premature socket drop and hardcoded port in single-instance check | `daemon/src/single_instance.rs:38-40` |
| **SEC-06** | **Medium** | Extension Manifest | Missing explicit Content Security Policy (CSP) in Manifest V3 | `extension/manifest.json:1-32` |
| **BUG-03** | **Medium** | Extension Worker | Service worker `fetch` lacks timeout; errors misattributed as offline | `extension/background.js:62-113` |
| **SEC-07** | **Medium** | Daemon Deps | Predictable `%TEMP%` extraction folders susceptible to local race condition | `daemon/src/deps.rs:145, 202, 356` |
| **SEC-08** | **Medium** | Daemon / Notifier | PowerShell script interpolation breaks on paths containing single quotes | `daemon/src/deps.rs:126`, `daemon/src/notifier.rs:99` |
| **SEC-09** | **Low** | Daemon Singleton | Named mutex without explicit DACL allows low-integrity DoS squatting | `daemon/src/single_instance.rs:50-72` |
| **SEC-10** | **Low** | Daemon Downloader | Unqualified execution of `yt-dlp` risks working directory binary planting | `daemon/src/downloader.rs:75` |
| **SEC-11** | **Low** | Daemon Downloader | Missing `--` delimiter before positional URL in `yt-dlp` execution | `daemon/src/downloader.rs:117` |
| **BUG-04** | **Low** | Daemon Config | Relative paths `./Music` and `./Downloads/ytd` used as fallback | `daemon/src/config.rs:22-27` |
| **BUG-05** | **Low** | Daemon Concurrency | Cascading panics on poisoned `RwLock` via `.unwrap()` | `daemon/src/server.rs:61`, `daemon/src/tray.rs:35` |
| **BUG-06** | **Low** | Extension Storage | Read-modify-write race condition in `saveHistoryItem` | `extension/background.js:115-128` |
| **BUG-07** | **Low** | Extension Popup | Unchecked storage schema in `loadHistory()` causes unhandled `TypeError` | `extension/popup/popup.js:171-184` |
| **PERF-01** | **Low** | Daemon Notifier | Redundant PowerShell invocation on every boot for shortcut creation | `daemon/src/notifier.rs:97-113` |

---

## 2. High Severity Vulnerabilities

### SEC-01: Permissive CORS & Missing Client Authentication on Daemon API
* **Location:** `daemon/src/server.rs:77`, `extension/background.js:64-70`
* **Mechanism:**
  The Axum router configures `CorsLayer::permissive()`, which responds with `Access-Control-Allow-Origin: *` and accepts arbitrary HTTP methods and headers from any origin. The daemon requires no authentication token, pre-shared key, or origin validation.
* **Impact:**
  Any webpage open in any browser tab (e.g. `https://malicious.example`) can issue cross-origin background `fetch()` requests directly to `http://127.0.0.1:48123`:
  1. Leaks local username and filesystem paths via `GET /config`.
  2. Triggers arbitrary downloads via `POST /download`.
  3. Overwrites Windows clipboard and launches applications via `POST /helper/open-extension`.
* **Remediation:**
  1. Restrict CORS in `server.rs` to allow only extension origins (`chrome-extension://*`, `moz-extension://*`).
  2. Enforce a custom header requirement (e.g. `X-YTD-Client: ytd-browser-extension`) on all API endpoints. Web browsers prohibit standard web pages from setting custom headers across origins without passing preflight checks.

---

### SEC-02: Missing Protocol & Host Validation in URL Ingestion (SSRF / File Access)
* **Location:** `daemon/src/sanitizer.rs:46-168`, `extension/background.js:36-50`, `daemon/src/downloader.rs:117`
* **Mechanism:**
  Neither the extension nor `sanitizer.rs` validates that the URL scheme is strictly `http` or `https`. Non-YouTube URLs fall into the fallback `else` branch and are returned as valid. In `downloader.rs`, the URL is passed directly to `yt-dlp` without `--` argument isolation.
* **Impact:**
  * **Local File Reading:** Passing `file:///C:/path/to/file` prompts `yt-dlp` to process local files.
  * **SSRF:** Passing internal addresses (`http://192.168.1.1/`, `http://169.254.169.254/`) forces `yt-dlp` to query internal network targets.
  * **Option Injection:** A URL starting with `-` could be parsed as a CLI flag.
* **Remediation:**
  1. In `sanitizer.rs`, reject any URL whose scheme is not `http` or `https`.
  2. In `sanitizer.rs`, reject non-YouTube/YouTube Music domains if intended only for YouTube.
  3. In `downloader.rs`, insert `--` before `&req.clean_url`.

---

### SEC-03: Executable Downloads Without Cryptographic Hash Verification
* **Location:** `daemon/src/deps.rs:114, 149, 206, 320, 360`
* **Mechanism:**
  `deps.rs` downloads native binaries (`yt-dlp.exe`, `ffmpeg.zip`, `deno.zip`) directly from external URLs without validating SHA-256 hashes or cryptographic signatures before extraction and execution.
* **Impact:**
  Compromised release assets, rogue CA interception, or malicious mirrors can cause unverified executables to be written to `%APPDATA%\ytd\bin` and executed with current user privileges.
* **Remediation:**
  Compute SHA-256 hashes of downloaded binaries and compare against pinned hash manifests or official release checksum files prior to copying into the executable search path.

---

## 3. Medium Severity Issues

### SEC-04: Unbounded Download Concurrency (Resource Exhaustion / DoS)
* **Location:** `daemon/src/server.rs:124-149`, `daemon/src/downloader.rs:22-64`
* **Mechanism:**
  Every `POST /download` spawns an asynchronous Tokio task that immediately executes a new `yt-dlp` process. There is no queue limit or semaphore.
* **Impact:**
  An attacker or rapid user requests can spawn dozens of concurrent `yt-dlp` and `ffmpeg` processes, exhausting CPU, memory, and disk I/O.
* **Remediation:**
  Use a global `tokio::sync::Semaphore` (capping active downloads to 3–4 concurrent tasks) and queue remaining requests.

---

### SEC-05: Desktop Hijacking via Unauthenticated `/helper/open-extension` Endpoint
* **Location:** `daemon/src/server.rs:218-228`, `daemon/src/deps.rs#L410-L458`
* **Mechanism:**
  The `POST /helper/open-extension` endpoint triggers PowerShell (`Set-Clipboard`), Microsoft Edge (`start edge://extensions`), and Windows Explorer (`explorer.exe`).
* **Impact:**
  Any webpage can repeatedly trigger this endpoint via CORS to spam Edge windows and overwrite the user's clipboard.
* **Remediation:**
  Remove this endpoint from the public HTTP API or require local authorization. The desktop notification area menu can trigger this action directly without exposing an HTTP route.

---

### BUG-01: Silent Server Bind Failure Leads to Zombie Tray State
* **Location:** `daemon/src/main.rs:87-101`
* **Mechanism:**
  If `server::run_server` fails to bind (e.g. port conflict), the error is logged to tracing, but the main thread continues running the system tray loop. The tray icon displays "YTD Daemon: Online".
* **Impact:**
  The user assumes the daemon is active, while all browser extension requests fail with "Daemon offline".
* **Remediation:**
  If the server task returns an error, dispatch a toast error notification and send a shutdown signal (`tx_exit.send(())`) to terminate the process cleanly.

---

### BUG-02: Flawed Singleton Port Check (TOCTOU & Custom Port Ignored)
* **Location:** `daemon/src/single_instance.rs:38-40`
* **Mechanism:**
  `SingleInstanceGuard` binds `std::net::TcpListener::bind(("127.0.0.1", DEFAULT_PORT))` and immediately drops it. In addition, it only checks hardcoded port `48123`, ignoring custom port settings loaded later from `config.json`.
* **Impact:**
  Dropping the socket leaves a race window before Tokio binds. Custom configured ports are not checked, and if 48123 is used by another application, a daemon configured for another port fails to start.
* **Remediation:**
  Rely on the Win32 Named Mutex for singleton lifecycle and let `server::run_server` handle port binding directly.

---

### SEC-06: Extension Missing Explicit Content Security Policy (CSP)
* **Location:** `extension/manifest.json:1-32`
* **Mechanism:**
  The extension relies on default Manifest V3 CSP settings without declaring an explicit `content_security_policy` block.
* **Remediation:**
  Explicitly define:
  ```json
  "content_security_policy": {
    "extension_pages": "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src http://127.0.0.1:48123; object-src 'none'; base-uri 'none';"
  }
  ```

---

### BUG-03: Service Worker Network Timeout & Error Misattribution
* **Location:** `extension/background.js:62-113`
* **Mechanism:**
  `fetch()` calls do not specify an `AbortSignal.timeout()`. Furthermore, all errors (including JSON parse failures on HTTP 500 responses) are caught and unconditionally reported as `"Could not connect to desktop daemon"`.
* **Remediation:**
  Add `signal: AbortSignal.timeout(5000)` and inspect `response.ok` before parsing JSON.

---

### SEC-07: Insecure Predictable Temp Directory for Archive Extraction
* **Location:** `daemon/src/deps.rs:145, 202, 356`
* **Mechanism:**
  `deps.rs` uses static folders in `%TEMP%` (`std::env::temp_dir().join("ytd_setup")`).
* **Impact:**
  Predictable folder paths in shared temp directories are susceptible to symlink attacks or pre-created trojan binary substitution by unprivileged local processes.
* **Remediation:**
  Use the `tempfile` crate to create randomly generated, private temporary directories that automatically clean up on drop.

---

### SEC-08: PowerShell Command String Interpolation Breaking on Paths with Single Quotes
* **Location:** `daemon/src/deps.rs:126, 161, 240, 435`, `daemon/src/notifier.rs:99`
* **Mechanism:**
  PowerShell commands are assembled via `format!("... '{}' ...", path.display())`.
* **Impact:**
  Windows usernames or paths containing single quotes (e.g. `C:\Users\John O'Connor\...`) break the PowerShell string literal, causing script syntax errors and failed downloads.
* **Remediation:**
  Escape single quotes (`path_str.replace("'", "''")`) or replace PowerShell with native Rust crates (`reqwest`, `zip`).

---

## 4. Low Severity & Robustness Issues

### SEC-09: Named Mutex Without Explicit DACL (Low-Integrity DoS Squatting)
* **Location:** `daemon/src/single_instance.rs:50-72`
* **Mechanism:** `CreateMutexW` is called with `lpMutexAttributes = NULL`. A low-integrity/sandboxed process in the same session can create `Local\YTD_Daemon_SingleInstance_Mutex` beforehand, preventing the daemon from starting.
* **Remediation:** Use an exclusive lockfile in `%APPDATA%\ytd\ytd.lock` using Windows file locking.

### SEC-10: Unqualified Process Execution of `yt-dlp` (Binary Planting)
* **Location:** `daemon/src/downloader.rs:75`
* **Mechanism:** `Command::new("yt-dlp")` searches the current working directory before `PATH`.
* **Remediation:** Execute `%APPDATA%\ytd\bin\yt-dlp.exe` via absolute path and pass `--ffmpeg-location %APPDATA%\ytd\bin`.

### SEC-11: Missing Positional Argument Terminator (`--`) in `yt-dlp`
* **Location:** `daemon/src/downloader.rs:117`
* **Remediation:** Prepend `--` before passing `clean_url`.

### BUG-04: Relative Path Fallback in Default Configuration
* **Location:** `daemon/src/config.rs:22-27`
* **Mechanism:** Falls back to `./Music` and `./Downloads/ytd` if home directories are unresolved, creating folders relative to the daemon's working directory.
* **Remediation:** Fall back to `%USERPROFILE%` or `%APPDATA%`.

### BUG-05: Cascading Panic Risks on Poisoned `RwLock` via `.unwrap()`
* **Location:** `daemon/src/server.rs:61`, `daemon/src/tray.rs:35, 205`
* **Mechanism:** Calling `.unwrap()` on `RwLock` panics if another thread panics while holding the lock.
* **Remediation:** Use `.unwrap_or_else(|e| e.into_inner())`.

### BUG-06: Storage Race Condition in Extension `saveHistoryItem`
* **Location:** `extension/background.js:115-128`
* **Mechanism:** Asynchronous read-modify-write without serialization can drop history entries during concurrent downloads.
* **Remediation:** Chain storage writes through a promise queue.

### BUG-07: Unchecked Storage Type in Popup `loadHistory()`
* **Location:** `extension/popup/popup.js:171-184`
* **Mechanism:** If `downloadHistory` is corrupted or not an array, `history.forEach` throws an unhandled `TypeError`.
* **Remediation:** Validate `Array.isArray(res.downloadHistory)` before iteration.

### PERF-01: Redundant PowerShell Invocation on Startup
* **Location:** `daemon/src/notifier.rs:97-113`
* **Mechanism:** Launches PowerShell on every boot to create `YTD.lnk` without checking if the shortcut already exists.
* **Remediation:** Check `if !shortcut_path.exists()` before launching PowerShell.
