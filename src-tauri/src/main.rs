mod commands;
mod models;
mod services;

use commands::*;
use std::path::PathBuf;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("com.tobe.app");
            std::fs::create_dir_all(&app_data)?;

            // Set window icon and install .desktop file on Linux
            #[cfg(target_os = "linux")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    if let Ok(resource_dir) = app.path().resource_dir() {
                        let icon_path = resource_dir.join("icons").join("32x32.png");
                        if icon_path.exists() {
                            if let Ok(icon) = tauri::image::Image::from_path(&icon_path) {
                                let _ = window.set_icon(icon);
                            }
                        }
                    }
                }

                // Install .desktop file + icon for taskbar/dock icon
                if let Ok(home) = std::env::var("HOME") {
                    let apps_dir = format!("{}/.local/share/applications", home);
                    let icons_dir = format!("{}/.local/share/icons/hicolor/128x128/apps", home);
                    let _ = std::fs::create_dir_all(&apps_dir);
                    let _ = std::fs::create_dir_all(&icons_dir);

                    // Copy 128x128 icon
                    if let Ok(resource_dir) = app.path().resource_dir() {
                        let src = resource_dir.join("icons").join("128x128.png");
                        if src.exists() {
                            let _ = std::fs::copy(&src, format!("{}/com.tobe.app.png", icons_dir));
                        }
                    }

                    // Get the binary path for Exec=
                    let exec_path = std::env::current_exe()
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "tobe".to_string());

                    // Write .desktop file
                    let desktop = format!(
                        "[Desktop Entry]\n\
                         Name=Tobe\n\
                         Comment=YouTube desktop client\n\
                         Exec={}\n\
                         Icon=com.tobe.app\n\
                         Type=Application\n\
                         Categories=AudioVideo;Video;Player;\n\
                         StartupWMClass=com.tobe.app\n",
                        exec_path
                    );
                    let _ = std::fs::write(format!("{}/com.tobe.app.desktop", apps_dir), desktop);

                    // Update icon cache
                    let _ = std::process::Command::new("gtk-update-icon-cache")
                        .arg(format!("{}/.local/share/icons/hicolor", home))
                        .stderr(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .status();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_subscriptions,
            add_subscription,
            remove_subscription,
            update_subscription_avatar,
            get_channel,
            get_channel_videos,
            get_channel_live,
            get_subscribed_feed,
            get_video,
            get_suggestions,
            search_channels,
            get_settings,
            update_settings,
            get_video_url,
            get_video_formats,
            get_thumbnails,
            get_channel_avatar,
            get_comments,
            clear_video_cache,
            log_message,
            get_app_version,
            fetch_more_channel_videos,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}