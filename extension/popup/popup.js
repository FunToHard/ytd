const DAEMON_URL = "http://127.0.0.1:48123";
const CLIENT_HEADER = "ytd-browser-extension";

function isValidHttpUrl(urlStr) {
  try {
    const u = new URL(urlStr);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

document.addEventListener("DOMContentLoaded", () => {
  const statusPill = document.getElementById("status-pill");
  const statusText = document.getElementById("status-text");
  const activeCount = document.getElementById("active-count");
  const musicDir = document.getElementById("music-dir");
  const videoDir = document.getElementById("video-dir");
  const urlInput = document.getElementById("url-input");
  const sendBtn = document.getElementById("send-btn");
  const sendCurrentTabBtn = document.getElementById("send-current-tab-btn");
  const feedbackMsg = document.getElementById("feedback-msg");
  const historyList = document.getElementById("history-list");
  const clearHistoryBtn = document.getElementById("clear-history-btn");
  const depsWarning = document.getElementById("deps-warning");
  const installDepsBtn = document.getElementById("install-deps-btn");
  const activeDownloadsCard = document.getElementById("active-downloads-card");
  const activeDownloadsList = document.getElementById("active-downloads-list");
  const activeItemsCount = document.getElementById("active-items-count");
  const cancelAllBtn = document.getElementById("cancel-all-btn");

  cancelAllBtn.addEventListener("click", cancelAllDownloads);

  // Check daemon status & config on open
  checkDaemonStatus();
  loadHistory();
  fetchActiveDownloads();
  const pollTimer = setInterval(fetchActiveDownloads, 1000);
  window.addEventListener("unload", () => clearInterval(pollTimer));

  // 1-Click install dependencies handler
  installDepsBtn.addEventListener("click", async () => {
    installDepsBtn.disabled = true;
    installDepsBtn.textContent = "Installing yt-dlp, ffmpeg & Deno...";
    showFeedback("Downloading dependencies in background...", true);

    try {
      await fetch(`${DAEMON_URL}/dependencies/install`, {
        method: "POST",
        headers: { "X-YTD-Client": CLIENT_HEADER }
      });
      pollDependencyInstallation();
    } catch (e) {
      showFeedback("Failed to trigger installation: " + e.message, false);
      installDepsBtn.disabled = false;
      installDepsBtn.textContent = "1-Click Install yt-dlp, ffmpeg & Deno";
    }
  });

  async function pollDependencyInstallation() {
    for (let i = 0; i < 40; i++) {
      await new Promise(r => setTimeout(r, 2500));
      try {
        const res = await fetch(`${DAEMON_URL}/dependencies/status`, {
          headers: { "X-YTD-Client": CLIENT_HEADER }
        });
        if (res.ok) {
          const json = await res.json();
          if (json.data && json.data.all_ready) {
            depsWarning.classList.add("hidden");
            showFeedback("yt-dlp, ffmpeg, and Deno are ready!", true);
            return;
          }
        }
      } catch (e) {}
    }
    installDepsBtn.disabled = false;
    installDepsBtn.textContent = "1-Click Install yt-dlp, ffmpeg & Deno";
  }

  // Send input URL
  sendBtn.addEventListener("click", () => {
    const raw = urlInput.value.trim();
    if (!raw) return;
    performSend(raw);
  });

  urlInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const raw = urlInput.value.trim();
      if (!raw) return;
      performSend(raw);
    }
  });

  // Send active tab URL
  sendCurrentTabBtn.addEventListener("click", async () => {
    try {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (tab && tab.url) {
        performSend(tab.url);
      } else {
        showFeedback("No active tab URL found", false);
      }
    } catch (e) {
      showFeedback("Could not get tab URL: " + e.message, false);
    }
  });

  // Clear history
  clearHistoryBtn.addEventListener("click", async () => {
    await chrome.storage.local.set({ downloadHistory: [] });
    loadHistory();
  });

  async function checkDaemonStatus() {
    try {
      const healthRes = await fetch(`${DAEMON_URL}/health`, {
        signal: AbortSignal.timeout(1500),
        headers: { "X-YTD-Client": CLIENT_HEADER }
      });
      if (!healthRes.ok) throw new Error("Health check failed");
      const healthData = await healthRes.json();

      statusPill.className = "status-pill online";
      statusText.textContent = "Connected";
      activeCount.textContent = healthData.data?.active_downloads ?? 0;

      // Fetch config directories
      const configRes = await fetch(`${DAEMON_URL}/config`, {
        signal: AbortSignal.timeout(1500),
        headers: { "X-YTD-Client": CLIENT_HEADER }
      });
      if (configRes.ok) {
        const configData = await configRes.json();
        if (configData.data) {
          musicDir.textContent = configData.data.audio_download_dir;
          musicDir.title = configData.data.audio_download_dir;
          videoDir.textContent = configData.data.video_download_dir;
          videoDir.title = configData.data.video_download_dir;
        }
      }

      // Check dependency health
      try {
        const depsRes = await fetch(`${DAEMON_URL}/dependencies/status`, {
          signal: AbortSignal.timeout(1500),
          headers: { "X-YTD-Client": CLIENT_HEADER }
        });
        if (depsRes.ok) {
          const depsData = await depsRes.json();
          if (depsData.data && !depsData.data.all_ready) {
            depsWarning.classList.remove("hidden");
          } else {
            depsWarning.classList.add("hidden");
          }
        }
      } catch (e) {}
    } catch (e) {
      statusPill.className = "status-pill offline";
      statusText.textContent = "Offline";
      musicDir.textContent = "Daemon offline";
      videoDir.textContent = "Daemon offline";
      activeCount.textContent = "0";
      depsWarning.classList.add("hidden");
      activeDownloadsCard.classList.add("hidden");
      activeDownloadsList.innerHTML = "";
    }
  }

  async function performSend(url) {
    if (!isValidHttpUrl(url)) {
      showFeedback("Please enter a valid HTTP or HTTPS link", false);
      return;
    }

    sendBtn.disabled = true;
    sendCurrentTabBtn.disabled = true;
    showFeedback("Sending to daemon...", true);

    chrome.runtime.sendMessage({ action: "sendUrl", url: url }, (res) => {
      sendBtn.disabled = false;
      sendCurrentTabBtn.disabled = false;

      if (chrome.runtime.lastError) {
        showFeedback("Extension error: " + chrome.runtime.lastError.message, false);
        return;
      }

      if (res && res.success) {
        const target = res.data.target === "music_audio" ? "MP3 (Music)" : "MP4 (Video)";
        showFeedback(`Queued for download as ${target}!`, true);
        urlInput.value = "";
        loadHistory();
        checkDaemonStatus();
        fetchActiveDownloads();
      } else {
        showFeedback("Error: " + (res?.error || "Failed to send"), false);
      }
    });
  }

  function showFeedback(msg, isSuccess) {
    feedbackMsg.textContent = msg;
    feedbackMsg.className = `feedback-msg ${isSuccess ? "success" : "error"}`;
    setTimeout(() => {
      feedbackMsg.classList.add("hidden");
    }, 4000);
  }

  async function loadHistory() {
    const res = await chrome.storage.local.get(["downloadHistory"]);
    const history = Array.isArray(res?.downloadHistory) ? res.downloadHistory : [];
    historyList.innerHTML = "";

    if (history.length === 0) {
      const li = document.createElement("li");
      li.className = "history-empty";
      li.textContent = "No recent transfers";
      historyList.appendChild(li);
      return;
    }

    history.forEach((item) => {
      if (!item || !item.url) return;
      const li = document.createElement("li");
      li.className = "history-item";

      const urlSpan = document.createElement("span");
      urlSpan.className = "history-url";
      urlSpan.textContent = item.url;
      urlSpan.title = item.url;

      const tagSpan = document.createElement("span");
      tagSpan.className = "history-tag";
      tagSpan.textContent = item.type || "Media";

      li.appendChild(urlSpan);
      li.appendChild(tagSpan);
      historyList.appendChild(li);
    });
  }

  async function fetchActiveDownloads() {
    try {
      const res = await fetch(`${DAEMON_URL}/downloads/active`, {
        signal: AbortSignal.timeout(1500),
        headers: { "X-YTD-Client": CLIENT_HEADER }
      });
      if (!res.ok) return;
      const json = await res.json();
      const downloads = Array.isArray(json.data) ? json.data : [];
      renderActiveDownloads(downloads);
    } catch {
      // Ignore transient network errors
    }
  }

  function renderActiveDownloads(downloads) {
    activeCount.textContent = downloads.length;
    activeItemsCount.textContent = downloads.length;

    if (downloads.length === 0) {
      activeDownloadsCard.classList.add("hidden");
      activeDownloadsList.innerHTML = "";
      return;
    }

    activeDownloadsCard.classList.remove("hidden");
    if (downloads.length >= 2) {
      cancelAllBtn.classList.remove("hidden");
    } else {
      cancelAllBtn.classList.add("hidden");
    }

    activeDownloadsList.innerHTML = "";
    downloads.forEach((item) => {
      const itemEl = document.createElement("div");
      itemEl.className = "active-item";

      const topRow = document.createElement("div");
      topRow.className = "active-item-top";

      const infoDiv = document.createElement("div");
      infoDiv.className = "active-item-info";

      const titleSpan = document.createElement("span");
      titleSpan.className = "active-item-title";
      titleSpan.textContent = item.title || item.clean_url;
      titleSpan.title = item.title || item.clean_url;

      const targetSpan = document.createElement("span");
      targetSpan.className = "active-item-target";
      targetSpan.textContent = item.target === "music_audio" ? "🎵 MP3" : "🎬 MP4";

      infoDiv.appendChild(titleSpan);
      infoDiv.appendChild(targetSpan);

      const cancelBtn = document.createElement("button");
      cancelBtn.className = "btn-cancel-item";
      cancelBtn.textContent = "✕ Cancel";
      cancelBtn.title = "Cancel download";
      cancelBtn.addEventListener("click", () => cancelSingleDownload(item.id, item.title));

      topRow.appendChild(infoDiv);
      topRow.appendChild(cancelBtn);

      const barBg = document.createElement("div");
      barBg.className = "progress-bar-bg";

      const barFill = document.createElement("div");
      barFill.className = "progress-bar-fill";

      let statusText = "Downloading...";

      if (item.status === "queued") {
        statusText = "Queued in line...";
        barFill.style.width = "0%";
      } else if (item.status === "converting") {
        statusText = "Converting media...";
        barFill.classList.add("indeterminate");
      } else if (item.status === "cancelled") {
        statusText = "Cancelling...";
        barFill.style.width = "100%";
      } else {
        if (typeof item.progress_percent === "number") {
          const pct = Math.min(100, Math.max(0, item.progress_percent));
          statusText = `Downloading ${pct.toFixed(1)}%`;
          barFill.style.width = `${pct}%`;
        } else {
          statusText = "Downloading...";
          barFill.classList.add("indeterminate");
        }
      }

      barBg.appendChild(barFill);

      const statusRow = document.createElement("div");
      statusRow.className = "active-item-status";

      const statusDesc = document.createElement("span");
      statusDesc.className = "status-detail";
      statusDesc.textContent = statusText;

      const statusBadge = document.createElement("span");
      statusBadge.textContent = item.status.toUpperCase();

      statusRow.appendChild(statusDesc);
      statusRow.appendChild(statusBadge);

      itemEl.appendChild(topRow);
      itemEl.appendChild(barBg);
      itemEl.appendChild(statusRow);
      activeDownloadsList.appendChild(itemEl);
    });
  }

  async function cancelSingleDownload(id, title) {
    try {
      showFeedback("Cancelling download...", true);
      const res = await fetch(`${DAEMON_URL}/downloads/cancel`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-YTD-Client": CLIENT_HEADER
        },
        body: JSON.stringify({ id })
      });
      if (res.ok) {
        showFeedback(`Cancelled: ${title || "#" + id}`, true);
        await fetchActiveDownloads();
      } else {
        const err = await res.json().catch(() => ({}));
        showFeedback(err.error || "Failed to cancel", false);
      }
    } catch (e) {
      showFeedback("Cancel error: " + e.message, false);
    }
  }

  async function cancelAllDownloads() {
    try {
      cancelAllBtn.disabled = true;
      showFeedback("Cancelling all downloads...", true);
      const res = await fetch(`${DAEMON_URL}/downloads/cancel`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-YTD-Client": CLIENT_HEADER
        },
        body: JSON.stringify({ all: true })
      });
      if (res.ok) {
        const json = await res.json();
        showFeedback(json.data?.message || "All downloads cancelled", true);
        await fetchActiveDownloads();
      } else {
        const err = await res.json().catch(() => ({}));
        showFeedback(err.error || "Failed to cancel all", false);
      }
    } catch (e) {
      showFeedback("Cancel error: " + e.message, false);
    } finally {
      cancelAllBtn.disabled = false;
    }
  }
});
