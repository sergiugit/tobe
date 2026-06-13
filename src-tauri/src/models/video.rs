use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Video {
    pub video_id: String,
    pub title: String,
    pub channel_id: String,
    pub channel_name: String,
    pub thumbnail: String,
    pub duration: u64,
    pub published_at: i64,
    pub view_count: u64,
    pub like_count: u64,
    pub is_live: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub channel_id: String,
    pub channel_name: String,
    pub channel_avatar: String,
    pub subscriber_count: u64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoFormat {
    pub format_id: String,
    pub height: u32,
    pub width: u32,
    pub ext: String,
    pub fps: Option<f64>,
    pub filesize: Option<u64>,
    pub note: String,
}
