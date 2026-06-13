use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub author: String,
    pub author_id: String,
    pub author_url: String,
    pub author_thumbnails: Vec<Thumbnail>,
    pub verified: bool,
    pub like_count: u64,
    pub is_pinned: bool,
    pub comment_id: String,
    pub content: String,
    pub content_html: Option<String>,
    pub published: u64,
    pub published_text: String,
    pub replies: Option<CommentReplies>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thumbnail {
    pub url: String,
    pub width: u64,
    pub height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentReplies {
    pub reply_count: u64,
    pub continuation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsResponse {
    pub comment_count: u64,
    pub video_id: String,
    pub comments: Vec<Comment>,
}
