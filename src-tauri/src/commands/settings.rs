use crate::models::Settings;
use std::fs;
use std::path::PathBuf;
use tauri::command;
use tauri::Manager;

fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    app.path().app_data_dir().expect("failed to get app data dir").join("settings.json")
}

#[command]
pub async fn get_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    let path = settings_path(&app);
    if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        let default = Settings::default();
        let json = serde_json::to_string_pretty(&default).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(default)
    }
}

#[command]
pub async fn update_settings(app: tauri::AppHandle, settings: Settings) -> Result<(), String> {
    let path = settings_path(&app);
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}
