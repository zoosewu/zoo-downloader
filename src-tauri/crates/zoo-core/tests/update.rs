//! Changing a task's URL or headers, and restarting it: through DownloadManager only.

mod common;

use common::{alt_byte_at, expected, Fixture};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use zoo_core::connection::Direct;
use zoo_core::transfer::RetryPolicy;
use zoo_core::{AddRequest, AutoRetry, Config, CoreError, DownloadManager, StartMode, Task, TaskId, TaskState, TaskUpdate};

const KB: u64 = 1024;
const SIZE: u64 = 1024 * KB;

fn config() -> Config {
    Config {
        max_concurrent: Some(3),
        connections_per_task: 2,
        min_split: 64 * KB,
        speed_limit: None,
        auto_retry: AutoRetry { enabled: true, max_attempts: 3, interval: Duration::from_millis(50) },
        transfer_retry: RetryPolicy { max_attempts: 2, base_delay: Duration::from_millis(10), max_delay: Duration::from_millis(20) },
        user_agent: None,
        categories: Default::default(),
    }
}

fn start(cfg: Config, store: &Path) -> DownloadManager {
    DownloadManager::start(cfg, store, Arc::new(Direct::new().unwrap())).unwrap()
}

fn req(url: String, dir: &Path) -> AddRequest {
    AddRequest { url, filename: None, dir: dir.to_path_buf(), start: StartMode::Now, headers: Vec::new(), tags: None, answers: Default::default(), note: String::new(), page_url: None }
}

async fn wait_until(m: &DownloadManager, id: TaskId, pred: impl Fn(&Task) -> bool) -> Task {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let t = m.list().await.unwrap().into_iter().find(|t| t.id == id).expect("task listed");
        if pred(&t) {
            return t;
        }
        assert!(tokio::time::Instant::now() < deadline, "timed out; last state: {t:#?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn url_update(url: String) -> TaskUpdate {
    TaskUpdate { url: Some(url), headers: None }
}

fn content(task: &Task) -> Vec<u8> {
    std::fs::read(task.final_path.as_ref().expect("final path")).unwrap()
}

/// Bytes the fixture served for `path` in non-probe ranged requests (whole bodies count fully).
fn fetched(fx: &Fixture, from: usize, path_query: &str) -> u64 {
    let (path, _) = path_query.split_once('?').unwrap_or((path_query, ""));
    fx.requests()[from..]
        .iter()
        .filter(|r| r.path == path)
        .map(|r| match r.range.as_deref() {
            Some("bytes=0-0") => 0,
            Some(v) => {
                let (s, e) = v.trim_start_matches("bytes=").split_once('-').unwrap();
                e.parse::<u64>().unwrap() - s.parse::<u64>().unwrap() + 1
            }
            None => SIZE,
        })
        .sum()
}

/// The most a resume may fetch: what is missing, plus the samples that check a new URL.
fn resume_budget(paused: &Task, checked: bool) -> u64 {
    SIZE - paused.downloaded + if checked { 4 * 64 * KB } else { 0 }
}

/// A task paused with some progress at a slow URL.
async fn paused_partway(m: &DownloadManager, fx: &Fixture, dir: &Path) -> Task {
    let id = m.add(req(fx.url(&format!("/file/{SIZE}?delay_ms=20")), dir)).await.unwrap().id;
    wait_until(m, id, |t| t.downloaded > 400 * KB).await;
    m.pause(id).await.unwrap();
    // Let the job stop and flush its progress.
    tokio::time::sleep(Duration::from_millis(200)).await;
    wait_until(m, id, |t| t.state == TaskState::Paused).await
}

#[tokio::test]
async fn a_mirror_with_the_same_bytes_keeps_the_progress() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let m = start(config(), store.path());
    let paused = paused_partway(&m, &fx, dir.path()).await;

    let mirror = fx.url(&format!("/file/{SIZE}?mirror"));
    let updated = m.update(paused.id, url_update(mirror.clone())).await.unwrap();
    assert_eq!((updated.url.as_str(), updated.state), (mirror.as_str(), TaskState::Paused));
    let before = fx.requests().len();
    m.resume(paused.id).await.unwrap();
    let done = wait_until(&m, paused.id, |t| t.state == TaskState::Completed).await;

    assert_eq!(content(&done), expected(SIZE));
    let refetched = fetched(&fx, before, &format!("/file/{SIZE}"));
    assert!(refetched <= resume_budget(&paused, true), "the mirror refetched {refetched} of {SIZE} bytes");
    assert!(resume_budget(&paused, true) < SIZE, "the test needs more progress before the pause");
}

#[tokio::test]
async fn different_bytes_fail_the_task_and_keep_the_progress_until_restart() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let m = start(config(), store.path());
    let paused = paused_partway(&m, &fx, dir.path()).await;

    let other = fx.url(&format!("/file/{SIZE}?alt"));
    m.update(paused.id, url_update(other)).await.unwrap();
    m.resume(paused.id).await.unwrap();
    let failed = wait_until(&m, paused.id, |t| t.state == TaskState::Failed).await;
    assert!(matches!(failed.error, Some(CoreError::SourceChanged(_))), "{:?}", failed.error);
    assert_eq!(failed.next_retry_at, None, "not retried automatically");
    assert!(failed.downloaded >= paused.downloaded, "progress is kept");

    // Retrying asks the same question again; restarting answers it.
    m.retry(paused.id).await.unwrap();
    let again = wait_until(&m, paused.id, |t| t.state == TaskState::Failed).await;
    assert!(matches!(again.error, Some(CoreError::SourceChanged(_))));

    let restarted = m.restart(paused.id).await.unwrap();
    assert!(matches!(restarted.state, TaskState::Queued | TaskState::Running), "{:?}", restarted.state);
    assert_eq!((restarted.downloaded, restarted.error.clone()), (0, None));
    let done = wait_until(&m, paused.id, |t| t.state == TaskState::Completed).await;
    assert_eq!(content(&done), (0..SIZE).map(alt_byte_at).collect::<Vec<_>>());
}

#[tokio::test]
async fn a_url_that_cannot_be_checked_counts_as_different() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let m = start(config(), store.path());
    let paused = paused_partway(&m, &fx, dir.path()).await;

    for url in [fx.url(&format!("/file/{}", SIZE + 1)), fx.url(&format!("/file/{SIZE}?norange"))] {
        m.update(paused.id, url_update(url.clone())).await.unwrap();
        m.resume(paused.id).await.unwrap();
        let failed = wait_until(&m, paused.id, |t| t.state == TaskState::Failed).await;
        assert!(matches!(failed.error, Some(CoreError::SourceChanged(_))), "{url}: {:?}", failed.error);
    }

    // Back to the original URL: the progress was never lost.
    let before = fx.requests().len();
    m.update(paused.id, url_update(paused.url.clone())).await.unwrap();
    m.resume(paused.id).await.unwrap();
    let done = wait_until(&m, paused.id, |t| t.state == TaskState::Completed).await;
    assert_eq!(content(&done), expected(SIZE));
    assert!(fetched(&fx, before, &format!("/file/{SIZE}")) <= resume_budget(&paused, false));
}

#[tokio::test]
async fn headers_and_the_default_user_agent_reach_the_server() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let m = start(Config { user_agent: Some("Configured/1.0".into()), ..config() }, store.path());

    let headers = vec![("Referer".to_string(), "https://site.example/".to_string()), ("Cookie".to_string(), "sid=1".to_string())];
    let a = m.add(AddRequest { headers, ..req(fx.url("/file/5000?a"), dir.path()) }).await.unwrap();
    wait_until(&m, a.id, |t| t.state == TaskState::Completed).await;
    let b = m.add(AddRequest { headers: vec![("User-Agent".into(), "Mine/2.0".into())], ..req(fx.url("/file/6000"), dir.path()) });
    let b = b.await.unwrap();
    wait_until(&m, b.id, |t| t.state == TaskState::Completed).await;

    let requests = fx.requests();
    let header = |r: &common::Request, name: &str| r.headers.get(name).cloned();
    for r in requests.iter().filter(|r| r.path == "/file/5000") {
        assert_eq!(header(r, "referer").as_deref(), Some("https://site.example/"));
        assert_eq!(header(r, "cookie").as_deref(), Some("sid=1"));
        assert_eq!(header(r, "user-agent").as_deref(), Some("Configured/1.0"));
    }
    assert!(requests.iter().filter(|r| r.path == "/file/6000").all(|r| header(r, "user-agent").as_deref() == Some("Mine/2.0")));

    // Headers can be changed later, like the URL.
    let c = m.add(AddRequest { start: StartMode::Later, ..req(fx.url("/file/7000"), dir.path()) }).await.unwrap();
    let cookie = vec![("Cookie".to_string(), "sid=2".to_string())];
    m.update(c.id, TaskUpdate { url: None, headers: Some(cookie.clone()) }).await.unwrap();
    m.resume(c.id).await.unwrap();
    let done = wait_until(&m, c.id, |t| t.state == TaskState::Completed).await;
    assert_eq!(done.headers, cookie);
    assert!(fx.requests().iter().filter(|r| r.path == "/file/7000").all(|r| header(r, "cookie").as_deref() == Some("sid=2")));
}

#[tokio::test]
async fn bad_updates_and_headers_are_rejected() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let m = start(config(), store.path());

    for headers in [vec![("Range".to_string(), "bytes=0-".to_string())], vec![("Bad Name".into(), "x".into())], vec![("X".into(), "a\nb".into())]] {
        let result = m.add(AddRequest { headers: headers.clone(), ..req(fx.url("/file/10"), dir.path()) }).await;
        assert!(matches!(result, Err(CoreError::Invalid(_))), "{headers:?}: {result:?}");
    }

    let running = m.add(req(fx.url(&format!("/file/{SIZE}?delay_ms=5")), dir.path())).await.unwrap().id;
    wait_until(&m, running, |t| t.state == TaskState::Running).await;
    assert!(matches!(m.update(running, url_update(fx.url("/file/10"))).await, Err(CoreError::Invalid(_))));
    assert!(matches!(m.restart(running).await, Err(CoreError::Invalid(_))));
    m.pause(running).await.unwrap();
    assert!(matches!(m.update(running, url_update("ftp://x/y".into())).await, Err(CoreError::Invalid(_))));
}
