use crate::models::Subscription;
use crate::services::InvidiousClient;
use std::fs;
use std::path::PathBuf;
use tauri::command;
use tauri::Manager;

fn data_dir(_app: &tauri::AppHandle) -> PathBuf {
    // Use dirs::data_local_dir() which respects the real user home, not $HOME
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("com.tobe.app")
}

fn subscriptions_path(app: &tauri::AppHandle) -> PathBuf {
    data_dir(app).join("subscriptions.json")
}

fn load_subscriptions(app: &tauri::AppHandle) -> Vec<Subscription> {
    let path = subscriptions_path(app);
    if path.exists() {
        let result: Vec<Subscription> = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        result
    } else {
        vec![]
    }
}

fn save_subscriptions(app: &tauri::AppHandle, subs: &[Subscription]) -> Result<(), String> {
    let path = subscriptions_path(app);
    let json = serde_json::to_string_pretty(subs).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[command]
pub async fn get_subscriptions(app: tauri::AppHandle) -> Result<Vec<Subscription>, String> {
    Ok(load_subscriptions(&app))
}

#[command]
pub async fn add_subscription(
    app: tauri::AppHandle,
    channel: crate::models::Channel,
) -> Result<Subscription, String> {
    let mut subs = load_subscriptions(&app);

    // Check if already subscribed
    if subs.iter().any(|s| s.channel_id == channel.channel_id) {
        return Err("Already subscribed".to_string());
    }

    // Try to fetch a real avatar if missing or is a /channel/ redirect URL
    let avatar = if channel.channel_avatar.is_empty() || channel.channel_avatar.contains("/channel/") {
        crate::services::invidious::fetch_channel_avatar(&channel.channel_id).await
    } else {
        channel.channel_avatar.clone()
    };

    let sub = Subscription {
        channel_id: channel.channel_id,
        channel_name: channel.channel_name,
        channel_avatar: avatar,
        subscribed_at: chrono::Utc::now().timestamp(),
    };

    subs.push(sub.clone());
    save_subscriptions(&app, &subs)?;
    Ok(sub)
}

#[command]
pub async fn remove_subscription(
    app: tauri::AppHandle,
    channel_id: String,
) -> Result<(), String> {
    let mut subs = load_subscriptions(&app);
    subs.retain(|s| s.channel_id != channel_id);
    save_subscriptions(&app, &subs)?;
    Ok(())
}

#[command]
pub async fn update_subscription_avatar(
    app: tauri::AppHandle,
    channel_id: String,
    channel_avatar: String,
) -> Result<(), String> {
    let mut subs = load_subscriptions(&app);
    if let Some(sub) = subs.iter_mut().find(|s| s.channel_id == channel_id) {
        sub.channel_avatar = channel_avatar;
        save_subscriptions(&app, &subs)?;
    }
    Ok(())
}
