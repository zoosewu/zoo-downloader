//! Marketplaces through `Marketplace`'s interface only: an HTTP fixture site, git and local folders.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use zoo_marketplace::{MarketError, Marketplace, MemorySecrets, SecretStore, Trust};

/// Serves `files` by path; answers 404 for anything else.
struct Site {
    addr: std::net::SocketAddr,
    files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Site {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Default::default();
        let served = files.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else { return };
                let files = served.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let body = files.lock().unwrap().get(&path).cloned();
                    let (status, body) = match body {
                        Some(b) => ("200 OK", b),
                        None => ("404 Not Found", Vec::new()),
                    };
                    let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self { addr, files, task }
    }

    fn catalog_url(&self) -> String {
        format!("http://{}/m/zoo-marketplace.json", self.addr)
    }

    fn put(&self, path: &str, body: impl Into<Vec<u8>>) {
        self.files.lock().unwrap().insert(path.into(), body.into());
    }

    /// Publish `module` as the only module of marketplace "zoo".
    fn publish(&self, module: &Module) {
        self.put("/m/demo/manifest.json", module.manifest());
        self.put("/m/demo/dist/index.js", module.code.clone());
        self.put("/m/zoo-marketplace.json", catalog("zoo", &[module], true));
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Clone)]
struct Module {
    id: &'static str,
    version: &'static str,
    domains: Vec<&'static str>,
    credentials: Vec<&'static str>,
    /// Extra manifest fields.
    extra: serde_json::Value,
    code: String,
}

impl Module {
    fn demo(version: &'static str, domains: &[&'static str]) -> Self {
        Self {
            id: "demo",
            version,
            domains: domains.to_vec(),
            credentials: vec![],
            extra: json!({}),
            code: format!("export default {{ async resolve(url) {{ return {{ url, filename: 'v{version}' }}; }} }};"),
        }
    }

    fn manifest(&self) -> String {
        let credentials: Vec<_> = self.credentials.iter().map(|k| json!({ "key": k, "label": k })).collect();
        let mut manifest = json!({
            "apiVersion": 1,
            "id": self.id,
            "name": "Demo host",
            "version": self.version,
            "urlPatterns": ["https://demo.example/f/*"],
            "permissions": { "network": { "allowDomains": self.domains }, "credentials": credentials },
        });
        manifest.as_object_mut().unwrap().extend(self.extra.as_object().unwrap().clone());
        manifest.to_string()
    }
}

fn catalog(name: &str, modules: &[&Module], pinned: bool) -> String {
    let modules: Vec<_> = modules
        .iter()
        .map(|m| {
            let mut entry = json!({ "id": m.id, "version": m.version, "source": m.id, "description": "test" });
            if pinned {
                entry["sha256"] = json!(sha256(m.code.as_bytes()));
            }
            entry
        })
        .collect();
    json!({ "name": name, "owner": "tests", "modules": modules }).to_string()
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn open(dir: &Path, secrets: &Arc<MemorySecrets>) -> Marketplace {
    Marketplace::open(dir, secrets.clone()).unwrap()
}

fn entry_code(market: &Marketplace) -> String {
    let modules = market.resolver_modules();
    std::fs::read_to_string(&modules[0].module.entry).unwrap()
}

#[tokio::test]
async fn install_needs_consent_and_runs_only_granted_permissions() {
    let site = Site::start().await;
    let v1 = Module::demo("1.0.0", &["demo.example"]);
    site.publish(&v1);
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);

    let source = market.add_source(&site.catalog_url()).await.unwrap();
    assert_eq!((source.name.as_str(), source.trust, source.modules.len()), ("zoo", Trust::Community, 1));
    assert!(market.resolver_modules().is_empty(), "nothing is installed by adding a marketplace");

    let plan = market.plan_install("demo@zoo").await.unwrap();
    assert!(plan.summary.needs_consent);
    assert_eq!(plan.summary.added.allow_domains, vec!["demo.example"]);
    assert!(market.resolver_modules().is_empty(), "planning installs nothing");

    let info = market.install(plan).unwrap();
    assert_eq!((info.key.as_str(), info.version.as_str(), info.enabled, info.local), ("demo@zoo", "1.0.0", true, false));
    let modules = market.resolver_modules();
    assert_eq!(modules[0].module.id, "demo@zoo");
    assert_eq!(modules[0].module.grants.allow_domains, vec!["demo.example"]);
    assert_eq!(modules[0].url_patterns, vec!["https://demo.example/f/*"]);
    assert_eq!(entry_code(&market), v1.code);
    assert!(modules[0].module.entry.starts_with(data.path()), "remote modules are copied into the app's data");
}

#[tokio::test]
async fn a_module_that_is_not_what_the_catalog_pinned_is_refused() {
    let site = Site::start().await;
    let module = Module::demo("1.0.0", &["demo.example"]);
    site.publish(&module);
    site.put("/m/demo/dist/index.js", "export default { async resolve() { steal(); } };");
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);
    market.add_source(&site.catalog_url()).await.unwrap();

    assert!(matches!(market.plan_install("demo@zoo").await, Err(MarketError::Integrity(_))));

    site.put("/m/zoo-marketplace.json", catalog("zoo", &[&module], false));
    market.refresh().await;
    let unpinned = market.plan_install("demo@zoo").await;
    assert!(matches!(&unpinned, Err(MarketError::Invalid(m)) if m.contains("sha256")), "{:?}", unpinned.err());
    assert!(matches!(market.plan_install("other@zoo").await, Err(MarketError::NotFound(_))));
}

#[tokio::test]
async fn updates_asking_for_more_wait_for_consent() {
    let site = Site::start().await;
    site.publish(&Module::demo("1.0.0", &["demo.example"]));
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);
    market.add_source(&site.catalog_url()).await.unwrap();
    market.install(market.plan_install("demo@zoo").await.unwrap()).unwrap();

    // Same permissions: an update the user does not need to approve again.
    site.publish(&Module::demo("1.0.1", &["demo.example"]));
    market.refresh().await;
    assert_eq!(market.installed()[0].update.as_deref(), Some("1.0.1"));
    let quiet = market.plan_install("demo@zoo").await.unwrap();
    assert!(!quiet.summary.needs_consent && quiet.summary.added.is_empty());
    assert_eq!(quiet.summary.current_version.as_deref(), Some("1.0.0"));
    market.install(quiet).unwrap();
    assert_eq!(market.installed()[0].update, None);

    // More domains: consent again; until then the installed version keeps running.
    let v2 = Module::demo("2.0.0", &["demo.example", "cdn.example"]);
    site.publish(&v2);
    market.refresh().await;
    let plan = market.plan_install("demo@zoo").await.unwrap();
    assert!(plan.summary.needs_consent);
    assert_eq!(plan.summary.added.allow_domains, vec!["cdn.example"]);
    assert!(entry_code(&market).contains("v1.0.1"));
    market.install(plan).unwrap();
    assert_eq!(entry_code(&market), v2.code);
    assert_eq!(market.resolver_modules()[0].module.grants.allow_domains, vec!["demo.example", "cdn.example"]);
}

#[tokio::test]
async fn installed_modules_work_offline_and_survive_a_restart() {
    let site = Site::start().await;
    site.publish(&Module::demo("1.0.0", &["demo.example"]));
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    {
        let market = open(data.path(), &secrets);
        market.add_source(&site.catalog_url()).await.unwrap();
        market.install(market.plan_install("demo@zoo").await.unwrap()).unwrap();
    }
    drop(site);

    let market = open(data.path(), &secrets);
    assert!(entry_code(&market).contains("v1.0.0"));
    let sources = market.refresh().await;
    assert!(sources[0].error.is_some(), "the refresh failure is reported");
    assert_eq!(sources[0].modules.len(), 1, "the last catalog is kept");
    assert_eq!(market.resolver_modules().len(), 1);
}

#[tokio::test]
async fn credentials_order_enabling_and_removal() {
    let site = Site::start().await;
    let mut module = Module::demo("1.0.0", &["demo.example"]);
    module.credentials = vec!["password"];
    site.publish(&module);
    let other = Module { id: "other", ..Module::demo("1.0.0", &["other.example"]) };
    site.put("/m/other/manifest.json", other.manifest());
    site.put("/m/other/dist/index.js", other.code.clone());
    site.put("/m/zoo-marketplace.json", catalog("zoo", &[&module, &other], true));
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);
    market.add_source(&site.catalog_url()).await.unwrap();
    for key in ["demo@zoo", "other@zoo"] {
        market.install(market.plan_install(key).await.unwrap()).unwrap();
    }

    let info = market.set_credentials("demo@zoo", BTreeMap::from([("password".into(), Some("s3cret".into()))])).unwrap();
    assert!(info.credentials[0].set);
    assert!(!format!("{:?}", market.installed()).contains("s3cret"), "values are never shown");
    let grants = &market.resolver_modules()[0].module.grants;
    assert_eq!(grants.credentials.get("password").map(|s| s.0.as_str()), Some("s3cret"));
    let undeclared = market.set_credentials("demo@zoo", BTreeMap::from([("token".into(), Some("x".into()))]));
    assert!(matches!(undeclared, Err(MarketError::Invalid(_))));

    let order = |m: &Marketplace| m.resolver_modules().iter().map(|r| r.module.id.clone()).collect::<Vec<_>>();
    assert_eq!(order(&market), ["demo@zoo", "other@zoo"], "first installed comes first");
    market.move_to("other@zoo", 0).unwrap();
    assert_eq!(order(&market), ["other@zoo", "demo@zoo"]);
    market.set_enabled("other@zoo", false).unwrap();
    assert_eq!(order(&market), ["demo@zoo"]);
    assert_eq!(market.installed().len(), 2, "disabled modules stay installed");

    market.remove("demo@zoo").unwrap();
    assert_eq!(secrets.get("demo@zoo/password").unwrap(), None, "credentials go with the module");
    assert!(!data.path().join("modules/zoo/demo").exists());
    market.remove_source("zoo").unwrap();
    assert!(market.installed().is_empty() && market.sources().is_empty());
}

#[tokio::test]
async fn the_module_proposes_auto_start_and_the_user_decides() {
    let site = Site::start().await;
    let module = Module { extra: json!({ "autoStart": true, "urlRegex": "^https://demo\\.example/f/[0-9]+$" }), ..Module::demo("1.0.0", &[]) };
    site.publish(&module);
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);
    market.add_source(&site.catalog_url()).await.unwrap();
    market.install(market.plan_install("demo@zoo").await.unwrap()).unwrap();
    assert!(market.installed()[0].auto_start, "the manifest's choice");

    let resolver = &market.resolver_modules()[0];
    assert!(resolver.matches("https://demo.example/f/123"));
    assert!(resolver.matches("https://demo.example/f/123#top"), "the fragment is ignored");
    assert!(!resolver.matches("https://demo.example/f/abc"), "urlRegex narrows urlPatterns");
    assert!(!resolver.matches("https://other.example/f/123"));

    market.set_auto_start("demo@zoo", false).unwrap();
    site.publish(&Module { version: "1.0.1", ..module });
    market.refresh().await;
    market.install(market.plan_install("demo@zoo").await.unwrap()).unwrap();
    assert!(!market.installed()[0].auto_start, "the user's choice survives an update");
    assert!(!open(data.path(), &secrets).installed()[0].auto_start, "and a restart");
}

#[tokio::test]
async fn a_local_folder_is_used_in_place_and_reloaded() {
    let (dev, data, secrets) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let module = Module::demo("0.1.0", &["demo.example"]);
    std::fs::write(dev.path().join("manifest.json"), module.manifest()).unwrap();
    std::fs::create_dir_all(dev.path().join("dist")).unwrap();
    std::fs::write(dev.path().join("dist/index.js"), &module.code).unwrap();
    let market = open(data.path(), &secrets);

    let source = market.add_source(&dev.path().to_string_lossy()).await.unwrap();
    assert_eq!(source.name, "dev-demo");
    let info = market.install(market.plan_install("demo@dev-demo").await.unwrap()).unwrap();
    assert!(info.local);
    assert_eq!(market.resolver_modules()[0].module.entry, dev.path().join("dist/index.js"));
    assert!(!market.reload_local().unwrap(), "nothing changed yet");

    let grown = Module { domains: vec!["demo.example", "cdn.example"], ..module.clone() };
    std::fs::write(dev.path().join("manifest.json"), grown.manifest().replace("demo.example/f/*", "demo.example/g/*")).unwrap();
    assert!(market.reload_local().unwrap());
    let info = &market.installed()[0];
    assert_eq!(info.url_patterns, vec!["https://demo.example/g/*"], "patterns follow the folder");
    assert!(info.needs_consent);
    assert_eq!(market.resolver_modules()[0].module.grants.allow_domains, vec!["demo.example"], "new permissions wait for consent");

    let plan = market.plan_install("demo@dev-demo").await.unwrap();
    assert_eq!(plan.summary.added.allow_domains, vec!["cdn.example"]);
    market.install(plan).unwrap();
    assert!(!market.installed()[0].needs_consent);
}

#[tokio::test]
async fn names_collide_and_the_official_marketplace_stays() {
    let site = Site::start().await;
    site.publish(&Module::demo("1.0.0", &["demo.example"]));
    let official = tempfile::tempdir().unwrap();
    std::fs::write(official.path().join("zoo-marketplace.json"), json!({ "name": "zoo", "modules": [] }).to_string()).unwrap();
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);

    assert_eq!(market.set_official(&official.path().to_string_lossy()).await.unwrap().trust, Trust::Official);
    assert!(matches!(market.add_source(&site.catalog_url()).await, Err(MarketError::Invalid(m)) if m.contains("already added")));
    assert!(matches!(market.remove_source("zoo"), Err(MarketError::Invalid(_))));
    assert!(market.set_official(&official.path().to_string_lossy()).await.is_ok(), "set again on every start");
    assert!(matches!(market.add_source("not a source").await, Err(MarketError::Invalid(_))));
}

/// Run git in `dir`; `false` when git is not installed.
fn git(dir: &Path, args: &[&str]) -> bool {
    match std::process::Command::new("git").arg("-C").arg(dir).args(args).output() {
        Ok(out) => {
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
            true
        }
        Err(_) => false,
    }
}

fn commit_module(repo: &Path, module: &Module) {
    std::fs::create_dir_all(repo.join("demo/dist")).unwrap();
    std::fs::write(repo.join("demo/manifest.json"), module.manifest()).unwrap();
    std::fs::write(repo.join("demo/dist/index.js"), &module.code).unwrap();
    std::fs::write(repo.join("zoo-marketplace.json"), catalog("gitzoo", &[module], true)).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["-c", "user.name=t", "-c", "user.email=t@example.invalid", "commit", "-q", "-m", module.version]);
}

#[tokio::test]
async fn a_git_marketplace_is_cloned_and_pulled() {
    let repo = tempfile::tempdir().unwrap();
    if !git(repo.path(), &["init", "-q"]) {
        eprintln!("git not installed; skipping");
        return;
    }
    commit_module(repo.path(), &Module::demo("1.0.0", &["demo.example"]));
    let url = url::Url::from_directory_path(repo.path()).unwrap().to_string();
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);

    let source = market.add_source(&url).await.unwrap();
    assert_eq!(source.name, "gitzoo");
    assert!(source.commit.is_some());
    let info = market.install(market.plan_install("demo@gitzoo").await.unwrap()).unwrap();
    assert!(!info.local, "git modules are copied");

    commit_module(repo.path(), &Module::demo("1.1.0", &["demo.example"]));
    let sources = market.refresh().await;
    assert_eq!(sources[0].error, None);
    assert_ne!(sources[0].commit, source.commit);
    assert_eq!(market.installed()[0].update.as_deref(), Some("1.1.0"));
    market.install(market.plan_install("demo@gitzoo").await.unwrap()).unwrap();
    assert!(entry_code(&market).contains("v1.1.0"));
}

#[tokio::test]
async fn a_version_for_a_newer_app_is_shown_but_not_offered() {
    let site = Site::start().await;
    site.publish(&Module::demo("1.0.0", &["demo.example"]));
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);
    market.add_source(&site.catalog_url()).await.unwrap();
    market.install(market.plan_install("demo@zoo").await.unwrap()).unwrap();

    // 2.0.0 is written against an API level this app does not run, with a field it does not know.
    let mut v2 = Module::demo("2.0.0", &["demo.example"]);
    v2.extra = json!({ "apiVersion": 99, "featureOfLevel99": true });
    site.publish(&v2);
    let mut listed: serde_json::Value = serde_json::from_str(&catalog("zoo", &[&v2], true)).unwrap();
    listed["modules"][0]["apiVersion"] = json!(99);
    listed["releases"] = json!("a field of a newer catalog format");
    site.put("/m/zoo-marketplace.json", listed.to_string());
    let sources = market.refresh().await;

    assert!(sources[0].error.is_none(), "a newer catalog still lists: {:?}", sources[0].error);
    assert!(sources[0].modules[0].needs_newer_app);
    let installed = &market.installed()[0];
    assert_eq!((installed.update.as_deref(), installed.update_needs_newer_app.as_deref()), (None, Some("2.0.0")));
    let refused = market.plan_install("demo@zoo").await.err().expect("refused").to_string();
    assert!(refused.contains("needs a newer Zoo Downloader"), "{refused}");
    assert!(entry_code(&market).contains("v1.0.0"), "the installed version keeps running");
}

/// Write `modules` as a marketplace folder (what older apps shipped as the official one).
fn folder_marketplace(dir: &Path, name: &str, modules: &[&Module]) {
    for m in modules {
        std::fs::create_dir_all(dir.join(m.id).join("dist")).unwrap();
        std::fs::write(dir.join(m.id).join("manifest.json"), m.manifest()).unwrap();
        std::fs::write(dir.join(m.id).join("dist/index.js"), &m.code).unwrap();
    }
    std::fs::write(dir.join("zoo-marketplace.json"), catalog(name, modules, false)).unwrap();
}

#[tokio::test]
async fn modules_the_app_no_longer_ships_are_downloaded_again() {
    // An older app: the official marketplace is a folder in its resources, run in place.
    let shipped = tempfile::tempdir().unwrap();
    let mut demo = Module::demo("1.0.0", &["demo.example"]);
    demo.credentials = vec!["password"];
    let wider = Module { id: "wider", ..Module::demo("1.0.0", &["wider.example"]) };
    folder_marketplace(shipped.path(), "zoo", &[&demo, &wider]);
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);
    market.set_official(&shipped.path().to_string_lossy()).await.unwrap();
    for key in ["demo@zoo", "wider@zoo"] {
        market.install(market.plan_install(key).await.unwrap()).unwrap();
    }
    market.set_credentials("demo@zoo", BTreeMap::from([("password".into(), Some("s3cret".into()))])).unwrap();
    market.set_enabled("demo@zoo", false).unwrap();
    market.set_enabled("demo@zoo", true).unwrap();
    market.set_auto_start("demo@zoo", true).unwrap();

    // The official marketplace is now published remotely, where "wider" asks for another domain.
    // The update may leave the old copy (demo still runs from it) or delete it (wider).
    std::fs::remove_dir_all(shipped.path().join("wider")).unwrap();
    let site = Site::start().await;
    let wider_now = Module { id: "wider", ..Module::demo("1.0.0", &["wider.example", "cdn.example"]) };
    for m in [&demo, &wider_now] {
        site.put(&format!("/m/{}/manifest.json", m.id), m.manifest());
        site.put(&format!("/m/{}/dist/index.js", m.id), m.code.clone());
    }
    site.put("/m/zoo-marketplace.json", catalog("zoo", &[&demo, &wider_now], true));
    let market = open(data.path(), &secrets);
    let missing = |m: &Marketplace| m.installed().into_iter().filter(|i| i.missing).map(|i| i.key).collect::<Vec<_>>();
    assert_eq!(missing(&market), ["wider@zoo"]);
    let running = |m: &Marketplace| m.resolver_modules().iter().map(|r| r.module.id.clone()).collect::<Vec<_>>();
    assert_eq!(running(&market), ["demo@zoo"], "a module without its files does not run");
    market.set_official(&site.catalog_url()).await.unwrap();

    assert_eq!(market.restore_missing().await, ["wider@zoo"], "new permissions wait for the user");
    let demo_now = market.installed().into_iter().find(|m| m.key == "demo@zoo").unwrap();
    assert!(!demo_now.missing && !demo_now.local && demo_now.auto_start && demo_now.credentials[0].set);
    assert_eq!(running(&market), ["demo@zoo"]);
    assert_eq!(missing(&market), ["wider@zoo"]);
    let plan = market.plan_install("wider@zoo").await.unwrap();
    assert!(plan.summary.needs_consent);
    market.install(plan).unwrap();
    assert!(market.restore_missing().await.is_empty());
}

/// A module in use looks for its updates, at most once per `max_age`.
#[tokio::test]
async fn a_module_in_use_finds_its_update() {
    let site = Site::start().await;
    site.publish(&Module::demo("1.0.0", &["demo.example"]));
    let (data, secrets) = (tempfile::tempdir().unwrap(), Arc::new(MemorySecrets::default()));
    let market = open(data.path(), &secrets);
    market.add_source(&site.catalog_url()).await.unwrap();
    market.install(market.plan_install("demo@zoo").await.unwrap()).unwrap();

    site.publish(&Module::demo("1.1.0", &["demo.example"]));
    let hour = std::time::Duration::from_secs(3600);
    assert!(!market.refresh_for("demo@zoo", hour).await, "fetched a moment ago");
    assert_eq!(market.installed()[0].update, None);
    assert!(market.refresh_for("demo@zoo", std::time::Duration::ZERO).await);
    assert_eq!(market.installed()[0].update.as_deref(), Some("1.1.0"), "shown, not installed");
    assert_eq!(market.installed()[0].version, "1.0.0");
    assert!(!market.refresh_for("nobody@nowhere", std::time::Duration::ZERO).await);
}
