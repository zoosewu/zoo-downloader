//! Transfer behaviour against a local fixture server.

mod common;

use common::{expected, Fixture};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use zoo_core::connection::Direct;
use zoo_core::rate::RateLimiter;
use zoo_core::transfer::{self, Ctx, Progress, RetryPolicy, TransferSpec};
use zoo_core::cipher::Decrypt;
use zoo_core::CoreError;

const KB: u64 = 1024;

fn spec(dir: &Path, url: String, connections: usize) -> TransferSpec {
    TransferSpec {
        origin: url.clone(),
        url,
        headers: Vec::new(),
        dir: dir.to_path_buf(),
        name: "out.bin".into(),
        part_path: dir.join("out.bin.test.part"),
        connections,
        segment_size: 64 * KB,
        retry: RetryPolicy { max_attempts: 4, base_delay: Duration::from_millis(10), max_delay: Duration::from_millis(50) },
        decrypt: None,
    }
}

fn ctx(limit: Option<u64>) -> Ctx {
    Ctx {
        conn: Arc::new(Direct::new().unwrap()),
        rate: Arc::new(RateLimiter::new(limit)),
        progress: Arc::new(Progress::default()),
        cancel: CancellationToken::new(),
    }
}

fn assert_file(path: &PathBuf, size: u64) {
    let data = std::fs::read(path).unwrap();
    assert_eq!(data.len() as u64, size);
    assert!(data == expected(size), "content mismatch");
}

fn no_leftovers(dir: &Path) {
    let names: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(
        names.iter().all(|n| !n.to_string_lossy().contains(".part")),
        "leftover part files: {names:?}"
    );
}

#[tokio::test]
async fn segmented_download_uses_many_ranges_and_completes() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 1000 * KB + 123;
    let c = ctx(None);

    let out = transfer::run(&spec(dir.path(), fx.url(&format!("/file/{size}")), 4), &c).await.unwrap();

    assert_file(&out, size);
    assert_eq!(c.progress.downloaded(), size);
    assert_eq!(c.progress.total(), Some(size));
    let ranged = fx.requests().iter().filter(|r| r.range.as_deref().is_some_and(|v| v != "bytes=0-0")).count();
    assert_eq!(ranged as u64, size.div_ceil(64 * KB));
    no_leftovers(dir.path());
}

#[tokio::test]
async fn server_without_range_support_streams_from_probe_response() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 300 * KB;

    let out = transfer::run(&spec(dir.path(), fx.url(&format!("/file/{size}?norange")), 4), &ctx(None)).await.unwrap();

    assert_file(&out, size);
    assert_eq!(fx.requests().len(), 1, "probe response should be reused as the body");
}

#[tokio::test]
async fn range_ignored_mid_transfer_falls_back_to_stream() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 300 * KB;

    let out = transfer::run(&spec(dir.path(), fx.url(&format!("/file/{size}?range200")), 4), &ctx(None)).await.unwrap();

    assert_file(&out, size);
    no_leftovers(dir.path());
}

#[tokio::test]
async fn dropped_connections_resume_within_segment() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 512 * KB;
    // The first three body responses die after 20 KiB.
    let url = fx.url(&format!("/file/{size}?drop={}&times=3", 20 * KB));

    let out = transfer::run(&spec(dir.path(), url, 2), &ctx(None)).await.unwrap();

    assert_file(&out, size);
    // Retries continue from the break, i.e. some request starts in the middle of a segment.
    let mid_segment = fx
        .requests()
        .iter()
        .filter_map(|r| r.range.as_deref()?.strip_prefix("bytes=")?.split_once('-')?.0.parse::<u64>().ok())
        .filter(|start| start % (64 * KB) != 0)
        .count();
    assert!(mid_segment >= 1, "expected a retry starting mid-segment: {:?}", fx.requests());
}

#[tokio::test]
async fn transient_errors_are_retried() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let url = fx.url(&format!("/file/{}?fail=503&times=2", 100 * KB));

    let out = transfer::run(&spec(dir.path(), url, 2), &ctx(None)).await.unwrap();

    assert_file(&out, 100 * KB);
}

#[tokio::test]
async fn connections_the_server_refuses_hand_their_ranges_to_the_others() {
    // Baidu Netdisk lets a free account's extra connections stall; other servers answer them
    // with an error status. Either way the download continues on the connections that work.
    for over in ["494", "drop"] {
        let fx = Fixture::start().await;
        let dir = tempfile::tempdir().unwrap();
        let size = 640 * KB;
        let url = fx.url(&format!("/file/{size}?limit=2&over={over}&delay_ms=2"));

        let out = transfer::run(&spec(dir.path(), url, 4), &ctx(None)).await;

        assert_file(&out.unwrap_or_else(|e| panic!("over={over}: {e}")), size);
        no_leftovers(dir.path());
    }
}

#[tokio::test]
async fn a_server_refusing_every_connection_still_fails() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let url = fx.url(&format!("/file/{}?limit=0&over=494", 640 * KB));

    let err = transfer::run(&spec(dir.path(), url, 4), &ctx(None)).await.unwrap_err();

    assert_eq!(err, CoreError::Http(494));
}

#[tokio::test]
async fn permanent_http_errors_fail_without_retry() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();

    let err = transfer::run(&spec(dir.path(), fx.url("/file/10?status=404"), 2), &ctx(None)).await.unwrap_err();

    assert_eq!(err, CoreError::Http(404));
    assert_eq!(fx.requests().len(), 1);
}

#[tokio::test]
async fn cancel_keeps_progress_and_resume_fetches_only_the_rest() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 2048 * KB;
    let url = fx.url(&format!("/file/{size}?delay_ms=5"));

    let first = ctx(None);
    let cancel = first.cancel.clone();
    let progress = first.progress.clone();
    let s = spec(dir.path(), url.clone(), 2);
    let run = tokio::spawn(async move { transfer::run(&s, &first).await });
    while progress.downloaded() < 256 * KB {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    cancel.cancel();
    assert_eq!(run.await.unwrap().unwrap_err(), CoreError::Cancelled);
    let kept = progress.downloaded();
    assert!(dir.path().join("out.bin.test.part").exists());

    let requests_before = fx.requests().len();
    let second = ctx(None);
    let out = transfer::run(&spec(dir.path(), url, 2), &second).await.unwrap();
    assert_file(&out, size);

    // Progress resumed from what was flushed (part of `kept`), and the rest was fetched once.
    let resumed_from = size - second_fetched(&fx.requests()[requests_before..]);
    assert!(resumed_from > 0 && resumed_from <= kept, "resumed_from={resumed_from} kept={kept}");
}

fn second_fetched(requests: &[common::Request]) -> u64 {
    requests
        .iter()
        .filter_map(|r| r.range.as_deref())
        .filter(|v| *v != "bytes=0-0")
        .map(|v| {
            let (s, e) = v.trim_start_matches("bytes=").split_once('-').unwrap();
            e.parse::<u64>().unwrap() - s.parse::<u64>().unwrap() + 1
        })
        .sum()
}

/// A new ETag (a share site's fresh link from another server) is checked by sampling the
/// progress, which is kept when the bytes match.
#[tokio::test]
async fn another_etag_with_the_same_bytes_keeps_the_progress() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 1024 * KB;
    let url = fx.url(&format!("/file/{size}?delay_ms=5"));

    let first = ctx(None);
    let (cancel, progress) = (first.cancel.clone(), first.progress.clone());
    let s = spec(dir.path(), url.clone(), 1);
    let run = tokio::spawn(async move { transfer::run(&s, &first).await });
    while progress.downloaded() < 128 * KB {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    cancel.cancel();
    let _ = run.await.unwrap();

    fx.set_etag("v2");
    let requests_before = fx.requests().len();
    let out = transfer::run(&spec(dir.path(), url, 2), &ctx(None)).await.unwrap();

    assert_file(&out, size);
    // The missing part plus at most four 64 KB samples, not the whole file again.
    let kept = progress.downloaded();
    let fetched = second_fetched(&fx.requests()[requests_before..]);
    assert!(fetched <= size - kept + 4 * 64 * KB, "fetched {fetched} of {size}, {kept} kept");
}

#[tokio::test]
async fn unknown_length_and_empty_files() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();

    let out = transfer::run(&spec(dir.path(), fx.url(&format!("/file/{}?norange&nolength", 70 * KB)), 2), &ctx(None))
        .await
        .unwrap();
    assert_file(&out, 70 * KB);

    let mut empty = spec(dir.path(), fx.url("/file/0"), 2);
    empty.name = "empty.bin".into();
    let out = transfer::run(&empty, &ctx(None)).await.unwrap();
    assert_eq!(std::fs::metadata(out).unwrap().len(), 0);
}

#[tokio::test]
async fn existing_file_is_never_overwritten() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("out.bin"), b"mine").unwrap();

    let out = transfer::run(&spec(dir.path(), fx.url("/file/1000"), 2), &ctx(None)).await.unwrap();

    assert_eq!(out, dir.path().join("out (1).bin"));
    assert_eq!(std::fs::read(dir.path().join("out.bin")).unwrap(), b"mine");
}

#[tokio::test]
async fn speed_limit_is_respected() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 300 * KB;
    let started = Instant::now();

    // 100 KiB/s with a one-second burst: 300 KiB needs at least ~2 s.
    transfer::run(&spec(dir.path(), fx.url(&format!("/file/{size}")), 4), &ctx(Some(100 * KB))).await.unwrap();

    let secs = started.elapsed().as_secs_f64();
    assert!(secs >= 1.8, "finished in {secs}s, faster than the limit allows");
}

const DECRYPT: Decrypt = Decrypt::Aes128Ctr { key: *b"0123456789abcdef", iv: [9, 8, 7, 6, 5, 4, 3, 2, 0, 0, 0, 0, 0, 0, 0, 0] };

/// The fixture's bytes, decrypted: what the file must hold (CTR is its own inverse).
fn decrypted(size: u64) -> Vec<u8> {
    let mut data = expected(size);
    DECRYPT.apply(0, &mut data);
    data
}

#[tokio::test]
async fn encrypted_files_are_saved_decrypted() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 300 * KB + 7;
    for (name, query, connections) in [("segments.bin", "", 4), ("stream.bin", "?norange", 1)] {
        let mut s = spec(dir.path(), fx.url(&format!("/file/{size}{query}")), connections);
        s.name = name.into();
        s.part_path = dir.path().join(format!("{name}.part"));
        s.decrypt = Some(DECRYPT);
        let out = transfer::run(&s, &ctx(None)).await.unwrap();
        assert!(std::fs::read(&out).unwrap() == decrypted(size), "{name}: content mismatch");
    }
}

#[tokio::test]
async fn a_resumed_encrypted_file_is_checked_against_its_decrypted_bytes() {
    let fx = Fixture::start().await;
    let dir = tempfile::tempdir().unwrap();
    let size = 1024 * KB;
    let url = fx.url(&format!("/file/{size}?delay_ms=5"));
    let encrypted = |connections| TransferSpec { decrypt: Some(DECRYPT), ..spec(dir.path(), url.clone(), connections) };

    let first = ctx(None);
    let (cancel, progress) = (first.cancel.clone(), first.progress.clone());
    let s = encrypted(1);
    let run = tokio::spawn(async move { transfer::run(&s, &first).await });
    while progress.downloaded() < 128 * KB {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    cancel.cancel();
    let _ = run.await.unwrap();

    // Another ETag: the kept bytes are sampled against the server's, decrypted the same way.
    fx.set_etag("v2");
    let out = transfer::run(&encrypted(2), &ctx(None)).await.unwrap();
    assert!(std::fs::read(&out).unwrap() == decrypted(size), "content mismatch");
}
