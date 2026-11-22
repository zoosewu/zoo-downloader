//! The host through the app's interface: a real `zoo-module serve` process behind a Supervisor.

mod common;

use std::time::{Duration, Instant};

use common::{host, limits, module};
use tempfile::TempDir;
use zoo_module_client::{Limits, Listed, ModuleError, ModuleRef, NoInteraction, Secret, Supervisor};

const OK: &str = r#"
import { defineModule } from 'zoo-module';
export default defineModule({
  async resolve(url, zoo) {
    return { url: url + '/direct', filename: 'f.zip', headers: { Referer: url }, size: 42 };
  },
});
"#;

#[tokio::test]
async fn resolves_with_module_result() {
    let dir = TempDir::new().unwrap();
    let resolved = host().resolve(module(&dir, "ok", OK), "https://site/f", Limits::default(), &NoInteraction).await.unwrap();
    assert_eq!(resolved.url, "https://site/f/direct");
    assert_eq!(resolved.filename.as_deref(), Some("f.zip"));
    assert_eq!(resolved.headers.get("Referer").map(String::as_str), Some("https://site/f"));
    assert_eq!(resolved.size, Some(42));
}

#[tokio::test]
async fn fail_carries_message_and_hint() {
    let dir = TempDir::new().unwrap();
    let m = module(
        &dir,
        "fail",
        "export default { async resolve(url, zoo) { throw zoo.fail('no link', { hint: 'page changed' }); } };",
    );
    match host().resolve(m, "https://site/f", Limits::default(), &NoInteraction).await {
        Err(ModuleError::Script { message, hint, stack, .. }) => {
            assert_eq!(message, "no link");
            assert_eq!(hint.as_deref(), Some("page changed"));
            assert!(stack.is_some());
        }
        other => panic!("expected script error, got {other:?}"),
    }
}

#[tokio::test]
async fn busy_loop_times_out_and_host_survives() {
    let dir = TempDir::new().unwrap();
    let host = host();
    let spin = module(&dir, "spin", "export default { resolve() { for (;;) {} } };");
    let started = Instant::now();
    assert_eq!(host.resolve(spin, "https://a/", limits(300), &NoInteraction).await, Err(ModuleError::Timeout { limit_ms: 300 }));
    assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());

    // Top-level code counts too.
    let spin_top = module(&dir, "spin_top", "for (;;) {}\nexport default { resolve() {} };");
    assert_eq!(host.resolve(spin_top, "https://a/", limits(300), &NoInteraction).await, Err(ModuleError::Timeout { limit_ms: 300 }));

    assert!(host.resolve(module(&dir, "ok", OK), "https://a", Limits::default(), &NoInteraction).await.is_ok());
    assert_eq!(host.spawn_count(), 1, "the host must not restart");
}

#[tokio::test]
async fn memory_hog_hits_heap_limit_and_host_survives() {
    let dir = TempDir::new().unwrap();
    let host = host();
    let hog = module(
        &dir,
        "hog",
        "export default { resolve() { const a = []; for (;;) a.push(new Array(1000).fill(Math.random())); } };",
    );
    let limits = Limits { heap_mb: 32, timeout_ms: 20_000 };
    assert_eq!(host.resolve(hog, "https://a/", limits, &NoInteraction).await, Err(ModuleError::OutOfMemory { limit_mb: 32 }));

    assert!(host.resolve(module(&dir, "ok", OK), "https://a", Limits::default(), &NoInteraction).await.is_ok());
    assert_eq!(host.spawn_count(), 1, "the host must not restart");
}

#[tokio::test]
async fn calls_run_in_parallel_and_in_isolation() {
    let dir = TempDir::new().unwrap();
    let host = host();
    let spin = module(&dir, "spin", "export default { resolve() { for (;;) {} } };");
    // Global state written by one call is not visible to the next.
    let leak = module(
        &dir,
        "leak",
        "export default { resolve(url) { const seen = globalThis.seen ?? 'none'; globalThis.seen = url; return { url: 'https://x/' + seen }; } };",
    );

    let started = Instant::now();
    let (spun, first, second) = tokio::join!(
        host.resolve(spin, "https://a/", limits(1_500), &NoInteraction),
        host.resolve(leak.clone(), "https://one/", Limits::default(), &NoInteraction),
        host.resolve(leak, "https://two/", Limits::default(), &NoInteraction),
    );
    assert_eq!(spun, Err(ModuleError::Timeout { limit_ms: 1_500 }));
    assert_eq!(first.unwrap().url, "https://x/none");
    assert_eq!(second.unwrap().url, "https://x/none");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn module_shape_errors_tell_how_to_fix() {
    let dir = TempDir::new().unwrap();
    let host = host();

    let no_default = module(&dir, "no_default", "export const resolve = () => ({ url: 'https://x/' });");
    match host.resolve(no_default, "https://a/", Limits::default(), &NoInteraction).await {
        Err(ModuleError::Load { hint: Some(hint), .. }) => assert!(hint.contains("export default defineModule")),
        other => panic!("expected load error with hint, got {other:?}"),
    }

    let no_resolve = module(&dir, "no_resolve", "export default { };");
    assert!(matches!(
        host.resolve(no_resolve, "https://a/", Limits::default(), &NoInteraction).await,
        Err(ModuleError::Load { hint: Some(_), .. })
    ));

    let imports = module(&dir, "imports", "import x from 'linkedom';\nexport default { resolve() {} };");
    match host.resolve(imports, "https://a/", Limits::default(), &NoInteraction).await {
        Err(ModuleError::Load { message, .. }) => assert!(message.contains("single file"), "{message}"),
        other => panic!("expected load error, got {other:?}"),
    }

    let syntax = module(&dir, "syntax", "export default {");
    assert!(matches!(host.resolve(syntax, "https://a/", Limits::default(), &NoInteraction).await, Err(ModuleError::Load { .. })));

    let missing = ModuleRef { entry: dir.path().join("nope.js"), ..module(&dir, "missing", "") };
    assert!(matches!(host.resolve(missing, "https://a/", Limits::default(), &NoInteraction).await, Err(ModuleError::Load { .. })));
}

#[tokio::test]
async fn invalid_results_are_rejected() {
    let dir = TempDir::new().unwrap();
    let host = host();
    for (name, result) in [
        ("relative", "{ url: '/file' }"),
        ("scheme", "{ url: 'file:///etc/passwd' }"),
        ("typo", "{ url: 'https://x/', fileName: 'a' }"),
        ("tiny-segments", "{ url: 'https://x/', maxSegmentSize: 1024 }"),
        ("nothing", "undefined"),
    ] {
        let m = module(&dir, name, &format!("export default {{ resolve() {{ return {result}; }} }};"));
        match host.resolve(m, "https://a/", Limits::default(), &NoInteraction).await {
            Err(ModuleError::Script { hint: Some(hint), .. }) => assert!(hint.contains("filename?"), "{name}"),
            other => panic!("{name}: expected script error with hint, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn never_settling_resolve_fails_fast() {
    let dir = TempDir::new().unwrap();
    let m = module(&dir, "stuck", "export default { resolve() { return new Promise(() => {}); } };");
    let started = Instant::now();
    match host().resolve(m, "https://a/", Limits::default(), &NoInteraction).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("never settled"), "{message}"),
        other => panic!("expected script error, got {other:?}"),
    }
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn module_cannot_reach_host_internals_or_stdout() {
    let dir = TempDir::new().unwrap();
    // console.log must not corrupt the protocol on stdout; Deno.core must be gone.
    let m = module(
        &dir,
        "probe",
        r#"export default { resolve() {
            console.log('{"type":"resolved","id":1}');
            return { url: 'https://x/' + typeof Deno + '/' + typeof globalThis.__zooRun };
        } };"#,
    );
    let resolved = host().resolve(m, "https://a/", Limits::default(), &NoInteraction).await.unwrap();
    assert_eq!(resolved.url, "https://x/undefined/function");
}

// ── Supervisor against misbehaving hosts ──

/// A "host" that reads one request and then exits without answering.
fn crashing_host() -> Supervisor {
    if cfg!(windows) {
        Supervisor::new("cmd", ["/c", "set /p x= & exit 1"])
    } else {
        Supervisor::new("sh", ["-c", "read x; exit 1"])
    }
}

/// A "host" that reads one request and never answers.
fn hung_host() -> Supervisor {
    if cfg!(windows) {
        Supervisor::new("cmd", ["/c", "set /p x= & ping -n 60 127.0.0.1 >nul"])
    } else {
        Supervisor::new("sh", ["-c", "read x; sleep 60"])
    }
}

fn dummy() -> ModuleRef {
    ModuleRef { id: "dummy@test".into(), entry: "dummy.js".into(), grants: Default::default(), api_version: 1 }
}

#[tokio::test]
async fn crashed_host_fails_the_call_and_is_restarted() {
    let host = crashing_host();
    for expected_spawns in 1..=2 {
        assert!(matches!(
            host.resolve(dummy(), "https://a/", Limits::default(), &NoInteraction).await,
            Err(ModuleError::Crashed { .. })
        ));
        assert_eq!(host.spawn_count(), expected_spawns);
    }
}

#[tokio::test]
async fn hung_host_is_killed_after_the_time_limit() {
    let host = hung_host();
    let started = Instant::now();
    match host.resolve(dummy(), "https://a/", limits(200), &NoInteraction).await {
        Err(ModuleError::Crashed { message }) => assert!(message.contains("stopped responding"), "{message}"),
        other => panic!("expected crash, got {other:?}"),
    }
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
}

#[tokio::test]
async fn missing_host_program_is_reported() {
    let host = Supervisor::new("definitely-not-a-zoo-module-binary", ["serve"]);
    assert!(matches!(
        host.resolve(dummy(), "https://a/", Limits::default(), &NoInteraction).await,
        Err(ModuleError::Crashed { .. })
    ));
}

#[tokio::test]
async fn dropping_a_call_stops_it_in_the_host() {
    let site = common::Server::start(|_| common::Resp::ok("x")).await;
    let dir = TempDir::new().unwrap();
    let m = common::module_allowing(
        &dir,
        "slow",
        &format!("export default {{ async resolve(url, zoo) {{ await zoo.wait(1); await fetch('{}'); return {{ url }}; }} }};", site.url("/after")),
        &["127.0.0.1"],
    );
    let host = common::loopback_host();
    let call = host.resolve(m, "https://a/", Limits::default(), &NoInteraction);
    assert!(tokio::time::timeout(Duration::from_millis(300), call).await.is_err());
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(site.requests().is_empty(), "the dropped call went on");
}

const LISTS: &str = r#"
export default {
  resolve(url) { return { url: url + '/direct' }; },
  list(url) {
    return [
      { url: url + '#path=a.zip', filename: 'a.zip', size: 1 },
      { url: url + '#path=d%2Fe%2Fb.zip', filename: 'b.zip', folder: '/d\\ e /' },
    ];
  },
};
"#;

#[tokio::test]
async fn list_returns_files_with_normalized_folders() {
    let dir = TempDir::new().unwrap();
    let items = host().list(module(&dir, "lists", LISTS), "https://s/1", Limits::default(), &NoInteraction).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!((items[0].url.as_str(), items[0].filename.as_deref(), items[0].size, items[0].folder.as_deref()), ("https://s/1#path=a.zip", Some("a.zip"), Some(1), None));
    assert_eq!(items[1].folder.as_deref(), Some("d/e"), "split on / and \\, trimmed, empty parts dropped");
}

#[tokio::test]
async fn a_module_without_list_lists_the_url_itself() {
    let dir = TempDir::new().unwrap();
    let items = host().list(module(&dir, "ok", OK), "https://s/1", Limits::default(), &NoInteraction).await.unwrap();
    assert_eq!(items, vec![Listed { url: "https://s/1".into(), filename: None, size: None, folder: None }]);
}

#[tokio::test]
async fn invalid_lists_are_rejected() {
    let dir = TempDir::new().unwrap();
    let host = host();
    for (name, result, says) in [
        ("relative", "[{ url: '/a', filename: 'a' }]", "not absolute"),
        ("twice", "[{ url: 'https://s/1#a', filename: 'a' }, { url: 'https://s/1#a', filename: 'b' }]", "twice"),
        ("unnamed", "[{ url: 'https://s/1#a' }]", "no filename"),
        ("blank", "[{ url: 'https://s/1#a', filename: ' ' }]", "no filename"),
        ("escape", "[{ url: 'https://s/1#a', filename: 'a', folder: 'x/../../etc' }]", "not a folder name"),
        ("drive", "[{ url: 'https://s/1#a', filename: 'a', folder: 'C:/x' }]", "not a folder name"),
        ("typo", "[{ url: 'https://s/1#a', fileName: 'a' }]", "invalid result"),
        ("object", "{ url: 'https://s/1' }", "invalid result"),
        ("too-many", "Array.from({ length: 1001 }, (_, i) => ({ url: 'https://s/1#' + i, filename: 'f' + i }))", "at most 1000"),
    ] {
        let m = module(&dir, name, &format!("export default {{ resolve() {{}}, list() {{ return {result}; }} }};"));
        match host.list(m, "https://s/1", Limits::default(), &NoInteraction).await {
            Err(ModuleError::Script { message, .. }) => assert!(message.contains(says), "{name}: {message}"),
            other => panic!("{name}: expected script error, got {other:?}"),
        }
    }

    let not_a_function = module(&dir, "not-fn", "export default { resolve() {}, list: [] };");
    assert!(matches!(host.list(not_a_function, "https://s/1", Limits::default(), &NoInteraction).await, Err(ModuleError::Load { .. })));
}

#[tokio::test]
async fn listed_urls_never_carry_credentials() {
    let dir = TempDir::new().unwrap();
    let mut m = module(
        &dir,
        "leaky",
        "export default { resolve() {}, list(url, zoo) { return [{ url: url + '?k=' + zoo.credentials.placeholder('key'), filename: 'a' }]; } };",
    );
    m.grants.credentials.insert("key".into(), Secret("s3cret-value".into()));
    match host().list(m, "https://s/1", Limits::default(), &NoInteraction).await {
        Err(ModuleError::Script { message, .. }) => {
            assert!(message.contains("credential placeholder") && !message.contains("s3cret"), "{message}")
        }
        other => panic!("expected script error, got {other:?}"),
    }
}
