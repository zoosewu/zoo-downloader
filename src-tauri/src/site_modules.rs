//! Shell side of site modules: the marketplace, the resolver it feeds, and the user answering
//! captchas. Commands live in `commands::site_modules`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{broadcast, oneshot};
use ts_rs::TS;
use zoo_marketplace::{MarketError, Marketplace, Plan, SecretStore};
use zoo_module_client::resolver::ModuleResolver;
use zoo_module_client::manifest::Text;
use zoo_module_client::{Captcha, Interaction, Secret, Supervisor};

/// Installed modules or marketplaces changed (by a command, a refresh or a local edit).
pub const MODULES_EVENT: &str = "modules-changed";
/// A `CaptchaEvent`.
pub const CAPTCHA_EVENT: &str = "captcha-event";

pub struct SiteModules {
    pub market: Arc<Marketplace>,
    pub resolver: Arc<ModuleResolver>,
    pub captchas: Arc<Captchas>,
    /// Downloaded and verified, waiting for the user's consent; by module key.
    plans: Mutex<HashMap<String, Plan>>,
    local: LocalWatch,
}

impl SiteModules {
    pub fn open(data_dir: &Path) -> Result<Self, MarketError> {
        let market = Arc::new(Marketplace::open(&data_dir.join("site-modules"), Arc::new(KeyringSecrets))?);
        let captchas = Arc::new(Captchas::default());
        let resolver = Arc::new(ModuleResolver::new(Arc::new(module_host()), captchas.clone()));
        let modules = Self { market, resolver, captchas, plans: Mutex::default(), local: LocalWatch::default() };
        modules.sync();
        Ok(modules)
    }

    /// Make downloads use the installed modules as they are now, and watch the folders of the
    /// local ones.
    pub fn sync(&self) {
        self.resolver.set_modules(self.market.resolver_modules());
        self.local.follow(self.market.local_dirs());
    }

    pub fn keep_plan(&self, plan: Plan) {
        self.plans.lock().unwrap().insert(plan.summary.key.clone(), plan);
    }

    pub fn take_plan(&self, key: &str) -> Option<Plan> {
        self.plans.lock().unwrap().remove(key)
    }
}

/// The folders of local modules, watched by the OS: an edited manifest is re-read at once.
#[derive(Default)]
struct LocalWatch(Mutex<Option<(notify::RecommendedWatcher, HashSet<PathBuf>)>>);

impl LocalWatch {
    /// Start watching; `changed` runs (on the watcher's thread) when a manifest changes.
    fn start(&self, changed: impl Fn() + Send + 'static) {
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|e| e.paths.iter().any(|p| p.file_name().is_some_and(|n| n == "manifest.json"))) {
                changed();
            }
        });
        match watcher {
            Ok(watcher) => *self.0.lock().unwrap() = Some((watcher, HashSet::new())),
            Err(e) => log::warn!("local modules will not reload by themselves: {e}"),
        }
    }

    /// Watch exactly `dirs`.
    fn follow(&self, dirs: Vec<PathBuf>) {
        use notify::Watcher;
        let mut guard = self.0.lock().unwrap();
        let Some((watcher, watched)) = guard.as_mut() else { return };
        let wanted: HashSet<PathBuf> = dirs.into_iter().collect();
        for gone in watched.difference(&wanted) {
            let _ = watcher.unwatch(gone);
        }
        for dir in wanted.difference(watched) {
            if let Err(e) = watcher.watch(dir, notify::RecursiveMode::NonRecursive) {
                log::warn!("cannot watch {}: {e}", dir.display());
            }
        }
        *watched = wanted;
    }
}

/// The site-module host (`zoo-module serve`), started on first use. It ships next to the app
/// executable: Tauri's externalBin in a bundle, the workspace target directory in development.
fn module_host() -> Supervisor {
    let name = if cfg!(windows) { "zoo-module.exe" } else { "zoo-module" };
    let path = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .unwrap_or_else(|| name.into());
    if !path.exists() {
        log::warn!("site-module host not found at {}; build it with `cargo build -p zoo-module`", path.display());
    }
    Supervisor::new(path, ["serve"])
}

/// How stale a module's catalog may be when the module is used before it is fetched again.
const CHECK_ON_USE: std::time::Duration = std::time::Duration::from_secs(3600);

/// A module started resolving a download: look for a newer version of it, at most every
/// `CHECK_ON_USE`, so the app finds updates while it keeps running. Only shown (the module
/// page's update button), never installed by itself.
pub fn check_update_on_use(app: &AppHandle, key: &str) {
    let (app, key) = (app.clone(), key.to_string());
    tauri::async_runtime::spawn(async move {
        // The same switch as the check at start ("check for module updates").
        if !app.state::<crate::commands::settings::SettingsState>().settings.lock().await.check_module_updates {
            return;
        }
        let modules = app.state::<SiteModules>();
        if modules.market.refresh_for(&key, CHECK_ON_USE).await {
            if let Some(update) = modules.market.installed().into_iter().find(|m| m.key == key).and_then(|m| m.update) {
                log::info!("{key}: version {update} is available");
            }
            modules.sync();
            let _ = app.emit(MODULES_EVENT, ());
        }
    });
}

/// Register the official marketplace, bring back modules whose files are gone, check for
/// updates if the user wants that, relay captchas to the frontend and re-read local modules
/// when their folders change.
pub fn start(app: AppHandle, check_updates: bool) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let modules = handle.state::<SiteModules>();
        let official = official_marketplace();
        if let Err(e) = modules.market.set_official(&official).await {
            log::warn!("cannot load the official marketplace from {official}: {e}");
        }
        let missing = modules.market.restore_missing().await;
        if !missing.is_empty() {
            log::warn!("modules to download again later: {missing:?}");
        }
        if check_updates {
            modules.market.refresh().await;
        }
        modules.sync();
        let _ = handle.emit(MODULES_EVENT, ());
    });

    // A local module's manifest changed: re-read it and tell downloads and the module page.
    let handle = app.clone();
    app.state::<SiteModules>().local.start(move || {
        let modules = handle.state::<SiteModules>();
        match modules.market.reload_local() {
            Ok(true) => {
                modules.sync();
                let _ = handle.emit(MODULES_EVENT, ());
            }
            Ok(false) => {}
            Err(e) => log::warn!("cannot reload local modules: {e}"),
        }
    });
    let modules = app.state::<SiteModules>();
    modules.local.follow(modules.market.local_dirs());

    let mut events = app.state::<SiteModules>().captchas.events.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => {
                    if let CaptchaEvent::Closed(id) = &event {
                        if let Some(window) = app.get_webview_window(&captcha_window(id)) {
                            let _ = window.close();
                        }
                    }
                    let _ = app.emit(CAPTCHA_EVENT, &event);
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    });
}

/// Where the official marketplace is published (docs/architecture/official-marketplace-repo.md).
pub const OFFICIAL_MARKETPLACE: &str = "zoosewu/zoo-downloader-marketplace";

/// The official marketplace; development builds may read a local clone instead
/// (`ZOO_OFFICIAL_MARKETPLACE=<folder>`), whose module edits then reload at once.
fn official_marketplace() -> String {
    let local = cfg!(debug_assertions).then(|| std::env::var("ZOO_OFFICIAL_MARKETPLACE").ok()).flatten();
    local.unwrap_or_else(|| OFFICIAL_MARKETPLACE.into())
}

/// Without Windows' `\\?\` prefix (resource paths have it), so the folder reads as people type it.
pub(crate) fn plain_path(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path,
    }
}

/// Release builds always use the published official marketplace.
#[cfg(test)]
#[test]
fn the_official_marketplace_is_the_github_repository() {
    assert_eq!(OFFICIAL_MARKETPLACE, "zoosewu/zoo-downloader-marketplace");
    if std::env::var_os("ZOO_OFFICIAL_MARKETPLACE").is_none() {
        assert_eq!(official_marketplace(), OFFICIAL_MARKETPLACE);
    }
}

#[cfg(test)]
#[test]
fn verbatim_prefixes_are_dropped() {
    assert_eq!(plain_path(PathBuf::from(r"\\?\D:\app\marketplace")), PathBuf::from(r"D:\app\marketplace"));
    assert_eq!(plain_path(PathBuf::from(r"\\?\UNC\server\share")), PathBuf::from(r"\\?\UNC\server\share"));
    assert_eq!(plain_path(PathBuf::from("/opt/app")), PathBuf::from("/opt/app"));
}

/// Module credentials in the OS keychain.
struct KeyringSecrets;

/// Dev builds keep their own module accounts (`APP_DIR`).
const KEYRING_SERVICE: &str = if cfg!(debug_assertions) { "zoo-downloader-dev.site-modules" } else { "zoo-downloader.site-modules" };

/// The OS credential store, chosen once. Each store's default naming matches what keyring 2
/// wrote, so credentials saved by older versions are still found.
fn keyring_entry(account: &str) -> keyring_core::Result<keyring_core::Entry> {
    static STORE: std::sync::Once = std::sync::Once::new();
    STORE.call_once(|| {
        #[cfg(windows)]
        let store = windows_native_keyring_store::Store::new();
        #[cfg(target_os = "macos")]
        let store = apple_native_keyring_store::keychain::Store::new();
        #[cfg(target_os = "linux")]
        let store = zbus_secret_service_keyring_store::Store::new();
        match store {
            Ok(store) => keyring_core::set_default_store(store),
            Err(e) => log::error!("no OS credential store: {e}"),
        }
    });
    keyring_core::Entry::new(KEYRING_SERVICE, account)
}

impl SecretStore for KeyringSecrets {
    fn get(&self, account: &str) -> Result<Option<Secret>, String> {
        match keyring_entry(account).and_then(|e| e.get_password()) {
            Ok(value) => Ok(Some(Secret(value))),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, account: &str, value: &Secret) -> Result<(), String> {
        keyring_entry(account).and_then(|e| e.set_password(&value.0)).map_err(|e| e.to_string())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        match keyring_entry(account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

// --- captchas ----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum CaptchaEvent {
    /// A module needs the user to solve this.
    Opened(CaptchaRequest),
    /// Answered, declined, or the download stopped waiting for it.
    Closed(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CaptchaRequest {
    pub id: String,
    pub challenge: CaptchaChallenge,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CaptchaChallenge {
    /// Type the text in the image.
    Image { mime: String, base64: String },
    /// Solve it on the site's own page (`captcha_open_window`).
    Recaptcha { site_key: String, page_url: String },
    /// Type what the module asks for (`zoo.ask`): `error` says why the last answer did not work;
    /// a `secret` answer is typed hidden.
    Ask { message: Text, error: Option<Text>, secret: bool },
}

impl From<Captcha> for CaptchaChallenge {
    fn from(c: Captcha) -> Self {
        match c {
            Captcha::Image { mime, base64 } => CaptchaChallenge::Image { mime, base64 },
            Captcha::Recaptcha { site_key, page_url } => CaptchaChallenge::Recaptcha { site_key, page_url },
            Captcha::Ask { message, error, secret, .. } => CaptchaChallenge::Ask { message, error, secret },
        }
    }
}

/// A captcha waiting for the user, and where the answer goes.
type Waiting = (CaptchaRequest, oneshot::Sender<Option<String>>);

/// Captchas waiting for the user. Modules ask through `Interaction`; the frontend answers.
pub struct Captchas {
    pending: Mutex<HashMap<String, Waiting>>,
    events: broadcast::Sender<CaptchaEvent>,
    next: AtomicU64,
}

impl Default for Captchas {
    fn default() -> Self {
        Self { pending: Mutex::default(), events: broadcast::channel(64).0, next: AtomicU64::new(1) }
    }
}

impl Captchas {
    pub fn pending(&self) -> Vec<CaptchaRequest> {
        let mut list: Vec<_> = self.pending.lock().unwrap().values().map(|(r, _)| r.clone()).collect();
        list.sort_by_key(|r| r.id.parse::<u64>().unwrap_or(0));
        list
    }

    /// Answer (or decline with `None`); `false` if nothing waits under `id` any more.
    pub fn answer(&self, id: &str, answer: Option<String>) -> bool {
        match self.pending.lock().unwrap().remove(id) {
            Some((_, reply)) => reply.send(answer.filter(|a| !a.is_empty())).is_ok(),
            None => false,
        }
    }

    pub fn recaptcha_page(&self, id: &str) -> Option<String> {
        match &self.pending.lock().unwrap().get(id)?.0.challenge {
            CaptchaChallenge::Recaptcha { page_url, .. } => Some(page_url.clone()),
            CaptchaChallenge::Image { .. } | CaptchaChallenge::Ask { .. } => None,
        }
    }

    #[cfg(test)]
    fn subscribe(&self) -> broadcast::Receiver<CaptchaEvent> {
        self.events.subscribe()
    }
}

/// Announces `Closed` however the wait ends, including the download being paused.
struct Closing<'a> {
    captchas: &'a Captchas,
    id: String,
}

impl Drop for Closing<'_> {
    fn drop(&mut self) {
        self.captchas.pending.lock().unwrap().remove(&self.id);
        let _ = self.captchas.events.send(CaptchaEvent::Closed(self.id.clone()));
    }
}

impl Interaction for Captchas {
    fn solve_captcha(&self, challenge: Captcha) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        Box::pin(async move {
            let id = self.next.fetch_add(1, Ordering::Relaxed).to_string();
            let request = CaptchaRequest { id: id.clone(), challenge: challenge.into() };
            let (reply, answer) = oneshot::channel();
            self.pending.lock().unwrap().insert(id.clone(), (request.clone(), reply));
            let _closing = Closing { captchas: self, id };
            let _ = self.events.send(CaptchaEvent::Opened(request));
            answer.await.ok().flatten()
        })
    }
}

pub fn captcha_window(id: &str) -> String {
    format!("captcha-{id}")
}

/// Navigating here hands the token over; nothing is ever loaded from it.
pub const TOKEN_HOST: &str = "zoo-captcha.invalid";

/// Runs in the site's page: once the user solved the reCAPTCHA, navigate to `TOKEN_HOST` with
/// the token. The page gets no access to the app (the window has no capabilities).
pub const RECAPTCHA_SCRIPT: &str = r#"
(function () {
  const timer = setInterval(function () {
    let token = '';
    try {
      token = (window.grecaptcha && window.grecaptcha.getResponse && window.grecaptcha.getResponse()) || '';
    } catch (e) {}
    if (!token) {
      const field = document.querySelector('textarea[name="g-recaptcha-response"]');
      token = field ? field.value : '';
    }
    if (token) {
      clearInterval(timer);
      window.location.href = 'https://zoo-captcha.invalid/?token=' + encodeURIComponent(token);
    }
  }, 500);
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_module_folders_report_manifest_edits_while_followed() {
        use std::time::Duration;
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let watch = LocalWatch::default();
        watch.start(move || {
            let _ = tx.send(());
        });
        watch.follow(vec![dir.path().to_path_buf()]);

        std::fs::write(dir.path().join("other.txt"), "x").unwrap();
        std::fs::write(dir.path().join("manifest.json"), "{}").unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("a manifest edit is reported");
        while rx.recv_timeout(Duration::from_millis(300)).is_ok() {} // one save can be several events

        watch.follow(vec![]);
        std::fs::write(dir.path().join("manifest.json"), "{ }").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "an unfollowed folder is not watched");
    }

    #[tokio::test]
    async fn a_captcha_waits_for_its_answer_and_always_closes() {
        let captchas = Arc::new(Captchas::default());
        let mut events = captchas.subscribe();
        let challenge = Captcha::Image { mime: "image/png".into(), base64: "AAAA".into() };

        let asking = { let c = captchas.clone(); let ch = challenge.clone(); tokio::spawn(async move { c.solve_captcha(ch).await }) };
        let CaptchaEvent::Opened(request) = events.recv().await.unwrap() else { panic!("opened first") };
        assert_eq!(captchas.pending(), vec![request.clone()]);
        assert!(captchas.answer(&request.id, Some("xk7p".into())));
        assert_eq!(asking.await.unwrap().as_deref(), Some("xk7p"));
        assert_eq!(events.recv().await.unwrap(), CaptchaEvent::Closed(request.id.clone()));
        assert!(!captchas.answer(&request.id, Some("late".into())), "answered once");

        // The download stops waiting (the future is dropped): the captcha closes by itself.
        let asking = { let c = captchas.clone(); tokio::spawn(async move { c.solve_captcha(challenge).await }) };
        let CaptchaEvent::Opened(request) = events.recv().await.unwrap() else { panic!() };
        asking.abort();
        assert_eq!(events.recv().await.unwrap(), CaptchaEvent::Closed(request.id));
        assert!(captchas.pending().is_empty());
    }

    #[test]
    fn challenges_serialize_for_the_frontend() {
        let request = CaptchaRequest {
            id: "1".into(),
            challenge: Captcha::Recaptcha { site_key: "k".into(), page_url: "https://x/".into() }.into(),
        };
        assert_eq!(
            serde_json::to_value(CaptchaEvent::Opened(request)).unwrap(),
            serde_json::json!({ "type": "opened", "data": { "id": "1", "challenge": { "kind": "recaptcha", "siteKey": "k", "pageUrl": "https://x/" } } })
        );
    }
}
