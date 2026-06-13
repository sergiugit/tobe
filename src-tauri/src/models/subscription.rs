use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub channel_id: String,
    pub channel_name: String,
    pub channel_avatar: String,
    pub subscribed_at: i64,
}
