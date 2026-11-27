//! zoo-marketplace: where site modules come from (ADR-0001).
//!
//! A marketplace is a catalog (`zoo-marketplace.json`) the user added from GitHub, an HTTPS
//! URL, a git repository or a local folder. Modules install as `id@marketplace` in two steps:
//! `plan_install` downloads and verifies, `install` commits once the user consented to the
//! permissions. The host only ever gets permissions the user consented to (`resolver_modules`).
//! Everything installed works offline.
//! Design: docs/architecture/phase-4-site-modules.md

pub mod catalog;
pub mod secrets;
mod source;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zoo_core::store::JsonStore;
use zoo_module_client::manifest::{url_regex, CredentialField, Manifest, Permissions, Text};
use zoo_module_client::resolver::InstalledModule;
use zoo_module_client::{Grants, Limits, ModuleRef, Secret};

pub use catalog::{Catalog, CatalogModule};
pub use secrets::{MemorySecrets, SecretStore};
use source::{Location, Root};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum MarketError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("{0}")]
    Git(String),
    #[error("file error: {0}")]
    Io(String),
    /// A downloaded module is not the one the catalog pinned.
    #[error("{0} does not match the sha256 in the catalog")]
    Integrity(String),
    #[error("credential store: {0}")]
    Secrets(String),
}

fn io(e: std::io::Error) -> MarketError {
    MarketError::Io(e.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Trust {
    /// Ships with the app.
    Official,
    Community,
}

/// Permissions as the user sees them when consenting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PermissionSummary {
    pub allow_domains: Vec<String>,
    pub credentials: Vec<CredentialSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CredentialSpec {
    pub key: String,
    pub label: Text,
    pub secret: bool,
    pub optional: bool,
    /// How to get the value (shown under the field).
    #[serde(default)]
    pub help: Option<Text>,
    #[serde(default)]
    pub help_url: Option<String>,
}

impl From<&CredentialField> for CredentialSpec {
    fn from(c: &CredentialField) -> Self {
        Self {
            key: c.key.clone(),
            label: c.label.clone(),
            secret: c.secret,
            optional: c.optional,
            help: c.help.clone(),
            help_url: c.help_url.clone(),
        }
    }
}

impl PermissionSummary {
    fn of(p: &Permissions) -> Self {
        Self {
            allow_domains: p.network.allow_domains.clone(),
            credentials: p.credentials.iter().map(CredentialSpec::from).collect(),
        }
    }

    /// What `self` asks for beyond `granted`.
    fn beyond(&self, granted: &PermissionSummary) -> Self {
        Self {
            allow_domains: self.allow_domains.iter().filter(|d| !granted.allow_domains.contains(d)).cloned().collect(),
            credentials: self.credentials.iter().filter(|c| !granted.credentials.iter().any(|g| g.key == c.key)).cloned().collect(),
        }
    }

    /// What `self` asks for that `granted` allows.
    fn within(&self, granted: &PermissionSummary) -> Self {
        Self {
            allow_domains: self.allow_domains.iter().filter(|d| granted.allow_domains.contains(d)).cloned().collect(),
            credentials: self.credentials.iter().filter(|c| granted.credentials.iter().any(|g| g.key == c.key)).cloned().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.allow_domains.is_empty() && self.credentials.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub name: String,
    /// What the user added: owner/repo, URL or folder.
    pub location: String,
    pub trust: Trust,
    pub owner: Option<String>,
    pub modules: Vec<CatalogModule>,
    pub refreshed_at: DateTime<Utc>,
    /// The checked-out commit of a git marketplace.
    pub commit: Option<String>,
    /// Why the last refresh failed; the catalog shown is the one before.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InstalledInfo {
    /// `id@marketplace`.
    pub key: String,
    pub id: String,
    pub marketplace: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub enabled: bool,
    /// Copied links this module resolves may start without the add-download page (only while
    /// the app-wide switch is on too). The manifest's `autoStart` unless the user changed it.
    pub auto_start: bool,
    pub trust: Trust,
    /// Read in place from a local folder (development).
    pub local: bool,
    pub url_patterns: Vec<String>,
    /// What the module may use now: requested and consented.
    pub permissions: PermissionSummary,
    pub credentials: Vec<CredentialStatus>,
    /// Shown above the account fields (the manifest's `credentialsHelp`).
    pub credentials_help: Option<Text>,
    pub credentials_help_url: Option<String>,
    /// The module's manifest asks for more than the user consented to (a local module changed).
    pub needs_consent: bool,
    /// A newer version in the catalog that this app runs.
    pub update: Option<String>,
    /// A newer version in the catalog that needs a newer Zoo Downloader.
    pub update_needs_newer_app: Option<String>,
    /// Its files are gone; it does not run until it is downloaded again (`install`).
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    #[serde(flatten)]
    pub spec: CredentialSpec,
    /// A value is stored (values are never shown).
    pub set: bool,
}

/// What installing (or updating) a module means, for the consent dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub key: String,
    pub name: String,
    pub version: String,
    /// Installed version, when this is an update.
    pub current_version: Option<String>,
    pub trust: Trust,
    pub permissions: PermissionSummary,
    /// Permissions not consented to before (all of them for a new install).
    pub added: PermissionSummary,
    /// The user must see and accept the permissions: a new install, or an update asking for more.
    pub needs_consent: bool,
}

/// A downloaded and verified module, not installed yet. `install` it once the user consented.
pub struct Plan {
    pub summary: PlanSummary,
    id: String,
    marketplace: String,
    manifest: Manifest,
    files: Files,
}

enum Files {
    Copy { manifest: Vec<u8>, entry: Vec<u8> },
    /// A local module directory, used where it is.
    InPlace(PathBuf),
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    sources: Vec<SourceRecord>,
    /// In priority order: the first module whose patterns match resolves.
    installed: Vec<InstalledRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceRecord {
    name: String,
    location: String,
    trust: Trust,
    catalog: Catalog,
    refreshed_at: DateTime<Utc>,
    commit: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledRecord {
    id: String,
    marketplace: String,
    enabled: bool,
    /// The user's choice; `None` follows the manifest.
    #[serde(default)]
    auto_start: Option<bool>,
    manifest: Manifest,
    /// What the user consented to.
    granted: Permissions,
    /// Holds manifest.json and the entry.
    dir: PathBuf,
    local: bool,
    update: Option<String>,
    /// A newer version this app is too old to run.
    #[serde(default)]
    update_needs_newer_app: Option<String>,
    installed_at: DateTime<Utc>,
}

impl InstalledRecord {
    fn key(&self) -> String {
        format!("{}@{}", self.id, self.marketplace)
    }

    /// The module's files are there (an app update removes the copy older versions ran in place).
    fn has_files(&self) -> bool {
        self.dir.join(&self.manifest.entry).is_file()
    }

    fn effective(&self) -> PermissionSummary {
        PermissionSummary::of(&self.manifest.permissions).within(&PermissionSummary::of(&self.granted))
    }
}

pub struct Marketplace {
    dir: PathBuf,
    store: JsonStore<State>,
    state: Mutex<State>,
    http: reqwest::Client,
    secrets: Arc<dyn SecretStore>,
}

impl Marketplace {
    /// Load `dir/marketplace.json`; installed modules live under `dir/modules`.
    pub fn open(dir: &Path, secrets: Arc<dyn SecretStore>) -> Result<Self, MarketError> {
        let store = JsonStore::new(dir.join("marketplace.json"));
        let state = store.load().map_err(io)?.unwrap_or_default();
        Ok(Self { dir: dir.to_path_buf(), store, state: Mutex::new(state), http: source::http_client(), secrets })
    }

    /// Change the state and save it; nothing changes if `f` fails.
    fn change<T>(&self, f: impl FnOnce(&mut State) -> Result<T, MarketError>) -> Result<T, MarketError> {
        let mut state = self.state.lock().unwrap();
        let mut draft = State { sources: state.sources.clone(), installed: state.installed.clone() };
        let value = f(&mut draft)?;
        self.store.save(&draft).map_err(io)?;
        *state = draft;
        Ok(value)
    }

    fn git_cache(&self) -> PathBuf {
        self.dir.join("git")
    }

    async fn load(&self, location: &Location) -> Result<(Catalog, Option<String>), MarketError> {
        let (root, commit) = source::materialize(location, &self.git_cache()).await?;
        Ok((root.catalog(&self.http).await?, commit))
    }

    // --- sources ---------------------------------------------------------------------------

    /// Add a marketplace: `owner/repo` (GitHub), an https:// catalog URL, a git URL or a folder.
    pub async fn add_source(&self, input: &str) -> Result<SourceInfo, MarketError> {
        self.add(input.trim(), Trust::Community).await
    }

    /// The official marketplace (GitHub, or a folder in development). Set on every start; it
    /// cannot be removed.
    pub async fn set_official(&self, location: &str) -> Result<SourceInfo, MarketError> {
        self.add(location, Trust::Official).await
    }

    async fn add(&self, input: &str, trust: Trust) -> Result<SourceInfo, MarketError> {
        let location = Location::parse(input)?;
        let (catalog, commit) = self.load(&location).await?;
        let record = SourceRecord {
            name: catalog.name.clone(),
            location: input.to_string(),
            trust,
            catalog,
            refreshed_at: Utc::now(),
            commit,
            error: None,
        };
        let info = source_info(&record);
        self.change(|st| {
            match st.sources.iter_mut().find(|s| s.name == record.name) {
                // The official marketplace moves with the app.
                Some(existing) if existing.location == record.location || (trust == Trust::Official && existing.trust == Trust::Official) => {
                    *existing = record;
                }
                Some(existing) => {
                    return Err(MarketError::Invalid(format!(
                        "a marketplace named {:?} is already added from {}",
                        existing.name, existing.location
                    )))
                }
                None => st.sources.push(record),
            }
            mark_updates(st);
            Ok(())
        })?;
        Ok(info)
    }

    pub fn sources(&self) -> Vec<SourceInfo> {
        self.state.lock().unwrap().sources.iter().map(source_info).collect()
    }

    /// Remove a marketplace and every module installed from it.
    pub fn remove_source(&self, name: &str) -> Result<(), MarketError> {
        let removed = self.change(|st| {
            let source = st.sources.iter().find(|s| s.name == name).ok_or_else(|| MarketError::NotFound(name.into()))?;
            if source.trust == Trust::Official {
                return Err(MarketError::Invalid("the official marketplace cannot be removed".into()));
            }
            let location = source.location.clone();
            st.sources.retain(|s| s.name != name);
            let (gone, kept) = std::mem::take(&mut st.installed).into_iter().partition(|m| m.marketplace == name);
            st.installed = kept;
            Ok((location, gone))
        })?;
        let (location, gone): (String, Vec<InstalledRecord>) = removed;
        for module in &gone {
            self.forget(module);
        }
        if let Ok(Location::Git(url)) = Location::parse(&location) {
            let _ = std::fs::remove_dir_all(source::git_dir(&self.git_cache(), &url));
        }
        Ok(())
    }

    /// Fetch every catalog again (git marketplaces pull), note available updates and re-read
    /// local modules. A marketplace that cannot be reached keeps its last catalog and an error.
    pub async fn refresh(&self) -> Vec<SourceInfo> {
        let sources: Vec<(String, String)> =
            self.state.lock().unwrap().sources.iter().map(|s| (s.name.clone(), s.location.clone())).collect();
        for (name, location) in sources {
            self.refresh_one(&name, &location).await;
        }
        if let Err(e) = self.reload_local() {
            log::warn!("cannot reload local modules: {e}");
        }
        self.sources()
    }

    /// Fetch the catalog of the marketplace `key` (`id@marketplace`) came from, unless that was
    /// done less than `max_age` ago: a module in use finds its updates while the app keeps
    /// running. True when it was fetched.
    pub async fn refresh_for(&self, key: &str, max_age: std::time::Duration) -> bool {
        let Ok((_, marketplace)) = split_key(key) else { return false };
        // Claimed before fetching: several downloads starting at once fetch it once.
        let due = self.change(|st| {
            let stale = |s: &&mut SourceRecord| (Utc::now() - s.refreshed_at).to_std().is_ok_and(|age| age >= max_age);
            let source = st.sources.iter_mut().find(|s| s.name == marketplace).filter(stale);
            Ok(source.map(|s| {
                s.refreshed_at = Utc::now();
                s.location.clone()
            }))
        });
        let Ok(Some(location)) = due else { return false };
        self.refresh_one(marketplace, &location).await;
        true
    }

    async fn refresh_one(&self, name: &str, location: &str) {
        let loaded = match Location::parse(location) {
            Ok(location) => self.load(&location).await,
            Err(e) => Err(e),
        };
        let result = self.change(|st| {
            let Some(record) = st.sources.iter_mut().find(|s| s.name == name) else { return Ok(()) };
            record.refreshed_at = Utc::now();
            match loaded {
                Ok((catalog, _)) if catalog.name != name => {
                    record.error = Some(format!("the catalog is now named {:?}; remove and add it again", catalog.name));
                }
                Ok((catalog, commit)) => {
                    (record.catalog, record.commit, record.error) = (catalog, commit, None);
                }
                Err(e) => record.error = Some(e.to_string()),
            }
            mark_updates(st);
            Ok(())
        });
        if let Err(e) = result {
            log::warn!("cannot save marketplace state: {e}");
        }
    }

    // --- installing ------------------------------------------------------------------------

    /// Download and verify `id@marketplace` (from the catalog as last refreshed) without
    /// installing it.
    pub async fn plan_install(&self, key: &str) -> Result<Plan, MarketError> {
        let (id, marketplace) = split_key(key)?;
        let (location, listed, trust, current) = {
            let st = self.state.lock().unwrap();
            let source = st.sources.iter().find(|s| s.name == marketplace).ok_or_else(|| MarketError::NotFound(marketplace.into()))?;
            let listed = source.catalog.module(id).cloned().ok_or_else(|| MarketError::NotFound(key.into()))?;
            let current = st.installed.iter().find(|m| m.key() == key).cloned();
            (source.location.clone(), listed, source.trust, current)
        };
        if let Some(why) = listed.unsupported() {
            return Err(MarketError::Invalid(format!("{key} {}: {why}", listed.version)));
        }
        let location = Location::parse(&location)?;
        let root = match &location {
            // Use the checkout the catalog came from.
            Location::Git(url) if source::git_dir(&self.git_cache(), url).join(".git").is_dir() => {
                Root::Dir(source::git_dir(&self.git_cache(), url))
            }
            other => source::materialize(other, &self.git_cache()).await?.0,
        };
        let path = |file: &str| if listed.source == "." { file.to_string() } else { format!("{}/{file}", listed.source) };

        let manifest_bytes = root.read(&self.http, &path("manifest.json"), source::MAX_MANIFEST).await?;
        let manifest = Manifest::parse(&String::from_utf8_lossy(&manifest_bytes)).map_err(MarketError::Invalid)?;
        if manifest.id != id {
            return Err(MarketError::Invalid(format!("{key}: its manifest.json says id {:?}", manifest.id)));
        }
        if listed.api_version.is_some_and(|level| level != manifest.api_version) {
            return Err(MarketError::Invalid(format!(
                "{key}: the catalog says apiVersion {:?} but its manifest.json says {}",
                listed.api_version, manifest.api_version
            )));
        }
        if !catalog::is_relative(&manifest.entry) {
            return Err(MarketError::Invalid(format!("{key}: entry {:?} must stay inside the module", manifest.entry)));
        }
        let entry = root.read(&self.http, &path(&manifest.entry), source::MAX_ENTRY).await?;
        match &listed.sha256 {
            Some(expected) if source::sha256_hex(&entry) != *expected => return Err(MarketError::Integrity(key.into())),
            None if !location.is_local() => {
                return Err(MarketError::Invalid(format!("{key}: the catalog has no sha256; remote marketplaces must pin each module")))
            }
            _ => {}
        }
        let files = match (&location, &root) {
            (Location::Dir(_), Root::Dir(dir)) => Files::InPlace(if listed.source == "." { dir.clone() } else { dir.join(&listed.source) }),
            _ => Files::Copy { manifest: manifest_bytes, entry },
        };

        let permissions = PermissionSummary::of(&manifest.permissions);
        let granted = current.as_ref().map(|m| PermissionSummary::of(&m.granted)).unwrap_or_default();
        let added = permissions.beyond(&granted);
        let summary = PlanSummary {
            key: key.into(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            current_version: current.as_ref().map(|m| m.manifest.version.clone()),
            trust,
            needs_consent: current.is_none() || !added.is_empty(),
            permissions,
            added,
        };
        Ok(Plan { summary, id: id.into(), marketplace: marketplace.into(), manifest, files })
    }

    /// Download again, keeping the user's settings and accounts, the modules whose files are
    /// gone or that still run in place from a folder their marketplace no longer is: those that
    /// ran from the app's own copy of the official marketplace before it moved to GitHub (an
    /// update may or may not delete that copy). One asking for more than the user consented to
    /// waits for them. Returns the modules not brought back.
    pub async fn restore_missing(&self) -> Vec<String> {
        let stale: Vec<String> = {
            let st = self.state.lock().unwrap();
            let moved = |m: &InstalledRecord| {
                m.local
                    && st.sources.iter().find(|s| s.name == m.marketplace).is_some_and(|s| Location::parse(&s.location).is_ok_and(|l| !l.is_local()))
            };
            st.installed.iter().filter(|m| !m.has_files() || moved(m)).map(|m| m.key()).collect()
        };
        let mut left = Vec::new();
        for key in stale {
            let restored = match self.plan_install(&key).await {
                Ok(plan) if !plan.summary.needs_consent => self.install(plan).map(drop),
                Ok(_) => Err(MarketError::Invalid(format!("{key} asks for new permissions"))),
                Err(e) => Err(e),
            };
            if let Err(e) = restored {
                log::warn!("cannot download {key} again: {e}");
                left.push(key);
            }
        }
        left
    }

    /// Install (or update to) a planned module: the user consented to its permissions.
    pub fn install(&self, plan: Plan) -> Result<InstalledInfo, MarketError> {
        let Plan { id, marketplace, manifest, files, summary } = plan;
        let (dir, local) = match files {
            Files::InPlace(dir) => (dir, true),
            Files::Copy { manifest: manifest_bytes, entry } => {
                let dir = self.dir.join("modules").join(&marketplace).join(&id);
                replace_dir(&dir, &[("manifest.json", &manifest_bytes), (&manifest.entry, &entry)]).map_err(io)?;
                (dir, false)
            }
        };
        self.change(|st| {
            if !st.sources.iter().any(|s| s.name == marketplace) {
                return Err(MarketError::NotFound(marketplace.clone()));
            }
            let record = InstalledRecord {
                id: id.clone(),
                marketplace: marketplace.clone(),
                enabled: true,
                auto_start: None,
                granted: manifest.permissions.clone(),
                manifest,
                dir,
                local,
                update: None,
                update_needs_newer_app: None,
                installed_at: Utc::now(),
            };
            match st.installed.iter_mut().find(|m| m.key() == summary.key) {
                Some(existing) => {
                    *existing = InstalledRecord {
                        enabled: existing.enabled,
                        auto_start: existing.auto_start,
                        installed_at: existing.installed_at,
                        ..record
                    };
                }
                None => st.installed.push(record),
            }
            mark_updates(st);
            Ok(())
        })?;
        self.info(&summary.key)
    }

    /// Uninstall: files (unless local) and stored credentials go too.
    pub fn remove(&self, key: &str) -> Result<(), MarketError> {
        let gone = self.change(|st| {
            let at = st.installed.iter().position(|m| m.key() == key).ok_or_else(|| MarketError::NotFound(key.into()))?;
            Ok(st.installed.remove(at))
        })?;
        self.forget(&gone);
        Ok(())
    }

    fn forget(&self, module: &InstalledRecord) {
        for field in &module.granted.credentials {
            if let Err(e) = self.secrets.delete(&account(&module.key(), &field.key)) {
                log::warn!("cannot delete a credential of {}: {e}", module.key());
            }
        }
        if !module.local {
            let _ = std::fs::remove_dir_all(&module.dir);
        }
    }

    pub fn set_enabled(&self, key: &str, enabled: bool) -> Result<(), MarketError> {
        self.change(|st| {
            find(st, key)?.enabled = enabled;
            Ok(())
        })
    }

    /// Whether copied links this module resolves start at once.
    pub fn set_auto_start(&self, key: &str, auto_start: bool) -> Result<(), MarketError> {
        self.change(|st| {
            find(st, key)?.auto_start = Some(auto_start);
            Ok(())
        })
    }

    /// Move a module to position `index` of the priority order.
    pub fn move_to(&self, key: &str, index: usize) -> Result<(), MarketError> {
        self.change(|st| {
            let at = st.installed.iter().position(|m| m.key() == key).ok_or_else(|| MarketError::NotFound(key.into()))?;
            let module = st.installed.remove(at);
            let index = index.min(st.installed.len());
            st.installed.insert(index, module);
            Ok(())
        })
    }

    /// Store (`Some`) or delete (`None`) credential values. Only consented keys are accepted.
    pub fn set_credentials(&self, key: &str, values: BTreeMap<String, Option<String>>) -> Result<InstalledInfo, MarketError> {
        let granted = {
            let st = self.state.lock().unwrap();
            st.installed.iter().find(|m| m.key() == key).map(InstalledRecord::effective).ok_or_else(|| MarketError::NotFound(key.into()))?
        };
        for name in values.keys() {
            if !granted.credentials.iter().any(|c| &c.key == name) {
                return Err(MarketError::Invalid(format!("{key} has no credential {name:?}")));
            }
        }
        for (name, value) in values {
            let account = account(key, &name);
            match value.filter(|v| !v.is_empty()) {
                Some(v) => self.secrets.set(&account, &Secret(v)),
                None => self.secrets.delete(&account),
            }
            .map_err(MarketError::Secrets)?;
        }
        self.info(key)
    }

    /// Re-read the manifests of local modules (they are developed in place). Returns whether
    /// any changed. New permissions they ask for stay off until the user consents again.
    pub fn reload_local(&self) -> Result<bool, MarketError> {
        let fresh: Vec<(String, Manifest)> = {
            let st = self.state.lock().unwrap();
            st.installed
                .iter()
                .filter(|m| m.local)
                .filter_map(|m| {
                    let json = std::fs::read_to_string(m.dir.join("manifest.json")).ok()?;
                    match Manifest::parse(&json) {
                        Ok(manifest) if manifest.id == m.id && manifest != m.manifest => Some((m.key(), manifest)),
                        Ok(_) => None,
                        Err(e) => {
                            log::warn!("{}: {e}", m.key());
                            None
                        }
                    }
                })
                .collect()
        };
        if fresh.is_empty() {
            return Ok(false);
        }
        self.change(|st| {
            for (key, manifest) in fresh {
                if let Ok(module) = find(st, &key) {
                    module.manifest = manifest;
                }
            }
            Ok(true)
        })
    }

    // --- reading ---------------------------------------------------------------------------

    pub fn installed(&self) -> Vec<InstalledInfo> {
        let st = self.state.lock().unwrap();
        st.installed.iter().map(|m| self.describe(&st, m)).collect()
    }

    fn info(&self, key: &str) -> Result<InstalledInfo, MarketError> {
        let st = self.state.lock().unwrap();
        let module = st.installed.iter().find(|m| m.key() == key).ok_or_else(|| MarketError::NotFound(key.into()))?;
        Ok(self.describe(&st, module))
    }

    fn describe(&self, st: &State, m: &InstalledRecord) -> InstalledInfo {
        let key = m.key();
        let requested = PermissionSummary::of(&m.manifest.permissions);
        let permissions = m.effective();
        let credentials = permissions
            .credentials
            .iter()
            .map(|c| CredentialStatus {
                spec: c.clone(),
                set: matches!(self.secrets.get(&account(&key, &c.key)), Ok(Some(_))),
            })
            .collect();
        InstalledInfo {
            id: m.id.clone(),
            marketplace: m.marketplace.clone(),
            name: m.manifest.name.clone(),
            version: m.manifest.version.clone(),
            description: m.manifest.description.clone(),
            homepage: m.manifest.homepage.clone(),
            enabled: m.enabled,
            auto_start: m.auto_start.unwrap_or(m.manifest.auto_start),
            trust: st.sources.iter().find(|s| s.name == m.marketplace).map_or(Trust::Community, |s| s.trust),
            local: m.local,
            url_patterns: m.manifest.url_patterns.clone(),
            needs_consent: !requested.beyond(&PermissionSummary::of(&m.granted)).is_empty(),
            update: m.update.clone(),
            update_needs_newer_app: m.update_needs_newer_app.clone(),
            missing: !m.has_files(),
            credentials,
            credentials_help: m.manifest.credentials_help.clone(),
            credentials_help_url: m.manifest.credentials_help_url.clone(),
            permissions,
            key,
        }
    }

    /// Folders of local (in-development) modules: their manifests are read from there, so the
    /// app watches them and calls `reload_local` when they change.
    pub fn local_dirs(&self) -> Vec<PathBuf> {
        self.state.lock().unwrap().installed.iter().filter(|m| m.local).map(|m| m.dir.clone()).collect()
    }

    /// The enabled modules in priority order, with the permissions and credentials the user
    /// granted, for `ModuleResolver::set_modules`.
    pub fn resolver_modules(&self) -> Vec<InstalledModule> {
        let st = self.state.lock().unwrap();
        st.installed
            .iter()
            .filter(|m| m.enabled && m.has_files())
            .filter_map(|m| {
                let key = m.key();
                let allowed = m.effective();
                let credentials = allowed
                    .credentials
                    .iter()
                    .filter_map(|c| match self.secrets.get(&account(&key, &c.key)) {
                        Ok(value) => value.map(|v| (c.key.clone(), v)),
                        Err(e) => {
                            log::warn!("cannot read a credential of {key}: {e}");
                            None
                        }
                    })
                    .collect();
                // Checked when the manifest was read; a failure here must not widen the match.
                let url_regex = match m.manifest.url_regex.as_deref().map(url_regex).transpose() {
                    Ok(regex) => regex,
                    Err(e) => {
                        log::warn!("{key}: urlRegex: {e}");
                        return None;
                    }
                };
                Some(InstalledModule {
                    module: ModuleRef {
                        id: key,
                        entry: m.dir.join(&m.manifest.entry),
                        grants: Grants { allow_domains: allowed.allow_domains, credentials },
                        api_version: m.manifest.api_version,
                    },
                    url_patterns: m.manifest.url_patterns.clone(),
                    url_regex,
                    limits: Limits::default(),
                })
            })
            .collect()
    }
}

fn source_info(s: &SourceRecord) -> SourceInfo {
    SourceInfo {
        name: s.name.clone(),
        location: s.location.clone(),
        trust: s.trust,
        owner: s.catalog.owner.clone(),
        modules: s.catalog.modules.iter().map(|m| CatalogModule { needs_newer_app: m.unsupported().is_some(), ..m.clone() }).collect(),
        refreshed_at: s.refreshed_at,
        commit: s.commit.clone(),
        error: s.error.clone(),
    }
}

/// Note which installed modules have a newer version in their catalog.
fn mark_updates(st: &mut State) {
    for module in &mut st.installed {
        let listed = st.sources.iter().find(|s| s.name == module.marketplace).and_then(|s| s.catalog.module(&module.id));
        let newer = listed.filter(|l| catalog::is_newer(&l.version, &module.manifest.version));
        module.update = newer.filter(|l| l.unsupported().is_none()).map(|l| l.version.clone());
        module.update_needs_newer_app = newer.filter(|l| l.unsupported().is_some()).map(|l| l.version.clone());
    }
}

fn find<'a>(st: &'a mut State, key: &str) -> Result<&'a mut InstalledRecord, MarketError> {
    st.installed.iter_mut().find(|m| m.key() == key).ok_or_else(|| MarketError::NotFound(key.into()))
}

fn split_key(key: &str) -> Result<(&str, &str), MarketError> {
    key.split_once('@').filter(|(id, mp)| !id.is_empty() && !mp.is_empty()).ok_or_else(|| MarketError::Invalid(format!("{key:?}: expected id@marketplace")))
}

fn account(key: &str, field: &str) -> String {
    format!("{key}/{field}")
}

/// Replace `dir` with exactly `files` (relative path, contents). Written beside it first, so a
/// failure leaves the old version in place.
fn replace_dir(dir: &Path, files: &[(&str, &[u8])]) -> std::io::Result<()> {
    let staging = dir.with_extension("new");
    let _ = std::fs::remove_dir_all(&staging);
    for (path, bytes) in files {
        let file = staging.join(path);
        std::fs::create_dir_all(file.parent().expect("inside staging"))?;
        std::fs::write(file, bytes)?;
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::rename(&staging, dir)
}
