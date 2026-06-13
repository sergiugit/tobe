use crate::models::Video;
use crate::models::VideoFormat;
use crate::services::InvidiousClient;
use tauri::command;
use tauri::Manager;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const VIDEO_CACHE_TTL_SECS: u64 = 600; // 10 minutes

/// Get the video cache directory
fn get_video_cache_dir() -> Result<PathBuf, String> {
    let cache_dir = PathBuf::from("/home/sergiu/.local/share/tobe/video_cache");
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create video cache dir: {}", e))?;
    Ok(cache_dir)
}

/// Load cached videos for a channel if the cache is still fresh
fn load_channel_cache(channel_id: &str) -> Option<Vec<Video>> {
    let cache_dir = get_video_cache_dir().ok()?;
    let cache_file = cache_dir.join(format!("{}.json", channel_id));
    
    let content = std::fs::read_to_string(&cache_file).ok()?;
    let cached: serde_json::Value = serde_json::from_str(&content).ok()?;
    
    let timestamp = cached.get("timestamp")?.as_u64()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    
    // Check if cache is still fresh
    if now - timestamp > VIDEO_CACHE_TTL_SECS {
        return None;
    }
    
    let videos_json = cached.get("videos")?;
    let videos: Vec<Video> = serde_json::from_value(videos_json.clone()).ok()?;
    
    Some(videos)
}

/// Save videos to cache for a channel
fn save_channel_cache(channel_id: &str, videos: &[Video]) {
    let Ok(cache_dir) = get_video_cache_dir() else { return };
    let cache_file = cache_dir.join(format!("{}.json", channel_id));
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    let cached = serde_json::json!({
        "timestamp": now,
        "videos": videos,
    });
    
    if let Ok(json) = serde_json::to_string_pretty(&cached) {
        let _ = std::fs::write(&cache_file, json);
    }
}

fn get_thumb_cache(_app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let cache = PathBuf::from("/home/sergiu/.local/share/com.tobe.app/thumb_cache");
    std::fs::create_dir_all(&cache).map_err(|e| format!("Failed to create cache dir: {}", e))?;
    Ok(cache)
}

#[command]
pub async fn get_thumbnails(
    app: tauri::AppHandle,
    thumbnails: Vec<(String, String)>,
) -> Result<Vec<(String, String)>, String> {
    let cache_dir = get_thumb_cache(&app)?;
    let mut results = Vec::new();

    for (video_id, url) in &thumbnails {
        let cache_path = cache_dir.join(format!("{}.jpg", video_id));
        let mut cached = false;
        if !cache_path.exists() {
            if let Ok(resp) = reqwest::get(url).await {
                if resp.status().is_success() {
                    if let Ok(bytes) = resp.bytes().await {
                        if std::fs::write(&cache_path, &bytes).is_ok() {
                            cached = true;
                        }
                    }
                }
            }
        } else {
            cached = true;
        }

        if cached {
            let abs_path = cache_path.to_string_lossy().to_string();
            // Asset protocol URL accessible from the webview
            let asset_url = format!("https://asset.localhost/{}", urlencoding::encode(&abs_path));
            results.push((video_id.clone(), asset_url));
        } else {
            // Cache file not available — keep using the original CDN URL
            results.push((video_id.clone(), url.clone()));
        }
    }
    Ok(results)
}

#[command]
pub async fn get_channel(
    channel_id: String,
    invidious_instance: String,
) -> Result<crate::models::Channel, String> {
    let client = InvidiousClient::new(invidious_instance);
    client.get_channel(&channel_id).await
}

#[command]
pub async fn get_channel_videos(
    channel_id: String,
    page: u32,
    invidious_instance: String,
) -> Result<Vec<Video>, String> {
    // Try cache first
    if let Some(cached) = load_channel_cache(&channel_id) {
        return Ok(cached);
    }
    
    // Fetch from network
    let client = InvidiousClient::new(invidious_instance);
    let videos = client.get_channel_videos(&channel_id, page).await?;
    
    // Save to cache
    save_channel_cache(&channel_id, &videos);
    
    Ok(videos)
}

#[command]
pub async fn get_channel_live(
    channel_id: String,
    invidious_instance: String,
) -> Result<Vec<Video>, String> {
    let client = InvidiousClient::new(invidious_instance);
    client.get_channel_live(&channel_id).await
}

#[command]
pub async fn get_subscribed_feed(
    subscriptions: Vec<serde_json::Value>,
    page: u32,
    invidious_instance: String,
) -> Result<Vec<Video>, String> {
    let t0 = std::time::Instant::now();
    let mut all_videos = Vec::new();
    let mut uncached_channels: Vec<String> = Vec::new();

    // First pass: load from cache
    for sub in &subscriptions {
        if let Some(channel_id) = sub["channel_id"].as_str() {
            if let Some(cached) = load_channel_cache(channel_id) {
                all_videos.extend(cached);
            } else {
                uncached_channels.push(channel_id.to_string());
            }
        }
    }


    // Second pass: fetch uncached channels in parallel
    if !uncached_channels.is_empty() {
        let mut handles = Vec::new();
        for channel_id in &uncached_channels {
            let client = InvidiousClient::new(invidious_instance.clone());
            let channel_id = channel_id.clone();
            handles.push(tokio::spawn(async move {
                let result = client.get_channel_videos(&channel_id, page).await;
                (channel_id, result)
            }));
        }

        for handle in handles {
            match handle.await {
                Ok((channel_id, Ok(videos))) => {
                    eprintln!("[feed] channel {}: {} videos", channel_id, videos.len());
                    all_videos.extend(videos);
                }
                Ok((channel_id, Err(e))) => {
                    eprintln!("[feed] channel {} error: {}", channel_id, e);
                }
                _ => {}
            }
        }
        eprintln!("[feed] total videos after parallel fetch: {}", all_videos.len());
    }

    // Sort by published_at descending (newest first)
    all_videos.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    Ok(all_videos)
}

#[command]
pub async fn get_video(
    video_id: String,
    invidious_instance: String,
) -> Result<Video, String> {
    let client = InvidiousClient::new(invidious_instance);
    client.get_video(&video_id).await
}

#[command]
pub async fn get_suggestions(
    query: String,
    invidious_instance: String,
) -> Result<Vec<Video>, String> {
    let client = InvidiousClient::new(invidious_instance);
    client.get_suggestions(&query).await
}

#[command]
pub async fn search_channels(
    query: String,
    invidious_instance: String,
) -> Result<Vec<crate::models::Channel>, String> {
    let client = InvidiousClient::new(invidious_instance);
    client.search_channels(&query).await
}

#[command]
pub async fn get_video_url(
    video_id: String,
    format_id: Option<String>,
) -> Result<String, String> {
    let yt_dlp = crate::services::invidious::yt_dlp_path();
    let node_path = String::from("node");
    let url = format!("https://www.youtube.com/watch?v={}", video_id);

    // Determine format string
    let fmt = match &format_id {
        Some(fid) => fid.as_str(),
        None => "best[protocol^=https]",
    };

    let extra_path = crate::services::invidious::yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args(["--remote-components", "ejs:github", "-f", fmt, "-g", &url])
        .output()
        .await
        .map_err(|e| format!("yt-dlp not found: {}", e))?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !url.is_empty() {
            return Ok(url);
        }
    }
    Err("Failed to get video URL".to_string())
}

#[command]
pub async fn get_video_formats(
    video_id: String,
) -> Result<Vec<VideoFormat>, String> {
    let yt_dlp = crate::services::invidious::yt_dlp_path();
    let node_path = String::from("node");
    let url = format!("https://www.youtube.com/watch?v={}", video_id);

    let extra_path = crate::services::invidious::yt_dlp_extra_path();
    let full_path = format!("{}:{}", extra_path, std::env::var("PATH").unwrap_or_default());
    let output = tokio::process::Command::new(yt_dlp)
        .env("PATH", &full_path)
        .args(["--remote-components", "ejs:github", "--dump-json", "--skip-download", &url])
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
            let mut seen_heights: std::collections::HashSet<u32> = std::collections::HashSet::new();
            let mut formats: Vec<VideoFormat> = Vec::new();

            if let Some(fmts) = v["formats"].as_array() {
                for f in fmts.iter().rev() {
                    let height = f["height"].as_u64().unwrap_or(0) as u32;
                    let vcodec = f["vcodec"].as_str().unwrap_or("none");
                    let acodec = f["acodec"].as_str().unwrap_or("none");
                    let ext = f["ext"].as_str().unwrap_or("").to_string();

                    // Track unique heights that have combined video+audio
                    if height > 0 && vcodec != "none" && acodec != "none" && !seen_heights.contains(&height) {
                        seen_heights.insert(height);
                        // Use a format expression so yt-dlp picks the best available
                        // progressive format at or below this resolution
                        let format_expr = format!("best[height<={}][protocol^=https]", height);
                        let filesize = f["filesize"].as_u64().or_else(|| f["filesize_approx"].as_u64());
                        let fps = f["fps"].as_f64();
                        let note = if let Some(fps_val) = fps {
                            format!("{}p {} {}fps", height, ext, fps_val as u32)
                        } else {
                            format!("{}p {}", height, ext)
                        };
                        formats.push(VideoFormat {
                            format_id: format_expr,
                            height,
                            width: f["width"].as_u64().unwrap_or(0) as u32,
                            ext,
                            fps,
                            filesize,
                            note,
                        });
                    }
                }
            }

            // Sort by height descending
            formats.sort_by(|a, b| b.height.cmp(&a.height));

            return Ok(formats);
        }
    }
    Err("Failed to parse formats".to_string())
}

#[command]
pub async fn get_channel_avatar(
    channel_id: String,
) -> Result<String, String> {
    let avatar = crate::services::fetch_channel_avatar(&channel_id).await;
    Ok(avatar)
}

#[command]
pub async fn get_comments(
    video_id: String,
    invidious_instance: String,
) -> Result<crate::models::CommentsResponse, String> {
    let client = InvidiousClient::new(invidious_instance);
    client.get_comments(&video_id).await
}

#[command]
pub async fn clear_video_cache() -> Result<(), String> {
    let cache_dir = get_video_cache_dir()?;
    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to clear cache: {}", e))?;
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to recreate cache dir: {}", e))?;
    }
    Ok(())
}

#[command]
pub async fn log_message(msg: String) {
    eprintln!("[JS] {}", msg);
}

#[command]
pub async fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[command]
pub async fn fetch_more_channel_videos(channel_id: String, continuation_token: String) -> Result<Vec<Video>, String> {
    crate::services::invidious::fetch_more_channel_videos(&channel_id, &continuation_token).await
}
