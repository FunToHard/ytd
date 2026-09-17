# YTD: YouTube and YouTube Music Downloader

[![GitHub Release](https://img.shields.io/github/v/release/FunToHard/ytd?style=flat-square&logo=github&color=blue)](https://github.com/FunToHard/ytd/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/FunToHard/ytd/ci.yml?branch=main&style=flat-square&logo=github-actions&label=CI)](https://github.com/FunToHard/ytd/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/actions/workflow/status/FunToHard/ytd/release.yml?style=flat-square&logo=github-actions&label=Release)](https://github.com/FunToHard/ytd/actions/workflows/release.yml)
[![Downloads](https://img.shields.io/github/downloads/FunToHard/ytd/latest/total?style=flat-square&logo=github&color=green)](https://github.com/FunToHard/ytd/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6?style=flat-square&logo=windows)](https://github.com/FunToHard/ytd/releases/latest)
[![Rust Edition](https://img.shields.io/badge/rust-2021%20edition-DEA584?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg?style=flat-square)](LICENSE)

YTD downloads YouTube videos and YouTube Music tracks directly from your web browser. The system contains two components: a Rust desktop daemon and a browser extension.

---

## System Architecture

The browser extension captures links and sends them to the local daemon. The desktop daemon runs an HTTP server on `127.0.0.1:48123`. The daemon sanitizes links and executes `yt-dlp` in the background. The daemon routes audio files to your Music folder and video files to your Video folder.

```mermaid
flowchart LR
    subgraph Browser ["Web Browser"]
        Ext["Browser Extension\n(Context Menu & Popup)"]
    end

    subgraph Desktop ["Desktop Daemon"]
        Server["Axum HTTP Server\n(127.0.0.1:48123)"]
        Sanitizer["URL Sanitizer\n(Strip Trackers & Mixes)"]
        Engine["yt-dlp Runner\n(Local Bin / PATH)"]
        Tray["Notification Area Menu\n(Folder Picker & Auto-Start)"]
    end

    subgraph Storage ["File System"]
        Music["Music Folder\n(MP3 Audio)"]
        Video["Video Folder\n(MP4 Video)"]
    end

    Ext -->|POST /download| Server
    Server --> Sanitizer
    Sanitizer --> Engine
    Engine --> Music
    Engine --> Video
    Tray -.-> Engine
```

---

## Core Features

### URL Sanitization
- The daemon strips tracking parameters from all submitted links.
- The daemon removes mix parameters to download only the requested track.
- The daemon normalizes short links to canonical URLs.
- The daemon preserves user-created playlists.

### Automatic Media Routing
- The daemon converts YouTube Music links to high-quality MP3 audio files.
- The daemon saves audio files in the operating system Music folder.
- The daemon downloads standard YouTube links as MP4 video files.
- The daemon saves video files in the configured video folder.

### Zero-CLI Dependency Setup & Automatic Updates
- The daemon detects missing `yt-dlp`, `ffmpeg`, and `deno` binaries automatically.
- Deno executes JavaScript player challenges to prevent YouTube signature errors and bandwidth throttling.
- Users can install all required binaries through a one-click dialog.
- The daemon downloads binaries directly into `%APPDATA%\ytd\bin\`.
- The daemon configures its internal process search path automatically without modifying global Windows environment variables.
- The daemon checks for and applies `yt-dlp` and `deno` updates automatically every 24 hours.
- Users can manually trigger dependency updates at any time from the notification area menu.

### Desktop Integration
- The daemon displays an icon in the Windows notification area.
- Users can change the video download directory from the context menu.
- Users can toggle Windows startup from the context menu.
- The daemon displays Windows notifications during downloads.
- Release builds run in the background without open console windows.

---

## Prerequisites

- Windows 10 or Windows 11 (64-bit)
- Google Chrome, Microsoft Edge, or any Chromium browser
- For building from source: Rust 1.80 or later

---

## Quick Start

### 1. Run the Desktop Daemon

1. Download the latest release from the Releases page.
2. Run `ytd-daemon.exe`.
3. The daemon initializes an icon in the Windows notification area.
4. If prompted, click **Yes** to install `yt-dlp`, `ffmpeg`, and `deno` automatically.

### 2. Install the Browser Extension

1. Right-click the YTD icon in the Windows notification area.
2. Select **Install Browser Extension...**.
3. The daemon opens your browser extension settings and the extension directory.
4. Enable **Developer mode** in your browser.
5. Click **Load unpacked**.
6. Select the `extension` folder.

---

## Usage

1. Open YouTube or YouTube Music in your browser.
2. Right-click a link, a video player, or the page background.
3. Select **Send to YTD**.
4. The browser extension forwards the link to the local daemon.
5. The daemon downloads the media and displays a desktop notification.

---

## Configuration

The daemon stores settings in `%APPDATA%\ytd\config.json`.

```json
{
  "video_download_dir": "C:\\Users\\<user>\\Downloads\\ytd",
  "audio_download_dir": "C:\\Users\\<user>\\Music",
  "port": 48123,
  "auto_strip_mixes": true,
  "auto_start": false
}
```

To change the video directory, click **Change Video Download Folder...** in the notification area menu.

---

## Build From Source

1. Clone the repository.
2. Open a terminal in the project root directory.
3. Run the release build command:
   ```powershell
   cargo build --release
   ```
4. Find the compiled executable at `target\release\ytd-daemon.exe`.

---

## Automated Tests

Run the test suite with Cargo:

```powershell
cargo test
```

---

## Isolated Testing in Windows Sandbox

Test the zero-CLI installation in a clean environment:

1. Double-click `ytd_sandbox.wsb`.
2. Windows Sandbox starts an isolated virtual machine.
3. The sandbox script disables Smart App Control automatically.
4. Follow the on-screen prompts to verify dependency installation.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
