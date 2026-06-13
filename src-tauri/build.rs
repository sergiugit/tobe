fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = std::env::var("PROFILE").unwrap();
    let icons_src = format!("{}/icons", manifest_dir);
    let icons_dst = format!("{}/target/{}/icons", manifest_dir, profile);

    if std::path::Path::new(&icons_src).exists() {
        let _ = std::fs::create_dir_all(&icons_dst);
        for entry in std::fs::read_dir(&icons_src).unwrap() {
            let entry = entry.unwrap();
            let dst = format!("{}/{}", icons_dst, entry.file_name().to_string_lossy());
            let _ = std::fs::copy(entry.path(), &dst);
        }
    }

    // Also copy icons next to the binary for direct execution (resource_dir = binary dir)
    let binary_dir = format!("{}/target/{}", manifest_dir, profile);
    for icon in &["32x32.png", "128x128.png"] {
        let src = format!("{}/{}", icons_src, icon);
        let dst = format!("{}/{}", binary_dir, icon);
        let _ = std::fs::copy(&src, &dst);
    }

    tauri_build::build();
}
