// AppSettings model - application settings

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    // General settings
    /// A language code ("zh-TW", "en", "ja", or one of the user's files); "" follows the system.
    pub language: String,
    pub theme: Theme,

    // Download settings
    pub default_download_dir: PathBuf,
    pub max_concurrent_downloads: Option<usize>, // None = unlimited
    pub thread_count: usize,                     // connections per download, default 8
    pub speed_limit_mode: SpeedLimitMode,
    pub speed_limit_kbps: Option<u64>,

    // Proxy settings
    pub enable_proxy: bool,
    /// When every proxy is unusable, connect directly instead of failing.
    #[serde(default = "default_true")]
    pub proxy_fallback_direct: bool,
    pub proxy_verification_interval_hours: u64, // default 24

    // UI settings
    pub enable_clipboard_monitoring: bool,
    /// Downloads the browser extension sends start at once, unless their module says otherwise
    /// (each module has its own switch). Off: they wait in the add window.
    #[serde(default)]
    pub auto_start_browser: bool,
    /// The same for links copied to the clipboard (while it is watched).
    #[serde(default)]
    pub auto_start_copied: bool,
    pub enable_notifications: bool,
    /// A tray icon; closing the window then hides it there instead of quitting.
    pub enable_system_tray: bool,
    /// Start when the user logs in (the OS login item follows this).
    #[serde(default)]
    pub launch_at_startup: bool,
    /// Started at login: stay hidden in the tray (needs the tray icon).
    #[serde(default = "default_true")]
    pub start_in_tray: bool,

    // Advanced settings
    pub auto_retry_enabled: bool,
    pub max_retry_count: u32,
    pub retry_interval_secs: u64,
    /// User-Agent for downloads that do not set their own; `None` uses zoo-downloader's.
    #[serde(default)]
    pub user_agent: Option<String>,

    // Site modules
    /// Look for module updates when the app starts (never installs them).
    #[serde(default = "default_true")]
    pub check_module_updates: bool,
    /// Look for a new version of the app at start and every few hours; installing asks first.
    #[serde(default = "default_true")]
    pub check_app_updates: bool,

    // Categories
    /// The user's categories, in sidebar order; their rules tag new downloads.
    #[serde(default)]
    pub categories: Vec<zoo_core::Category>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SpeedLimitMode {
    Unlimited,
    Fixed,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: String::new(),
            theme: Theme::Dark,
            default_download_dir: dirs::download_dir().unwrap_or_else(|| PathBuf::from(".")),
            max_concurrent_downloads: None,
            thread_count: 8,
            speed_limit_mode: SpeedLimitMode::Unlimited,
            speed_limit_kbps: None,
            enable_proxy: false,
            proxy_fallback_direct: true,
            proxy_verification_interval_hours: 24,
            enable_clipboard_monitoring: true,
            auto_start_browser: false,
            auto_start_copied: false,
            enable_notifications: true,
            enable_system_tray: true,
            launch_at_startup: false,
            start_in_tray: true,
            auto_retry_enabled: true,
            max_retry_count: 3,
            retry_interval_secs: 5,
            user_agent: None,
            check_module_updates: true,
            check_app_updates: true,
            categories: Vec::new(),
        }
    }
}

impl AppSettings {
    /// The download engine's slice of the settings.
    pub fn engine_config(&self) -> zoo_core::Config {
        zoo_core::Config {
            max_concurrent: self.max_concurrent_downloads.filter(|&n| n > 0),
            connections_per_task: self.thread_count.max(1),
            // How a download splits across its connections is the engine's call.
            min_split: zoo_core::Config::default().min_split,
            speed_limit: match self.speed_limit_mode {
                SpeedLimitMode::Fixed => self.speed_limit_kbps.filter(|&k| k > 0).map(|k| k * 1024),
                SpeedLimitMode::Unlimited => None,
            },
            auto_retry: zoo_core::AutoRetry {
                enabled: self.auto_retry_enabled,
                max_attempts: self.max_retry_count,
                interval: std::time::Duration::from_secs(self.retry_interval_secs.max(1)),
            },
            transfer_retry: Default::default(),
            user_agent: self.user_agent.clone().filter(|ua| !ua.trim().is_empty()),
            categories: self.classifier(),
        }
    }

    /// Saved settings were checked (`settings_update`); a hand-edited file may not be, so a
    /// broken category is left out instead of failing the rest.
    fn classifier(&self) -> zoo_core::Classifier {
        zoo_core::Classifier::new(&self.categories).unwrap_or_else(|e| {
            log::warn!("categories: {e}");
            let mut usable: Vec<zoo_core::Category> = Vec::new();
            for c in &self.categories {
                let mut with = usable.clone();
                with.push(c.clone());
                if zoo_core::Classifier::new(&with).is_ok() {
                    usable = with;
                }
            }
            zoo_core::Classifier::new(&usable).unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_files_without_new_fields_still_load() {
        let mut json = serde_json::to_value(AppSettings::default()).unwrap();
        json.as_object_mut().unwrap().remove("proxyFallbackDirect");
        let loaded: AppSettings = serde_json::from_value(json).unwrap();
        assert!(loaded.proxy_fallback_direct);
    }

    #[test]
    fn engine_config_maps_every_engine_field() {
        let s = AppSettings {
            max_concurrent_downloads: Some(2),
            thread_count: 4,
            speed_limit_mode: SpeedLimitMode::Fixed,
            speed_limit_kbps: Some(100),
            auto_retry_enabled: false,
            max_retry_count: 7,
            retry_interval_secs: 9,
            ..AppSettings::default()
        };
        let c = s.engine_config();
        assert_eq!(c.max_concurrent, Some(2));
        assert_eq!(c.connections_per_task, 4);
        assert_eq!(c.speed_limit, Some(100 * 1024));
        assert!(!c.auto_retry.enabled);
        assert_eq!(c.auto_retry.max_attempts, 7);
        assert_eq!(c.auto_retry.interval.as_secs(), 9);

        let unlimited = AppSettings { speed_limit_mode: SpeedLimitMode::Unlimited, ..s }.engine_config();
        assert_eq!(unlimited.speed_limit, None);
        assert_eq!(AppSettings::default().engine_config().max_concurrent, None, "default is unlimited (FR-013)");
    }
}
