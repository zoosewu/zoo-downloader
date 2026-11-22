//! Downloads through the proxy pool. The fixture server doubles as an HTTP proxy for its own
//! URLs and records which requests arrived in proxy (absolute-URI) form.

mod common;

use common::{expected, Fixture};
use std::sync::Arc;
use std::time::Duration;
use zoo_core::proxy_pool::{ProxyPool, ProxyStatus};
use zoo_core::transfer::RetryPolicy;
use zoo_core::{AddRequest, AutoRetry, Config, CoreError, DownloadManager, StartMode, TaskState};

fn config() -> Config {
    Config {
        max_concurrent: Some(2),
        connections_per_task: 4,
        min_split: 64 * 1024,
        speed_limit: None,
        auto_retry: AutoRetry { enabled: false, max_attempts: 0, interval: Duration::from_millis(10) },
        transfer_retry: RetryPolicy { max_attempts: 4, base_delay: Duration::from_millis(5), max_delay: Duration::from_millis(10) },
        user_agent: None,
        categories: Default::default(),
    }
}

async fn download(m: &DownloadManager, url: String, dir: &std::path::Path) -> zoo_core::Task {
    let id = m.add(AddRequest { url, filename: None, dir: dir.into(), start: StartMode::Now, headers: Vec::new(), tags: None, answers: Default::default(), note: String::new(), page_url: None }).await.unwrap().id;
    for _ in 0..500 {
        let t = m.list().await.unwrap().into_iter().find(|t| t.id == id).unwrap();
        if matches!(t.state, TaskState::Completed | TaskState::Failed) {
            return t;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("download did not finish");
}

#[tokio::test]
async fn segments_are_spread_across_proxies() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let pool = Arc::new(ProxyPool::open(store.path().join("proxies.json")).unwrap());
    // Two pool entries that both reach the fixture: "127.0.0.1" and "localhost".
    pool.import(&format!("127.0.0.1:{0}\nlocalhost:{0}", fx.addr.port()));
    let m = DownloadManager::start(config(), store.path(), pool.clone()).unwrap();

    let size = 512 * 1024;
    let task = download(&m, fx.url(&format!("/file/{size}")), dir.path()).await;

    assert_eq!(task.state, TaskState::Completed);
    assert!(std::fs::read(task.final_path.unwrap()).unwrap() == expected(size));
    assert!(fx.requests().iter().all(|r| r.proxied), "every request went through a proxy");
    let used: Vec<_> = pool.list().into_iter().map(|p| (p.host, p.successful_requests, p.status)).collect();
    assert!(used.iter().all(|(_, ok, status)| *ok > 0 && *status == ProxyStatus::Active), "{used:?}");
}

#[tokio::test]
async fn dead_proxy_is_blacklisted_and_direct_fallback_finishes_the_job() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let dead_port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    }; // closed again: connections are refused
    let pool = Arc::new(ProxyPool::open(store.path().join("proxies.json")).unwrap().with_failure_limit(2));
    pool.import(&format!("127.0.0.1:{dead_port}"));
    let m = DownloadManager::start(config(), store.path(), pool.clone()).unwrap();

    let task = download(&m, fx.url("/file/100000"), dir.path()).await;

    assert_eq!(task.state, TaskState::Completed);
    assert_eq!(pool.list()[0].status, ProxyStatus::Blacklisted);
    assert!(fx.requests().iter().all(|r| !r.proxied), "the fixture was reached directly");
}

#[tokio::test]
async fn without_fallback_a_dead_pool_fails_the_task() {
    let fx = Fixture::start().await;
    let (dir, store) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let dead_port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let pool = Arc::new(ProxyPool::open(store.path().join("proxies.json")).unwrap().with_failure_limit(1));
    pool.import(&format!("127.0.0.1:{dead_port}"));
    pool.set_fallback_direct(false);
    let m = DownloadManager::start(config(), store.path(), pool.clone()).unwrap();

    let task = download(&m, fx.url("/file/1000"), dir.path()).await;

    assert_eq!(task.state, TaskState::Failed);
    assert!(matches!(task.error, Some(CoreError::NoConnection(_))), "{:?}", task.error);
    assert!(fx.requests().is_empty());
}
