const DAEMON_URL = "http://127.0.0.1:48123";
const CLIENT_HEADER = "ytd-browser-extension";

// Ensure only HTTP and HTTPS protocols are accepted
function isValidHttpUrl(urlStr) {
  try {
    const u = new URL(urlStr);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

// Setup context menus on installation or update
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    // 1. Context menu when right-clicking any web link (excludes javascript:, mailto:, file:)
    chrome.contextMenus.create({
      id: "ytd-download-link",
      title: "Send to YTD",
      contexts: ["link"],
      targetUrlPatterns: ["*://*/*"]
    });

    // 2. Context menu when right-clicking a video or audio player
    chrome.contextMenus.create({
      id: "ytd-download-media",
      title: "Send to YTD",
      contexts: ["video", "audio"],
      targetUrlPatterns: ["*://*/*"]
    });

    // 3. Context menu when right-clicking anywhere on YouTube or YT Music pages
    chrome.contextMenus.create({
      id: "ytd-download-page",
      title: "Send Current Page to YTD",
      contexts: ["page"],
      documentUrlPatterns: [
        "*://*.youtube.com/*",
        "*://youtube.com/*",
        "*://music.youtube.com/*",
        "*://youtu.be/*"
      ]
    });
  });
});

// Handle context menu clicks
chrome.contextMenus.onClicked.addListener((info, tab) => {
  let targetUrl = null;

  if (info.menuItemId === "ytd-download-link" && info.linkUrl) {
    targetUrl = info.linkUrl;
  } else if (info.menuItemId === "ytd-download-media" && info.srcUrl) {
    targetUrl = info.srcUrl;
  } else if (info.menuItemId === "ytd-download-page") {
    targetUrl = info.pageUrl || (tab && tab.url);
  }

  if (targetUrl) {
    if (!isValidHttpUrl(targetUrl)) {
      chrome.notifications.create({
        type: "basic",
        iconUrl: "icons/icon48.png",
        title: "YTD: Invalid Link",
        message: "Only HTTP and HTTPS links are supported."
      });
      return;
    }
    sendUrlToDaemon(targetUrl);
  }
});

// Listen for messages from popup
chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.action === "sendUrl") {
    if (!isValidHttpUrl(request.url)) {
      sendResponse({ success: false, error: "Only HTTP and HTTPS URLs are supported." });
      return false;
    }
    sendUrlToDaemon(request.url)
      .then(res => sendResponse(res))
      .catch(err => sendResponse({ success: false, error: err.message }));
    return true; // async response
  }
});

async function sendUrlToDaemon(rawUrl) {
  try {
    const response = await fetch(`${DAEMON_URL}/download`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-YTD-Client": CLIENT_HEADER
      },
      body: JSON.stringify({ url: rawUrl }),
      signal: AbortSignal.timeout(5000)
    });

    let data;
    try {
      data = await response.json();
    } catch {
      throw new Error(`Invalid response from daemon (HTTP ${response.status})`);
    }

    if (response.ok && data.success) {
      const item = data.data;
      const isMusic = item.target === "music_audio";
      const targetType = isMusic ? "Music (MP3)" : "Video (MP4)";

      saveHistoryItem({
        url: item.clean_url,
        type: targetType,
        timestamp: Date.now(),
        status: "Queued"
      });

      chrome.notifications.create({
        type: "basic",
        iconUrl: "icons/icon48.png",
        title: "YTD: Download Queued",
        message: `Queued as ${targetType}:\n${item.clean_url}`
      });

      return { success: true, data: item };
    } else {
      const errorMsg = data.error || `HTTP ${response.status} error`;
      chrome.notifications.create({
        type: "basic",
        iconUrl: "icons/icon48.png",
        title: "YTD: Download Rejected",
        message: errorMsg
      });
      return { success: false, error: errorMsg };
    }
  } catch (err) {
    const isOffline = err.name === "TimeoutError" || err.message.includes("Failed to fetch") || err.message.includes("NetworkError");
    chrome.notifications.create({
      type: "basic",
      iconUrl: "icons/icon48.png",
      title: isOffline ? "YTD Daemon Offline" : "YTD Error",
      message: isOffline
        ? "Could not connect to desktop daemon at 127.0.0.1:48123. Please ensure the YTD daemon is running."
        : err.message
    });
    return { success: false, error: err.message };
  }
}

async function saveHistoryItem(item) {
  try {
    const res = await chrome.storage.local.get(["downloadHistory"]);
    const history = res.downloadHistory || [];
    history.unshift(item);
    // Keep last 15 items
    if (history.length > 15) {
      history.pop();
    }
    await chrome.storage.local.set({ downloadHistory: history });
  } catch (e) {
    console.error("Failed to save download history", e);
  }
}
