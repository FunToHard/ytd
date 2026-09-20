use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadTarget {
    MusicAudio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SanitizedRequest {
    pub raw_url: String,
    pub clean_url: String,
    pub target: DownloadTarget,
    pub video_id: Option<String>,
    pub playlist_id: Option<String>,
}

/// Tracking parameters commonly attached to links that should always be stripped
const TRACKING_PARAMS: &[&str] = &[
    "si",
    "feature",
    "pp",
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "fbclid",
    "gclid",
    "ref",
    "source",
    "ab_channel",
    "start_radio",
    "themeRefresh",
    "rv",
];

pub fn sanitize_url(raw: &str) -> Result<SanitizedRequest, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    let parsed = Url::parse(trimmed).map_err(|e| format!("Invalid URL: {}", e))?;

    // SEC-02: Enforce that only HTTP and HTTPS protocols are accepted.
    // Explicitly reject file://, javascript:, data:, ftp://, etc.
    let scheme = parsed.scheme().to_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(format!(
            "Unsupported URL scheme '{}'. Only HTTP and HTTPS protocols are permitted.",
            scheme
        ));
    }

    let host = parsed.host_str().unwrap_or("").to_lowercase();
    if host.is_empty() {
        return Err("URL must contain a valid host".to_string());
    }

    // SEC-02: Prevent Server-Side Request Forgery (SSRF) and intranet scanning.
    if is_private_or_local_host(&host) {
        return Err("Local and private network URLs are prohibited".to_string());
    }

    let is_yt_music = host == "music.youtube.com";
    let is_standard_yt = host == "youtube.com"
        || host == "www.youtube.com"
        || host == "m.youtube.com"
        || host == "youtu.be";

    let target = if is_yt_music {
        DownloadTarget::MusicAudio
    } else {
        DownloadTarget::Video
    };

    // Handle youtu.be shortlinks: youtu.be/<video_id>?si=...
    if host == "youtu.be" {
        let video_id = parsed
            .path_segments()
            .and_then(|mut segs| segs.next())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if let Some(id) = video_id {
            return Ok(SanitizedRequest {
                raw_url: trimmed.to_string(),
                clean_url: format!("https://www.youtube.com/watch?v={}", id),
                target: DownloadTarget::Video,
                video_id: Some(id),
                playlist_id: None,
            });
        }
    }

    // Handle YouTube Shorts: youtube.com/shorts/<video_id>?feature=share
    if is_standard_yt && parsed.path().starts_with("/shorts/") {
        let video_id = parsed
            .path_segments()
            .and_then(|mut segs| {
                segs.next(); // skip 'shorts'
                segs.next()
            })
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        if let Some(id) = video_id {
            return Ok(SanitizedRequest {
                raw_url: trimmed.to_string(),
                clean_url: format!("https://www.youtube.com/watch?v={}", id),
                target: DownloadTarget::Video,
                video_id: Some(id),
                playlist_id: None,
            });
        }
    }

    // Process query parameters for YouTube and YT Music
    let mut video_id = None;
    let mut playlist_id = None;
    let mut clean_query_pairs = Vec::new();

    for (k, v) in parsed.query_pairs() {
        let key = k.as_ref();
        let val = v.as_ref();

        if TRACKING_PARAMS.contains(&key) {
            continue;
        }

        if key == "v" {
            video_id = Some(val.to_string());
            clean_query_pairs.push(("v".to_string(), val.to_string()));
            continue;
        }

        if key == "list" {
            // Drop auto-generated radio/mix playlists (RD...) so yt-dlp doesn't pull 50+ songs
            if val.starts_with("RD") {
                continue;
            }
            playlist_id = Some(val.to_string());
            clean_query_pairs.push(("list".to_string(), val.to_string()));
            continue;
        }

        // Keep timestamp if present
        if key == "t" {
            clean_query_pairs.push(("t".to_string(), val.to_string()));
        }
    }

    // Rebuild clean URL
    let clean_url = if is_yt_music || is_standard_yt {
        let scheme = parsed.scheme();
        let path = parsed.path();
        let mut clean_url = format!("{}://{}{}", scheme, host, path);
        if !clean_query_pairs.is_empty() {
            let query_str = clean_query_pairs
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            clean_url.push('?');
            clean_url.push_str(&query_str);
        }
        clean_url
    } else {
        // Generic URLs: strip tracking query parameters
        let mut filtered_url = parsed.clone();
        let filtered_pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .filter(|(k, _)| !TRACKING_PARAMS.contains(&k.as_ref()))
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();

        filtered_url.set_query(None);
        if !filtered_pairs.is_empty() {
            let mut serializer = filtered_url.query_pairs_mut();
            for (k, v) in &filtered_pairs {
                serializer.append_pair(k, v);
            }
        }
        filtered_url.to_string()
    };

    Ok(SanitizedRequest {
        raw_url: trimmed.to_string(),
        clean_url,
        target,
        video_id,
        playlist_id,
    })
}

/// Helper function to detect local, private, or link-local hosts to prevent SSRF
fn is_private_or_local_host(host: &str) -> bool {
    let clean_host = host.trim_start_matches('[').trim_end_matches(']');
    if clean_host == "localhost"
        || clean_host.ends_with(".localhost")
        || clean_host.ends_with(".local")
        || clean_host.ends_with(".localtest.me")
        || clean_host == "localtest.me"
        || clean_host.ends_with(".nip.io")
        || clean_host.ends_with(".vcap.me")
        || clean_host.ends_with(".lvh.me")
        || clean_host == "0.0.0.0"
        || clean_host == "::1"
    {
        return true;
    }

    if let Ok(ip) = clean_host.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(ipv4) => {
                ipv4.is_loopback()
                    || ipv4.is_private()
                    || ipv4.is_link_local()
                    || ipv4.is_unspecified()
                    || ipv4.is_broadcast()
            }
            std::net::IpAddr::V6(ipv6) => {
                if ipv6.is_loopback() || ipv6.is_unspecified() {
                    return true;
                }
                // Check IPv4-mapped IPv6 addresses (e.g. ::ffff:127.0.0.1)
                if let Some(ipv4) = ipv6.to_ipv4_mapped() {
                    return ipv4.is_loopback()
                        || ipv4.is_private()
                        || ipv4.is_link_local()
                        || ipv4.is_unspecified()
                        || ipv4.is_broadcast();
                }
                // Check IPv6 unique local (fc00::/7) and link-local (fe80::/10)
                let segments = ipv6.segments();
                let is_unique_local = (segments[0] & 0xfe00) == 0xfc00;
                let is_link_local = (segments[0] & 0xffc0) == 0xfe80;
                is_unique_local || is_link_local
            }
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yt_music_sanitization() {
        let raw = "https://music.youtube.com/watch?v=dQw4w9WgXcQ&si=u7JgL3eL63v&feature=share";
        let res = sanitize_url(raw).expect("Should parse");
        assert_eq!(res.target, DownloadTarget::MusicAudio);
        assert_eq!(res.video_id.as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(
            res.clean_url,
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn test_yt_music_radio_mix_stripped() {
        let raw = "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RDAMVMdQw4w9WgXcQ&si=xxx";
        let res = sanitize_url(raw).expect("Should parse");
        assert_eq!(res.target, DownloadTarget::MusicAudio);
        assert_eq!(
            res.clean_url,
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert!(res.playlist_id.is_none());
    }

    #[test]
    fn test_standard_yt_with_mix_stripped() {
        let raw = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ&start_radio=1&rv=xxx&si=yyy";
        let res = sanitize_url(raw).expect("Should parse");
        assert_eq!(res.target, DownloadTarget::Video);
        assert_eq!(res.video_id.as_deref(), Some("dQw4w9WgXcQ"));
        assert_eq!(
            res.clean_url,
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn test_youtu_be_shortlink() {
        let raw = "https://youtu.be/dQw4w9WgXcQ?si=abcdef123&feature=shared";
        let res = sanitize_url(raw).expect("Should parse");
        assert_eq!(res.target, DownloadTarget::Video);
        assert_eq!(
            res.clean_url,
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn test_youtube_shorts() {
        let raw = "https://www.youtube.com/shorts/dQw4w9WgXcQ?feature=share";
        let res = sanitize_url(raw).expect("Should parse");
        assert_eq!(res.target, DownloadTarget::Video);
        assert_eq!(
            res.clean_url,
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn test_genuine_playlist_preserved() {
        let raw = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAlr7vYJ3_9W5Z4jG4e1c1Qf5x8Z&si=tracker";
        let res = sanitize_url(raw).expect("Should parse");
        assert_eq!(res.target, DownloadTarget::Video);
        assert_eq!(
            res.playlist_id.as_deref(),
            Some("PLrAlr7vYJ3_9W5Z4jG4e1c1Qf5x8Z")
        );
        assert_eq!(
            res.clean_url,
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAlr7vYJ3_9W5Z4jG4e1c1Qf5x8Z"
        );
    }

    #[test]
    fn test_disallowed_url_schemes() {
        assert!(sanitize_url("file:///C:/Windows/win.ini").is_err());
        assert!(sanitize_url("javascript:alert(1)").is_err());
        assert!(sanitize_url("data:text/html,test").is_err());
        assert!(sanitize_url("ftp://ftp.example.com/file.mp4").is_err());
    }

    #[test]
    fn test_ssrf_and_private_hosts_rejected() {
        assert!(sanitize_url("http://127.0.0.1/test").is_err());
        assert!(sanitize_url("http://localhost/test").is_err());
        assert!(sanitize_url("http://192.168.1.1/video.mp4").is_err());
        assert!(sanitize_url("http://10.0.0.1/video.mp4").is_err());
        assert!(sanitize_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(sanitize_url("http://[::1]/video.mp4").is_err());
        assert!(sanitize_url("http://[::ffff:127.0.0.1]/test").is_err());
        assert!(sanitize_url("http://[fe80::1]/test").is_err());
        assert!(sanitize_url("http://[fc00::1]/test").is_err());
        assert!(sanitize_url("http://localtest.me/test").is_err());
        assert!(sanitize_url("http://127.0.0.1.nip.io/test").is_err());
    }
}
