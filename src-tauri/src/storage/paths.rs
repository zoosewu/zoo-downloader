// Cross-platform path handling

use std::path::PathBuf;

/// Windows: %APPDATA%\zoo-downloader, macOS: ~/Library/Application Support/zoo-downloader,
/// Linux: ~/.config/zoo-downloader. Dev builds: `zoo-downloader-dev` (`APP_DIR`).
pub fn get_app_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(zoo_native_host::APP_DIR)
}

pub fn get_config_file() -> PathBuf {
    get_app_data_dir().join("config.json")
}
