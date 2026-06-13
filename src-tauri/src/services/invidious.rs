use crate::models::{Channel, Video};
use reqwest::Client;
use std::time::Duration;
use std::sync::Mutex;
use std::collections::HashMap;
use std::time::Instant;
use chrono::{NaiveDate, TimeZone, Utc};

/// Extra PATH entries to prepend when spawning yt-dlp/node.
/// Desktop apps often lack user-local bin dirs in PATH.
pub(crate) fn yt_dlp_extra_path() -> String {
    let mut entries = vec![];
    for dir in &[
        "/home/sergiu/.local/bin",
        "/home/sergiu/.cargo/bin",
        "/home/sergiu/.hermes/node/bin",
        "/usr/local/bin",
    ] {
        if std::path::Path::new(dir).is_dir() {
            entries.push(*dir);
        }
    }
    entries.join(":")
}

/// Resolve yt-dlp path. Tries common install locations first,
/// then falls back to bare "yt-dlp" (relies on PATH).
pub(crate) fn yt_dlp_path() -> &'static str {
    static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        for candidate in &[
            "/home/sergiu/.local/bin/yt-dlp",
            "/usr/local/bin/yt-dlp",
            "/usr/bin/yt-dlp",
            "/snap/bin/yt-dlp",
        ] {
            if std::path::Path::new(candidate).exists() {
                return candidate.to_string();
            }
        }
        "yt-dlp".to_string()
    })
}

#[derive(Clone)]
pub struct InvidiousClient {
    base_url: String,
    client: Client,
}

/// Channel avatar cache (channel_id -> (url, cached_at))
static AVATAR_CACHE: std::sync::LazyLock<Mutex<HashMap<String, (String, Instant)>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

const AVATAR_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

impl InvidiousClient {
    pub fn new(base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        Self { base_url, client }
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    fn empty_array() -> Vec<serde_json::Value> {
        vec![]
    }

    pub async fn get_channel(&self, channel_id: &str) -> Result<Channel, String> {
        // Try Invidious first; fall back to yt-dlp on any error or non-JSON response
        let url = self.api_url(&format!("/channels/{}", channel_id));
        let result = self.client.get(&url).send().await;
        if let Ok(resp) = result {
            if let Ok(text) = resp.text().await {
                if text.trim_start().starts_with('{') {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        return Ok(Channel {
                            channel_id: channel_id.to_string(),
                            channel_name: json["author"].as_str().unwrap_or("Unknown").to_string(),
                            channel_avatar: json["authorThumbnails"].as_array()
                                .and_then(|arr| arr.last())
                                .and_then(|t| t["url"].as_str())
                                .unwrap_or("")
                                .to_string(),
                            subscriber_count: json["subCount"].as_u64().unwrap_or(0),
                            description: json["description"].as_str().unwrap_or("").to_string(),
                        });
                    }
                }
            }
        }
        // Fallback to yt-dlp channel info
        get_channel_ytdlp(channel_id).await
    }

    pub async fn get_channel_videos(&self, channel_id: &str, page: u32) -> Result<Vec<Video>, String> {
        // Try Invidious API first (works for all pages)
        let url = self.api_url(&format!("/channels/{}/videos?page={}", channel_id, page));
        if let Ok(resp) = self.client.get(&url).send().await {
            if let Ok(text) = resp.text().await {
                if text.trim_start().starts_with('{') || text.trim_start().starts_with('[') {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let empty = Self::empty_array();
                        let videos = json["videos"].as_array().unwrap_or(&empty);
                        if !videos.is_empty() {
                            return Ok(videos.iter().filter_map(|v| self.parse_video(v)).collect());
                        }
                    }
                }
            }
        }
        // Fallback: InnerTube (page 1) or yt-dlp
        if page <= 1 {
            let result = get_channel_videos_innertube(channel_id).await?;
            if !result.is_empty() { return Ok(result); }
        }
        // Final fallback: yt-dlp
        get_channel_videos_ytdlp(channel_id).await
    }

    pub async fn get_channel_live(&self, channel_id: &str) -> Result<Vec<Video>, String> {
        let url = self.api_url(&format!("/channels/{}/live", channel_id));
        let result = self.client.get(&url).send().await;
        if let Ok(resp) = result {
            if let Ok(text) = resp.text().await {
                if text.trim_start().starts_with('{') {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let empty = Self::empty_array();
                        let videos = json["videos"].as_array().unwrap_or(&empty);
                        return Ok(videos.iter().filter_map(|v| self.parse_video(v)).collect());
                    }
                }
            }
        }
        // Fallback to yt-dlp live streams
        get_channel_live_ytdlp(channel_id).await
    }

    pub async fn get_video(&self, video_id: &str) -> Result<Video, String> {
        let url = self.api_url(&format!("/videos/{}", video_id));
        let resp = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !text.trim_start().starts_with('{') {
            return get_video_ytdlp(video_id).await;
        }
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        self.parse_video(&json).ok_or_else(|| "Failed to parse video".to_string())
    }

    pub async fn get_suggestions(&self, query: &str) -> Result<Vec<Video>, String> {
        // Try Invidious API first
        let url = self.api_url(&format!("/search?q={}", urlencoding::encode(query)));
        if let Ok(resp) = self.client.get(&url).send().await {
            if let Ok(text) = resp.text().await {
                if text.trim_start().starts_with('[') || text.trim_start().starts_with('{') {
                    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                        return Ok(arr.iter().filter_map(|v| self.parse_video(v)).collect());
                    }
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let empty = Self::empty_array();
                        return Ok(json.as_array().unwrap_or(&empty).iter().filter_map(|v| self.parse_video(v)).collect());
                    }
                }
            }
        }
        // Fallback to InnerTube search
        let innertube_url = format!("https://www.youtube.com/youtubei/v1/search?key={}", INNERTUBE_API_KEY);
        let body = serde_json::json!({
            "query": query,
            "context": { "client": { "clientName": "WEB", "clientVersion": "2.20260206.01.00", "hl": "en", "gl": "US" } }
        });
        let resp = self.client.post(&innertube_url).header("Content-Type", "application/json").header("Origin", "https://www.youtube.com").json(&body).send().await.map_err(|e| e.to_string())?;
        let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let mut videos = Vec::new();
        // Parse search results from InnerTube
        if let Some(contents) = json["contents"]["twoColumnSearchResultsRenderer"]["primaryContents"]["sectionListRenderer"]["contents"].as_array() {
            for section in contents {
                if let Some(items) = section["itemSectionRenderer"]["contents"].as_array() {
                    for item in items {
                        if let Some(vr) = item.get("videoRenderer") {
                            if let Some(id) = vr["videoId"].as_str() {
                                let title = vr["title"]["runs"].as_array().and_then(|r| r.first()).and_then(|r| r["text"].as_str()).unwrap_or("Unknown");
                                let channel = vr["ownerText"]["runs"].as_array().and_then(|r| r.first()).and_then(|r| r["text"].as_str()).unwrap_or("Unknown");
                                let thumb = vr["thumbnail"]["thumbnails"].as_array().and_then(|t| t.first()).and_then(|t| t["url"].as_str()).unwrap_or("");
                                // Parse duration from InnerTube's lengthSeconds (string or number)
                                // or lengthText display text ("9:58" format)
                                let duration = vr["lengthSeconds"].as_u64()
                                    .or_else(|| vr["lengthSeconds"].as_str()?.parse::<u64>().ok())
                                    .or_else(|| {
                                        vr["lengthText"]["simpleText"].as_str()
                                            .or_else(|| vr["lengthText"]["runs"].as_array()?.first()?["text"].as_str())
                                            .map(parse_duration_string)
                                    })
                                    .unwrap_or(0);
                                // Parse published_at from InnerTube's publishedTimeText
                                let published_at = vr["publishedTimeText"]["simpleText"].as_str()
                                    .or_else(|| {
                                        vr["publishedTimeText"]["runs"].as_array()
                                            .and_then(|r| r.first())
                                            .and_then(|r| r["text"].as_str())
                                    })
                                    .and_then(parse_relative_time)
                                    .unwrap_or(0);
                                // Parse view count from InnerTube's shortViewCountText (display text like "1.2M views")
                                let view_count = vr["shortViewCountText"]["simpleText"].as_str()
                                    .or_else(|| {
                                        vr["viewCountText"]["simpleText"].as_str()
                                    })
                                    .or_else(|| {
                                        vr["shortViewCountText"]["runs"].as_array()
                                            .and_then(|r| r.first())
                                            .and_then(|r| r["text"].as_str())
                                    })
                                    .map(parse_view_count)
                                    .unwrap_or(0);
                                let is_live = vr["badges"].as_array().map_or(false, |badges| {
                                    badges.iter().any(|b| {
                        b["metadataBadgeRenderer"]["style"].as_str() == Some("BADGE_STYLE_TYPE_LIVE_NOW")
                                    })
                                }) || vr["isLiveNow"].as_bool().unwrap_or(false);
                                videos.push(Video {
                                    video_id: id.to_string(),
                                    title: title.to_string(),
                                    channel_id: String::new(),
                                    channel_name: channel.to_string(),
                                    thumbnail: if thumb.starts_with("//") { format!("https:{}", thumb) } else { thumb.to_string() },
                                    duration,
                                    published_at,
                                    view_count,
                                    like_count: 0,
                                    is_live,
                                    description: String::new(),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(videos)
    }

    pub async fn search_channels(&self, query: &str) -> Result<Vec<Channel>, String> {
        let url = self.api_url(&format!("/search?q={}&type=channel", urlencoding::encode(query)));
        let resp = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !text.trim_start().starts_with('[') {
            // Fallback to yt-dlp for search
            return search_channels_ytdlp(query).await;
        }
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let results = json.as_array().unwrap();
        Ok(results.iter().filter_map(|r| {
            let channel_id = r["authorId"].as_str()?;
            Some(Channel {
                channel_id: channel_id.to_string(),
                channel_name: r["author"].as_str().unwrap_or("Unknown").to_string(),
                channel_avatar: r["authorThumbnails"].as_array()
                    .and_then(|arr| arr.last())
                    .and_then(|t| t["url"].as_str())
                    .unwrap_or("")
                    .to_string(),
                subscriber_count: 0,
                description: "".to_string(),
            })
        }).collect())
    }

    pub async fn get_comments(&self, video_id: &str) -> Result<crate::models::CommentsResponse, String> {
        let url = self.api_url(&format!("/comments/{}", video_id));
        let resp = self.client.get(&url).send().await.map_err(|e| e.to_string())?;
        let json: crate::models::CommentsResponse = resp.json().await.map_err(|e| e.to_string())?;
        Ok(json)
    }

    fn parse_video(&self, v: &serde_json::Value) -> Option<Video> {
        let video_id = v["videoId"].as_str()?;
        let title = v["title"].as_str().unwrap_or("Untitled");
        let channel_id = v["authorId"].as_str().unwrap_or("");
        let channel_name = v["author"].as_str().unwrap_or("Unknown");
        let thumbnail = v["videoThumbnails"].as_array()
            .and_then(|arr| arr.iter().find(|t| t["quality"].as_str() == Some("medium")))
            .or_else(|| v["videoThumbnails"].as_array().and_then(|arr| arr.first()))
            .and_then(|t| t["url"].as_str())
            .unwrap_or("");
        let duration = v["lengthSeconds"].as_u64().unwrap_or(0);
        // Try published timestamp first, then parse publishedText (relative time like "2 hours ago")
        let published_at = v["published"].as_i64()
            .or_else(|| {
                v["publishedText"].as_str().and_then(|s| parse_relative_time(s))
            })
            .unwrap_or(0);
        let view_count = v["viewCount"].as_u64().unwrap_or(0);
        let is_live = v["liveNow"].as_bool().unwrap_or(false);
        let description = v["description"].as_str().unwrap_or("").to_string();

        Some(Video {
            video_id: video_id.to_string(),
            title: title.to_string(),
            channel_id: channel_id.to_string(),
            channel_name: channel_name.to_string(),
            thumbnail: thumbnail.to_string(),
            duration,
            published_at,
            view_count,
            like_count: 0,
            is_live,
            description,
        })
    }
}

// yt-dlp search fallback when Invidious search is disabled
async fn search_channels_ytdlp(query: &str) -> Result<Vec<Channel>, String> {
    let yt_dlp = yt_dlp_path();
    let extra_path = yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args([
            "--cookies-from-browser", "firefox",
            "--js-runtimes", "node:node",
            "--flat-playlist",
            "--dump-json",
            &format!("ytsearch10:{}", query),
        ])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut channels = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(channel_id) = v["channel_id"].as_str() {
                if seen_ids.insert(channel_id.to_string()) {
                    channels.push(Channel {
                        channel_id: channel_id.to_string(),
                        channel_name: v["channel"].as_str().unwrap_or("Unknown").to_string(),
                        channel_avatar: String::new(),
                        subscriber_count: 0,
                        description: v["description"].as_str().unwrap_or("").to_string(),
                    });
                }
            }
        }
    }

    if channels.is_empty() {
        return Err("No channels found".to_string());
    }
    Ok(channels)
}

// yt-dlp channel videos fallback
async fn get_channel_videos_ytdlp(channel_id: &str) -> Result<Vec<Video>, String> {
    let yt_dlp = yt_dlp_path();
    let url = format!("https://www.youtube.com/channel/{}/videos", channel_id);
    let extra_path = yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args([
            "--cookies-from-browser", "firefox",
            "--js-runtimes", "node:node",
            "--flat-playlist",
            "--dump-json",
            "--playlist-end", "50",
            &url,
        ])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut videos: Vec<Video> = Vec::new();

    for line in stdout.lines().take(50) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let video_id = v["id"].as_str().unwrap_or("").to_string();
            let title = v["title"].as_str().unwrap_or("Untitled").to_string();
            let duration = v["duration"].as_f64().unwrap_or(0.0) as u64;
            let view_count = v["view_count"].as_u64().unwrap_or(0);
            // yt-dlp returns upload_date as "YYYYMMDD" and timestamp as Unix timestamp
            let published_at = v["timestamp"].as_i64()
                .or_else(|| {
                    v["upload_date"].as_str().and_then(|s| {
                        if s.len() == 8 {
                            // Parse YYYYMMDD
                            let year = s[0..4].parse::<i32>().ok()?;
                            let month = s[4..6].parse::<u32>().ok()?;
                            let day = s[6..8].parse::<u32>().ok()?;
                            NaiveDate::from_ymd_opt(year, month, day)
                                .and_then(|d| d.and_hms_opt(0, 0, 0))
                                .map(|dt| Utc.from_utc_datetime(&dt).timestamp())
                        } else { None }
                    })
                })
                .unwrap_or(0);

            if !video_id.is_empty() {
                videos.push(Video {
                    video_id: video_id.clone(),
                    title,
                    channel_id: channel_id.to_string(),
                    channel_name: String::new(),
                    thumbnail: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", video_id),
                    duration,
                    published_at,
                    view_count,
                    like_count: 0,
                    is_live: false,
                    description: String::new(),
                });
            }
        }
    }

    // Fetch real view counts + timestamps via InnerTube API
    if !videos.is_empty() {
        let data = fetch_view_counts_innertube(channel_id).await;
        for video in &mut videos {
            if let Some(&(count, published)) = data.get(&video.video_id) {
                video.view_count = count;
                if published > 0 {
                    video.published_at = published;
                }
            }
        }
    }

    if videos.is_empty() {
        return Err("No videos found".to_string());
    }
    Ok(videos)
}

// yt-dlp live streams fallback - uses the /streams tab
async fn get_channel_live_ytdlp(channel_id: &str) -> Result<Vec<Video>, String> {
    let yt_dlp = yt_dlp_path();
    let url = format!("https://www.youtube.com/channel/{}/streams", channel_id);
    let extra_path = yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args([
            "--cookies-from-browser", "firefox",
            "--js-runtimes", "node:node",
            "--dump-json",
            "--skip-download",
            "--playlist-end", "20",
            &url,
        ])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp live search failed: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut videos = Vec::new();
    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let is_live = v["live_status"].as_str() == Some("is_live");
            let is_upcoming = v["live_status"].as_str() == Some("is_upcoming");
            if is_live || is_upcoming {
                let video_id = v["id"].as_str().unwrap_or("").to_string();
                if video_id.is_empty() { continue; }
                let title = v["title"].as_str().unwrap_or("Untitled").to_string();
                let channel_name = v["channel"].as_str().or_else(|| v["uploader"].as_str()).unwrap_or("Unknown").to_string();
                let channel_id_from_video = v["channel_id"].as_str().or_else(|| v["playlist_channel_id"].as_str()).unwrap_or("").to_string();
                let view_count = v["view_count"].as_u64().unwrap_or(0);
                let duration = v["duration"].as_u64().unwrap_or(0);
                let thumbnail = v["thumbnail"].as_str().unwrap_or("").to_string();
                videos.push(Video {
                    video_id,
                    title,
                    channel_name,
                    channel_id: channel_id_from_video,
                    view_count,
                    duration,
                    thumbnail,
                    published_at: 0,
                    is_live: true,
                    like_count: 0,
                    description: String::new(),
                });
            }
        }
    }

    if videos.is_empty() {
        return Err("No live or upcoming streams found".to_string());
    }
    Ok(videos)
}

// yt-dlp channel info fallback
async fn get_channel_ytdlp(channel_id: &str) -> Result<Channel, String> {
    let yt_dlp = yt_dlp_path();
    let url = format!("https://www.youtube.com/channel/{}", channel_id);
    let ch_id_owned = channel_id.to_string();

    // Run yt-dlp and avatar fetch in parallel
    let (output_result, avatar) = tokio::join!(
        async {
            let extra_path = yt_dlp_extra_path();
            let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
            tokio::process::Command::new(yt_dlp)
                .env("PATH", &full_path)
                .args([
                    "--cookies-from-browser", "firefox",
                    "--js-runtimes", "node:node",
                    "--dump-json",
                    "--skip-download",
                    "--playlist-end", "1",
                    &url,
                ])
                .output()
                .await
                .map_err(|e| format!("yt-dlp not found: {}", e))
        },
        fetch_channel_avatar(&ch_id_owned)
    );

    let output = output_result?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(line) = stdout.lines().next() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let name = v["channel"].as_str()
                .or_else(|| v["uploader"].as_str())
                .or_else(|| v["playlist_channel"].as_str())
                .or_else(|| v["playlist_uploader"].as_str())
                .unwrap_or("Unknown");

            let ch_id = v["channel_id"].as_str()
                .or_else(|| v["playlist_channel_id"].as_str())
                .unwrap_or(channel_id);
            let sub_count = v["channel_follower_count"].as_u64().unwrap_or(0);

            return Ok(Channel {
                channel_id: ch_id.to_string(),
                channel_name: name.to_string(),
                channel_avatar: avatar,
                subscriber_count: sub_count,
                description: v["description"].as_str().unwrap_or("").to_string(),
            });
        }
    }
    Err("Failed to get channel info".to_string())
}

// yt-dlp videos fallback
async fn get_video_ytdlp(video_id: &str) -> Result<Video, String> {
    let yt_dlp = yt_dlp_path();
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let extra_path = yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args(["--cookies-from-browser", "firefox", "--js-runtimes", "node:node", "--dump-json", "--flat-playlist", "--playlist-end", "1", &url])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found: {}", e))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("yt-dlp failed: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(line) = stdout.lines().next() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let title = v["title"].as_str().unwrap_or("Untitled");
            let channel = v["channel"].as_str().unwrap_or("");
            let duration = v["duration"].as_f64().unwrap_or(0.0) as u64;
            let view_count = v["view_count"].as_u64().unwrap_or(0);
            let like_count = v["like_count"].as_u64().unwrap_or(0);
            let description = v["description"].as_str().unwrap_or("");
            let ch_id = v["channel_id"].as_str().unwrap_or("");

            return Ok(Video {
                video_id: video_id.to_string(),
                title: title.to_string(),
                channel_id: ch_id.to_string(),
                channel_name: channel.to_string(),
                thumbnail: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", video_id),
                duration,
                published_at: 0,
                view_count,
                like_count,
                is_live: false,
                description: description.to_string(),
            });
        }
    }
    Err("Failed to get video info".to_string())
}

// yt-dlp suggestions fallback — search similar videos
async fn get_suggestions_ytdlp(query: &str) -> Result<Vec<Video>, String> {
    let yt_dlp = yt_dlp_path();
    let search_url = format!("ytsearch15:{}", query);
    let extra_path = yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args(["--cookies-from-browser", "firefox", "--js-runtimes", "node:node", "--flat-playlist", "--dump-json", "--playlist-end", "15", &search_url])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found: {}", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut videos = Vec::new();

    for line in stdout.lines().take(15) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let vid = v["id"].as_str().unwrap_or("");
            if vid.is_empty() { continue; }
            let ch_name = v["channel"].as_str().unwrap_or("");
            let ch_id = v["channel_id"].as_str().unwrap_or("");
            let dur = v["duration"].as_f64().unwrap_or(0.0) as u64;
            let views = v["view_count"].as_u64().unwrap_or(0);
            videos.push(Video {
                video_id: vid.to_string(),
                title: v["title"].as_str().unwrap_or("Unknown").to_string(),
                channel_id: ch_id.to_string(),
                channel_name: ch_name.to_string(),
                thumbnail: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", vid),
                duration: dur,
                published_at: 0,
                view_count: views,
                like_count: 0,
                is_live: false,
                description: String::new(),
            });
        }
    }
    Ok(videos)
}

// InnerTube API key for YouTube's web client (public constant, embedded in all YouTube web pages).
const INNERTUBE_API_KEY: &str = "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";

/// Call YouTube's InnerTube browse endpoint to get channel info including avatar URL.
async fn fetch_channel_avatar_innertube(channel_id: &str) -> String {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
    {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let url = format!("https://www.youtube.com/youtubei/v1/browse?key={}", INNERTUBE_API_KEY);

    let body = serde_json::json!({
        "browseId": channel_id,
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": "2.20260206.01.00",
                "hl": "en",
                "gl": "US"
            }
        }
    });

    let resp = match client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Origin", "https://www.youtube.com")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(_) => return String::new(),
    };

    // Try to extract avatar URL from various response formats.
    // Format 1: pageHeaderRenderer (modern YouTube UI — most channels)
    if let Some(img) = json
        .pointer("/header/pageHeaderRenderer/content/pageHeaderViewModel/image/decoratedAvatarViewModel/avatar/avatarViewModel/image/sources/0/url")
    {
        if let Some(url) = img.as_str() {
            if !url.is_empty() {
                return url.to_string();
            }
        }
    }

    // Format 2: c4TabbedHeaderRenderer (older YouTube UI)
    if let Some(thumb) = json
        .pointer("/header/c4TabbedHeaderRenderer/author/best_thumbnail/url")
    {
        if let Some(url) = thumb.as_str() {
            if !url.is_empty() {
                return url.to_string();
            }
        }
    }

    // Format 3: c4TabbedHeaderRenderer with avatar array
    if let Some(avatars) = json
        .pointer("/header/c4TabbedHeaderRenderer/author/avatar")
    {
        if let Some(arr) = avatars.as_array() {
            if let Some(last) = arr.last() {
                if let Some(url) = last["url"].as_str() {
                    if !url.is_empty() {
                        return url.to_string();
                    }
                }
            }
        }
    }

    // Format 4: microformat thumbnail
    if let Some(thumb) = json
        .pointer("/microformat/microformatDataRenderer/thumbnail/thumbnails/0/url")
    {
        if let Some(url) = thumb.as_str() {
            if !url.is_empty() {
                return url.to_string();
            }
        }
    }

    // Format 5: channelMetadataRenderer avatar
    if let Some(avatar) = json
        .pointer("/metadata/channelMetadataRenderer/avatar/url")
    {
        if let Some(url) = avatar.as_str() {
            if !url.is_empty() {
                return url.to_string();
            }
        }
    }

    String::new()
}

/// yt-dlp-based fallback: scrape a YouTube @handle page for avatar URL.
/// Much slower but works when InnerTube API is blocked.
async fn fetch_channel_avatar_ytdlp_fallback(channel_id: &str) -> String {
    let yt_dlp = yt_dlp_path();
    let channel_url = format!("https://www.youtube.com/channel/{}", channel_id);

    // Resolve @handle via yt-dlp
    let extra_path = yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let handle_output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args([
            "--cookies-from-browser", "firefox",
            "--js-runtimes", "node:node",
            "--flat-playlist",
            "--playlist-end", "1",
            "--print", "playlist_uploader_id",
            &channel_url,
        ])
        .output()
        .await;

    let handle = match handle_output {
        Ok(o) if o.status.success() => {
            let h = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if h.is_empty() { return String::new(); }
            h.trim_start_matches('@').to_string()
        }
        _ => return String::new(),
    };

    // Scrape the @handle page for yt3.ggpht.com URL
    let html_url = format!("https://www.youtube.com/@{}/", handle);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
    {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let resp = match client
        .get(&html_url)
        .header("Cookie", "CONSENT=PENDING+999")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return String::new(),
    };

    let html = match resp.text().await {
        Ok(t) => t,
        Err(_) => return String::new(),
    };

    // Find yt3.ggpht.com or yt3.googleusercontent.com URL in HTML
    for domain in &["yt3.ggpht.com", "yt3.googleusercontent.com"] {
        if let Some(pos) = html.find(domain) {
            let start = pos;
            let end = (start + 300).min(html.len());
            let slice = &html[start..end];
            if let Some(url_end) = slice.find(|c: char| c == '"' || c == '\'' || c == '>' || c == '<') {
                let avatar_url = format!("https://{}", &slice[..url_end]);
                if !avatar_url.is_empty() {
                    return avatar_url;
                }
            }
        }
    }
    String::new()
}

/// Fetches the channel avatar URL.
/// Primary method: InnerTube API (reliable, no consent bypass needed).
/// Fallback: yt-dlp + HTML scraping (when InnerTube is blocked or fails).
/// Results are cached for 1 hour.
/// Returns empty string if not found.
pub(crate) async fn fetch_channel_avatar(channel_id: &str) -> String {
    // Check cache first
    {
        let cache = AVATAR_CACHE.lock().unwrap();
        if let Some((url, cached_at)) = cache.get(channel_id) {
            if cached_at.elapsed() < AVATAR_CACHE_TTL {
                return url.clone();
            }
        }
    }

    // Primary: InnerTube API
    let avatar = fetch_channel_avatar_innertube(channel_id).await;
    if !avatar.is_empty() {
        let mut cache = AVATAR_CACHE.lock().unwrap();
        cache.insert(channel_id.to_string(), (avatar.clone(), Instant::now()));
        return avatar;
    }

    // Fallback: yt-dlp + HTML scrape
    let avatar = fetch_channel_avatar_ytdlp_fallback(channel_id).await;
    if !avatar.is_empty() {
        let mut cache = AVATAR_CACHE.lock().unwrap();
        cache.insert(channel_id.to_string(), (avatar.clone(), Instant::now()));
    }
    avatar
}

/// Parse YouTube view count text into a u64.
/// Handles "1,234,567 views", "1.2M views", "123K views", "No views", live "watching".
fn parse_view_count(text: &str) -> u64 {
    let text = text.trim().to_lowercase();
    let text = text
        .trim_end_matches(" views")
        .trim_end_matches(" view")
        .trim_end_matches(" watching")
        .trim()
        .to_string();

    if text == "no" || text.is_empty() {
        return 0;
    }

    let text = text.replace(',', "");

    if text.ends_with('m') {
        let num: f64 = text[..text.len() - 1].parse().unwrap_or(0.0);
        return (num * 1_000_000.0) as u64;
    }
    if text.ends_with('k') {
        let num: f64 = text[..text.len() - 1].parse().unwrap_or(0.0);
        return (num * 1_000.0) as u64;
    }
    if text.ends_with('b') {
        let num: f64 = text[..text.len() - 1].parse().unwrap_or(0.0);
        return (num * 1_000_000_000.0) as u64;
    }

    text.parse().unwrap_or(0)
}

/// Parse a duration string like "9:58" or "1:02:30" into seconds.
fn parse_duration_string(text: &str) -> u64 {
    let parts: Vec<&str> = text.split(':').collect();
    match parts.len() {
        1 => parts[0].parse::<u64>().unwrap_or(0),
        2 => {
            parts[0].parse::<u64>().unwrap_or(0) * 60 + parts[1].parse::<u64>().unwrap_or(0)
        }
        3 => {
            parts[0].parse::<u64>().unwrap_or(0) * 3600
                + parts[1].parse::<u64>().unwrap_or(0) * 60
                + parts[2].parse::<u64>().unwrap_or(0)
        }
        _ => 0,
    }
}

/// Parse a relative time string like "2 hours ago", "1 day ago", "3 months ago"
/// into a Unix timestamp (seconds since epoch).
fn parse_relative_time(text: &str) -> Option<i64> {
    let text = text.trim().to_lowercase();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;

    // Extract number and unit
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 3 { return None; }

    let num: i64 = parts[0].parse().ok()?;
    let unit = parts[1];

    let seconds = match unit {
        "second" | "seconds" => num,
        "minute" | "minutes" => num * 60,
        "hour" | "hours" => num * 3600,
        "day" | "days" => num * 86400,
        "week" | "weeks" => num * 604800,
        "month" | "months" => num * 2592000,
        "year" | "years" => num * 31536000,
        _ => return None,
    };

    Some(now - seconds)
}
fn find_video_renderers(value: &serde_json::Value) -> Vec<&serde_json::Value> {
    let mut results = Vec::new();
    find_video_renderers_recursive(value, &mut results);
    results
}

fn find_video_renderers_recursive<'a>(
    value: &'a serde_json::Value,
    results: &mut Vec<&'a serde_json::Value>,
) {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(vr) = map.get("videoRenderer") {
                results.push(vr);
            } else {
                for v in map.values() {
                    find_video_renderers_recursive(v, results);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                find_video_renderers_recursive(v, results);
            }
        }
        _ => {}
    }
}

/// Fetch view counts for all videos in a channel using YouTube's InnerTube API.
/// Makes a single POST request and extracts view counts from videoRenderer items.
/// Returns a map of video_id -> view_count. Returns empty map on any failure.
async fn fetch_view_counts_innertube(channel_id: &str) -> HashMap<String, (u64, i64)> {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
    {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let url = format!(
        "https://www.youtube.com/youtubei/v1/browse?key={}",
        INNERTUBE_API_KEY
    );

    let body = serde_json::json!({
        "browseId": channel_id,
        "params": "EgZ2aWRlb3MQBg==",
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": "2.20260206.01.00",
                "hl": "en",
                "gl": "US"
            }
        }
    });

    let resp = match client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Origin", "https://www.youtube.com")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };

    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(_) => return HashMap::new(),
    };

    let mut result = HashMap::new();
    let renderers = find_video_renderers(&json);

    for vr in renderers {
        if let Some(video_id) = vr["videoId"].as_str() {
            let count = vr["viewCountText"]["simpleText"]
                .as_str()
                .map(parse_view_count)
                .unwrap_or(0);
            // Extract published timestamp from publishedTimeText
            // InnerTube returns relative text like "2 hours ago" in accessibility data,
            // but the actual timestamp is in the hover text or we compute from relative text
            let published_at = vr["publishedTimeText"]["simpleText"]
                .as_str()
                .and_then(|t| parse_relative_time(t))
                .unwrap_or(0);
            result.insert(video_id.to_string(), (count, published_at));
        }
    }

    result
}
/// Parse duration from YouTube accessibility label text.
/// Handles formats like:
///   "Title 8 minutes, 46 seconds"
///   "Title 1:23:45"
///   "Title 12:34"
///   "Title SHORTS"
fn parse_duration_from_accessibility(label: &str) -> Option<u64> {
    // Try to find duration patterns in the label
    // Pattern 1: "X minutes, Y seconds" or "X minute, Y seconds"
    let label_lower = label.to_lowercase();
    
    // Check for SHORTS
    if label_lower.contains("shorts") {
        return Some(0);
    }
    
    // Try "X minutes, Y seconds" pattern
    if let Some(min_pos) = label_lower.find("minute") {
        // Extract minutes number before "minute"
        let before = &label[..min_pos].trim();
        let mins: u64 = before
            .split_whitespace()
            .last()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        
        // Look for seconds after "minute(s),"
        let after_comma = &label[min_pos + 6..];
        let secs: u64 = after_comma
            .split_whitespace()
            .next()
            .and_then(|s| s.trim_matches(|c: char| !c.is_ascii_digit()).parse().ok())
            .unwrap_or(0);
        
        return Some(mins * 60 + secs);
    }
    
    // Try "X seconds" pattern (short videos)
    if let Some(sec_pos) = label_lower.find("second") {
        let before = &label[..sec_pos].trim();
        let secs: u64 = before
            .split_whitespace()
            .last()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        return Some(secs);
    }
    
    // Try HH:MM:SS or MM:SS pattern at the end of the label
    // Split by common delimiters and look for time-like patterns
    for part in label.rsplit(|c: char| c == ' ' || c == '•' || c == '|') {
        let part = part.trim();
        if part.is_empty() { continue; }
        
        let segments: Vec<&str> = part.split(':').collect();
        match segments.len() {
            2 => {
                if let (Ok(m), Ok(s)) = (segments[0].parse::<u64>(), segments[1].parse::<u64>()) {
                    if m < 60 && s < 60 {
                        return Some(m * 60 + s);
                    }
                }
            }
            3 => {
                if let (Ok(h), Ok(m), Ok(s)) = (
                    segments[0].parse::<u64>(),
                    segments[1].parse::<u64>(),
                    segments[2].parse::<u64>(),
                ) {
                    if h < 24 && m < 60 && s < 60 {
                        return Some(h * 3600 + m * 60 + s);
                    }
                }
            }
            _ => {}
        }
    }
    
    None
}

/// Fetch channel videos using YouTube's InnerTube API.
/// Fetches up to 10 pages (~300 videos) using continuation tokens.
async fn get_channel_videos_innertube(channel_id: &str) -> Result<Vec<Video>, String> {
    get_channel_videos_innertube_pages(channel_id, 10).await
}

/// Fetch more videos for a channel using continuation token (called from JS)
pub async fn fetch_more_channel_videos(channel_id: &str, continuation_token: &str) -> Result<Vec<Video>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let url = format!("https://www.youtube.com/youtubei/v1/browse?key={}", INNERTUBE_API_KEY);
    let body = serde_json::json!({
        "continuation": continuation_token,
        "context": { "client": { "clientName": "WEB", "clientVersion": "2.20260206.01.00", "hl": "en", "gl": "US" } }
    });
    let resp = client.post(&url).header("Content-Type", "application/json").header("Origin", "https://www.youtube.com").json(&body).send().await.map_err(|e| format!("InnerTube request failed: {}", e))?;
    let json: serde_json::Value = resp.json().await.map_err(|e| format!("InnerTube JSON parse failed: {}", e))?;
    let videos = parse_innertube_videos(&json);
    Ok(videos)
}

async fn get_channel_videos_innertube_pages(channel_id: &str, max_pages: u32) -> Result<Vec<Video>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
    let url = format!("https://www.youtube.com/youtubei/v1/browse?key={}", INNERTUBE_API_KEY);
    let mut all_videos = Vec::new();
    let mut continuation: Option<String> = None;
    for page in 0..max_pages {
        let body = if let Some(ref token) = continuation {
            serde_json::json!({ "continuation": token, "context": { "client": { "clientName": "WEB", "clientVersion": "2.20260206.01.00", "hl": "en", "gl": "US" } } })
        } else {
            serde_json::json!({ "browseId": channel_id, "params": "EgZ2aWRlb3PyBgQKAjoA", "context": { "client": { "clientName": "WEB", "clientVersion": "2.20260206.01.00", "hl": "en", "gl": "US" } } })
        };
        let resp = client.post(&url).header("Content-Type", "application/json").header("Origin", "https://www.youtube.com").json(&body).send().await.map_err(|e| format!("InnerTube request failed: {}", e))?;
        let json: serde_json::Value = resp.json().await.map_err(|e| format!("InnerTube JSON parse failed: {}", e))?;
        let videos = parse_innertube_videos(&json);
        all_videos.extend(videos);
        continuation = find_continuation_token(&json);
        if continuation.is_none() { break; }
    }
    if all_videos.is_empty() { return get_channel_videos_ytdlp(channel_id).await; }
    Ok(all_videos)
}

fn parse_innertube_videos(json: &serde_json::Value) -> Vec<Video> {
    let mut videos = Vec::new();
    if let Some(tabs) = json["contents"]["twoColumnBrowseResultsRenderer"]["tabs"].as_array() {
        for tab in tabs {
            let tr = match tab.get("tabRenderer") { Some(t) => t, None => continue };
            if tr["title"].as_str().unwrap_or("") != "Videos" && !tr["title"].as_str().unwrap_or("").is_empty() { continue; }
            let items = match tr["content"].get("richGridRenderer").and_then(|r| r["contents"].as_array()) { Some(i) => i, None => continue };
            for item in items { if let Some(v) = parse_rich_item(item) { videos.push(v); } }
            return videos;
        }
    }
    if let Some(actions) = json["onResponseReceivedActions"].as_array() {
        for action in actions {
            if let Some(items) = action["appendContinuationItemsAction"]["continuationItems"].as_array() {
                for item in items { if let Some(v) = parse_rich_item(item) { videos.push(v); } }
            }
        }
    }
    videos
}

fn parse_rich_item(item: &serde_json::Value) -> Option<Video> {
    let rir = item.get("richItemRenderer")?;
    let lvm = rir["content"].get("lockupViewModel")?;
    let video_id = lvm["contentId"].as_str()?.to_string();
    let title = lvm.pointer("/metadata/lockupMetadataViewModel/title/content")?.as_str()?.to_string();
    let thumb = lvm["contentImage"]["thumbnailViewModel"]["image"]["sources"].as_array()
        .and_then(|a| a.iter().find(|s| s["width"].as_u64().map(|w| w >= 244 && w <= 480).unwrap_or(false)).or_else(|| a.last()))
        .and_then(|s| s["url"].as_str()).unwrap_or("").to_string();
    let thumb = if thumb.starts_with("//") { format!("https:{}", thumb) } else if thumb.starts_with("/") { format!("https://i.ytimg.com{}", thumb) } else { thumb };
    let row = lvm.pointer("/metadata/lockupMetadataViewModel/metadata/contentMetadataViewModel/metadataRows/0/metadataParts");
    let views = row.and_then(|p| p[0].get("text")).and_then(|t| t.get("content")).and_then(|c| c.as_str()).map(parse_view_count).unwrap_or(0);
    let published = row.and_then(|p| p.get(1)).and_then(|p| p.get("text")).and_then(|t| t.get("content")).and_then(|c| c.as_str()).and_then(|t| parse_relative_time(t)).unwrap_or(0);
    let duration = lvm.pointer("/rendererContext/accessibilityContext/label").and_then(|v| v.as_str()).and_then(|l| parse_duration_from_accessibility(l)).unwrap_or(0);
    Some(Video { video_id, title, channel_id: String::new(), channel_name: String::new(), thumbnail: thumb, duration, published_at: published, view_count: views, like_count: 0, is_live: false, description: String::new() })
}

fn find_continuation_token(value: &serde_json::Value) -> Option<String> {
    if let Some(obj) = value.as_object() {
        if let Some(tok) = obj.get("continuationItemRenderer").and_then(|c| c["continuationEndpoint"]["continuationCommand"]["token"].as_str()) {
            return Some(tok.to_string());
        }
        for v in obj.values() { if let Some(t) = find_continuation_token(v) { return Some(t); } }
    } else if let Some(arr) = value.as_array() {
        for v in arr { if let Some(t) = find_continuation_token(v) { return Some(t); } }
    }
    None
}
