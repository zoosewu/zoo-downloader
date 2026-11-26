//! Tell Chrome and Edge where `zoo-native-host` is. Runs at every start, so moving the app
//! fixes itself; nothing is written when nothing changed. Failures only get logged.

use serde_json::json;
use std::io;
use std::path::{Path, PathBuf};
use zoo_native_host::{extension_origins, HOST_NAME};

const HOST_EXE: &str = if cfg!(windows) { "zoo-native-host.exe" } else { "zoo-native-host" };

pub fn register() {
    // The browsers know one host: the installed app's. A dev build taking it over would make
    // the extension start the dev build.
    if cfg!(debug_assertions) {
        log::info!("dev build: the browser extension stays with the installed app");
        return;
    }
    if let Err(e) = try_register() {
        log::warn!("cannot register the browser extension host: {e}");
    }
}

fn try_register() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let host = exe.parent().map_or_else(|| PathBuf::from(HOST_EXE), |dir| dir.join(HOST_EXE));
    if !host.is_file() {
        log::warn!("{} is missing; the browser extension cannot reach the app", host.display());
    }
    let manifest = manifest(&host);

    #[cfg(windows)]
    {
        let file = crate::storage::paths::get_app_data_dir().join("native-messaging-host.json");
        write_if_changed(&file, &manifest)?;
        for key in REGISTRY_KEYS {
            set_registry_default(key, &file)?;
        }
    }
    #[cfg(unix)]
    for dir in browser_host_dirs() {
        write_if_changed(&dir.join(format!("{HOST_NAME}.json")), &manifest)?;
    }
    Ok(())
}

/// The native messaging host manifest. Built with serde so Windows paths are escaped.
fn manifest(host: &Path) -> String {
    let manifest = json!({
        "name": HOST_NAME,
        "description": "Zoo Downloader",
        "path": host,
        "type": "stdio",
        "allowed_origins": extension_origins().collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&manifest).expect("a manifest always serializes")
}

fn write_if_changed(file: &Path, content: &str) -> io::Result<()> {
    if std::fs::read_to_string(file).is_ok_and(|old| old == content) {
        return Ok(());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, content)
}

#[cfg(windows)]
const REGISTRY_KEYS: [&str; 2] = [
    r"Software\Google\Chrome\NativeMessagingHosts\com.zoo.downloader",
    r"Software\Microsoft\Edge\NativeMessagingHosts\com.zoo.downloader",
];

#[cfg(windows)]
fn set_registry_default(key: &str, file: &Path) -> io::Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let value = file.to_string_lossy().into_owned();
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(key)?;
    if key.get_value::<String, _>("").is_ok_and(|old| old == value) {
        return Ok(());
    }
    key.set_value("", &value)
}

/// `NativeMessagingHosts` of each installed browser (only browsers whose profile folder exists).
#[cfg(unix)]
fn browser_host_dirs() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    const BROWSERS: [&str; 2] = ["Google/Chrome", "Microsoft Edge"];
    #[cfg(not(target_os = "macos"))]
    const BROWSERS: [&str; 2] = ["google-chrome", "microsoft-edge"];
    let Some(config) = dirs::config_dir() else { return Vec::new() };
    BROWSERS.iter().map(|b| config.join(b)).filter(|d| d.is_dir()).map(|d| d.join("NativeMessagingHosts")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_names_the_host_and_only_our_extension() {
        let host = Path::new(r"C:\Program Files\Zoo Downloader\zoo-native-host.exe");
        let text = manifest(host);
        assert!(text.contains(r#""path": "C:\\Program Files\\Zoo Downloader\\zoo-native-host.exe""#), "{text}");
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["name"], "com.zoo.downloader");
        assert_eq!(json["type"], "stdio");
        assert_eq!(
            json["allowed_origins"],
            json!(["chrome-extension://oibblcgobgdenncecbfgdcpeegliejcj/", "chrome-extension://bhgmngdlfelamchfhlahkkclffhpgfma/"])
        );
    }

    #[test]
    fn writes_only_when_the_content_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("hosts").join("m.json");
        write_if_changed(&file, "a").unwrap();
        let written = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_if_changed(&file, "a").unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), written);
        write_if_changed(&file, "b").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "b");
    }
}
