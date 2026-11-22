//! One task's transfer: probe the server, fetch the missing bytes, finalize the file.
//!
//! Ranged mode (server honours `Range` and reports a size): the missing byte ranges of the
//! part file are cut into segments and fetched by up to `connections` workers. Each segment
//! retries from where it stopped, so a dropped connection costs only the in-flight bytes.
//!
//! Stream mode (no range support or unknown size): one GET from byte 0; a failure restarts it.
//!
//! Cancelling keeps a ranged part file for a later resume; the caller decides whether to
//! discard it.

use crate::cipher::Decrypt;
use crate::connection::{ConnectionSource, Lease, Outcome};
use crate::error::CoreError;
use crate::part_file::{PartFile, Previous, Validator};
use crate::rate::RateLimiter;
use crate::task::ActiveRange;
use futures_util::StreamExt;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, ETAG, LAST_MODIFIED, RANGE};
use reqwest::{Response, StatusCode};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct TransferSpec {
    /// The task's own URL. Identifies the resource for resuming, so a part file survives
    /// the direct link changing between resolutions.
    pub origin: String,
    /// What to fetch: the resolved direct link.
    pub url: String,
    /// Sent with every request (from resolution: Referer, Cookie, ...).
    pub headers: Vec<(String, String)>,
    /// Directory and file name of the finished file (renamed on conflict).
    pub dir: PathBuf,
    pub name: String,
    /// In-progress file; see `naming::part_path`.
    pub part_path: PathBuf,
    pub connections: usize,
    /// The least a split of a worker's range leaves on either side (see `run_workers`).
    pub min_split: u64,
    /// The most one request may ask for (sites that refuse large ranges); `None`: a worker's
    /// whole range in one request.
    pub max_request: Option<u64>,
    pub retry: RetryPolicy,
    /// The server's bytes are encrypted: decrypted before they are written (or compared).
    pub decrypt: Option<Decrypt>,
}

impl TransferSpec {
    /// The server's `bytes` at `offset` as the file holds them.
    fn plain<'a>(&self, offset: u64, bytes: &'a [u8]) -> Cow<'a, [u8]> {
        match &self.decrypt {
            None => Cow::Borrowed(bytes),
            Some(decrypt) => {
                let mut plain = bytes.to_vec();
                decrypt.apply(offset, &mut plain);
                Cow::Owned(plain)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Attempts per segment (and per stream / probe) without progress before giving up.
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 5, base_delay: Duration::from_secs(1), max_delay: Duration::from_secs(30) }
    }
}

impl RetryPolicy {
    /// Delay before retry number `attempt` (1-based): base, 2×base, 4×base, … capped.
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u32.checked_shl(attempt.saturating_sub(1)).unwrap_or(u32::MAX);
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }
}

/// Live counters the manager samples for progress and speed, plus the segment picture.
pub struct Progress {
    downloaded: AtomicU64,
    total: AtomicU64,
    /// The part file while a ranged transfer runs; `None` in stream mode or between attempts.
    part: Mutex<Option<Arc<PartFile>>>,
    /// What each worker is fetching right now, by worker index.
    active: Mutex<BTreeMap<usize, ActiveRange>>,
}

const UNKNOWN: u64 = u64::MAX;

impl Default for Progress {
    fn default() -> Self {
        Self {
            downloaded: AtomicU64::new(0),
            total: AtomicU64::new(UNKNOWN),
            part: Mutex::new(None),
            active: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Progress {
    /// Counters that start where the task left off, so a job that fails before it transfers
    /// anything does not report the task's progress as lost.
    pub fn starting_at(downloaded: u64, total: Option<u64>) -> Self {
        let progress = Self::default();
        progress.reset(downloaded, total);
        progress
    }

    /// Finished and in-flight ranges of a running ranged transfer; `None` in stream mode.
    pub fn segments(&self) -> Option<(Vec<(u64, u64)>, Vec<ActiveRange>)> {
        let done = self.part.lock().unwrap().as_ref()?.done_ranges();
        let active = self.active.lock().unwrap().values().copied().collect();
        Some((done, active))
    }

    fn set_part(&self, part: Option<Arc<PartFile>>) {
        *self.part.lock().unwrap() = part;
        self.active.lock().unwrap().clear();
    }

    fn set_active(&self, worker: usize, range: Option<ActiveRange>) {
        let mut active = self.active.lock().unwrap();
        match range {
            Some(r) => active.insert(worker, r),
            None => active.remove(&worker),
        };
    }

    pub fn downloaded(&self) -> u64 {
        self.downloaded.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> Option<u64> {
        Some(self.total.load(Ordering::Relaxed)).filter(|&t| t != UNKNOWN)
    }

    fn reset(&self, downloaded: u64, total: Option<u64>) {
        self.downloaded.store(downloaded, Ordering::Relaxed);
        self.total.store(total.unwrap_or(UNKNOWN), Ordering::Relaxed);
    }

    fn add(&self, n: u64) {
        self.downloaded.fetch_add(n, Ordering::Relaxed);
    }
}

/// Everything a transfer shares with the rest of the system.
#[derive(Clone)]
pub struct Ctx {
    pub conn: Arc<dyn ConnectionSource>,
    pub rate: Arc<RateLimiter>,
    pub progress: Arc<Progress>,
    pub cancel: CancellationToken,
}

/// Run the transfer to completion. Returns the final file path.
pub async fn run(spec: &TransferSpec, ctx: &Ctx) -> Result<PathBuf, CoreError> {
    let probed = probe_source(spec, ctx).await?;
    fetch(spec, ctx, probed).await
}

/// What the first request learned about the source.
pub struct Probed {
    probe: Probe,
    /// The server's file name (`Content-Disposition`), unsanitized.
    pub filename: Option<String>,
}

/// The first request, retried like any other. The caller may rename the spec before `fetch`.
pub async fn probe_source(spec: &TransferSpec, ctx: &Ctx) -> Result<Probed, CoreError> {
    with_retry(spec.retry, &ctx.cancel, || probe(spec, ctx)).await
}

/// Fetch the missing bytes and finalize the file.
pub async fn fetch(spec: &TransferSpec, ctx: &Ctx, probed: Probed) -> Result<PathBuf, CoreError> {
    let probe = probed.probe;
    let remaining = match &probe {
        Probe::Ranged(v) => v.size.map(|size| size.saturating_sub(ctx.progress.downloaded())),
        Probe::Whole(response) => response.content_length(),
    };
    if let Some(needed) = remaining {
        check_space(&spec.dir, needed)?;
    }
    match probe {
        Probe::Ranged(validator) => match ranged(spec, ctx, validator).await {
            Err(SegmentError::RangeIgnored) => stream(spec, ctx, None).await,
            Err(SegmentError::Core(e)) => Err(e),
            Ok(path) => Ok(path),
        },
        Probe::Whole(response) => {
            // Progress cannot be checked against a server without ranges: the user decides.
            let no_size = Validator { url: spec.origin.clone(), size: None, etag: None, last_modified: None };
            if let Previous::Changed(_) = PartFile::open(spec.part_path.clone(), &no_size)? {
                return Err(no_ranges());
            }
            stream(spec, ctx, Some(response)).await
        }
    }
}

/// Fail early when `needed` more bytes will not fit. The directory may not exist yet: ask
/// about the nearest one that does. If the disk cannot be asked, let the writes find out.
fn check_space(dir: &Path, needed: u64) -> Result<(), CoreError> {
    let Some(existing) = dir.ancestors().find(|d| d.is_dir()) else { return Ok(()) };
    match fs4::available_space(existing) {
        Ok(available) if available < needed => Err(CoreError::InsufficientSpace { needed, available }),
        Ok(_) => Ok(()),
        Err(e) => {
            log::warn!("cannot read free space of {}: {e}", existing.display());
            Ok(())
        }
    }
}

fn no_ranges() -> CoreError {
    CoreError::SourceChanged("the link does not support resuming, so the download so far cannot be checked".into())
}

enum Probe {
    Ranged(Validator),
    /// The server ignored the range; this response already carries the whole body.
    Whole(Response),
}

async fn probe(spec: &TransferSpec, ctx: &Ctx) -> Result<Probed, CoreError> {
    let (lease, response) = send(ctx, spec, Some("bytes=0-0".into())).await?;
    let filename = response.headers().get(CONTENT_DISPOSITION).and_then(|v| crate::naming::from_content_disposition(v.as_bytes()));
    let probe = probe_kind(spec, ctx, lease, response).await?;
    Ok(Probed { probe, filename })
}

async fn probe_kind(spec: &TransferSpec, ctx: &Ctx, lease: Lease, response: Response) -> Result<Probe, CoreError> {
    let header = |name| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let validator =
        |size| Validator { url: spec.origin.clone(), size, etag: header(ETAG), last_modified: header(LAST_MODIFIED) };

    match response.status() {
        StatusCode::PARTIAL_CONTENT => match header(CONTENT_RANGE).as_deref().and_then(parse_content_range) {
            Some(ContentRange { total: Some(total), .. }) => Ok(Probe::Ranged(validator(Some(total)))),
            // Ranges work but the size is unknown: cannot plan segments.
            _ => Ok(Probe::Whole(send(ctx, spec, None).await?.1)),
        },
        StatusCode::OK => Ok(Probe::Whole(response)),
        StatusCode::RANGE_NOT_SATISFIABLE if header(CONTENT_RANGE).is_some_and(|v| v.ends_with("/0")) => {
            Ok(Probe::Ranged(validator(Some(0))))
        }
        status => {
            ctx.conn.report(&lease, Outcome::Failure);
            Err(CoreError::Http(status.as_u16()))
        }
    }
}

/// Send a GET for the spec's source through the connection source, scoring the lease by
/// time to first byte.
async fn send(ctx: &Ctx, spec: &TransferSpec, range: Option<String>) -> Result<(Lease, Response), CoreError> {
    let lease = ctx.conn.acquire()?;
    let mut request = lease.client.get(&spec.url);
    for (name, value) in &spec.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    if let Some(range) = range {
        request = request.header(RANGE, range);
    }
    let started = Instant::now();
    let sent = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(CoreError::Cancelled),
        r = request.send() => r,
    };
    match sent {
        Ok(response) => {
            ctx.conn.report(&lease, Outcome::Success { latency: started.elapsed() });
            Ok((lease, response))
        }
        Err(e) => {
            ctx.conn.report(&lease, Outcome::Failure);
            Err(e.into())
        }
    }
}

enum SegmentError {
    /// A ranged request came back as 200: the server stopped honouring ranges.
    RangeIgnored,
    Core(CoreError),
}

impl From<CoreError> for SegmentError {
    fn from(e: CoreError) -> Self {
        SegmentError::Core(e)
    }
}

impl From<std::io::Error> for SegmentError {
    fn from(e: std::io::Error) -> Self {
        SegmentError::Core(e.into())
    }
}

async fn ranged(spec: &TransferSpec, ctx: &Ctx, validator: Validator) -> Result<PathBuf, SegmentError> {
    let part = match PartFile::open(spec.part_path.clone(), &validator)? {
        Previous::Same(part) => part,
        Previous::Unverified(part) => {
            verify(spec, ctx, &part).await?;
            part.flush()?;
            part
        }
        Previous::Changed(why) => return Err(CoreError::SourceChanged(why).into()),
        Previous::None => {
            PartFile::discard(&spec.part_path)?;
            PartFile::create(spec.part_path.clone(), validator)?
        }
    };
    ctx.progress.reset(part.downloaded(), part.size());
    let part = Arc::new(part);
    ctx.progress.set_part(Some(part.clone()));
    let result = run_workers(spec, ctx, &part).await;
    // Release the shared handle before flushing, finalizing or deleting the file.
    ctx.progress.set_part(None);
    // Keep whatever was fetched for the next attempt, whether we failed or not.
    part.flush()?;
    if let Err(e) = result {
        if matches!(e, SegmentError::RangeIgnored) {
            drop(part);
            PartFile::discard(&spec.part_path)?;
        }
        return Err(e);
    }

    let part = Arc::try_unwrap(part).map_err(|_| CoreError::Protocol("part file still shared".into()))?;
    Ok(part.finalize(&spec.dir, &spec.name)?)
}

const SAMPLES: u64 = 4;
const SAMPLE_LEN: u64 = 64 * 1024;

/// The link is not the one `part` was downloaded from (a new URL, or another server's ETag):
/// sampled ranges of what it holds must come back identical, or the progress is not this file's.
async fn verify(spec: &TransferSpec, ctx: &Ctx, part: &PartFile) -> Result<(), CoreError> {
    for range in samples(&part.done_ranges()) {
        let mut ours = vec![0; (range.end - range.start) as usize];
        part.read_at(range.start, &mut ours)?;
        let theirs = with_retry(spec.retry, &ctx.cancel, || fetch_sample(spec, ctx, range.clone())).await?;
        if *spec.plain(range.start, &theirs) != ours[..] {
            return Err(CoreError::SourceChanged(format!("bytes {}-{} differ", range.start, range.end - 1)));
        }
    }
    Ok(())
}

async fn fetch_sample(spec: &TransferSpec, ctx: &Ctx, range: Range<u64>) -> Result<Vec<u8>, CoreError> {
    let (lease, response) = send(ctx, spec, Some(format!("bytes={}-{}", range.start, range.end - 1))).await?;
    match response.status() {
        StatusCode::PARTIAL_CONTENT => {}
        StatusCode::OK => return Err(no_ranges()),
        status => {
            ctx.conn.report(&lease, Outcome::Failure);
            return Err(CoreError::Http(status.as_u16()));
        }
    }
    let content_range = response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok()).and_then(parse_content_range);
    if !content_range.is_some_and(|cr| cr.start == range.start && cr.end_inclusive == range.end - 1) {
        return Err(CoreError::Protocol(format!("asked for bytes {}-{}, got another range", range.start, range.end - 1)));
    }
    let body = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(CoreError::Cancelled),
        body = response.bytes() => body?,
    };
    Ok(body.to_vec())
}

/// Up to `SAMPLES` ranges of at most `SAMPLE_LEN` bytes, spread evenly over the done bytes
/// (the first and the last included).
fn samples(done: &[(u64, u64)]) -> Vec<Range<u64>> {
    let covered: u64 = done.iter().map(|(s, e)| e - s).sum();
    let len = SAMPLE_LEN.min(covered);
    let mut out: Vec<Range<u64>> = Vec::new();
    if len == 0 {
        return out;
    }
    for i in 0..SAMPLES {
        // Position `at` counts done bytes only; find the range it falls in.
        let mut at = i * (covered - len) / (SAMPLES - 1);
        for &(s, e) in done {
            if at < e - s {
                let range = s + at..(s + at + len).min(e);
                if out.last() != Some(&range) {
                    out.push(range);
                }
                break;
            }
            at -= e - s;
        }
    }
    out
}

/// Fetch every missing range of `part` with up to `spec.connections` workers.
///
/// Each worker streams a range of its own, so there are few requests and no idle round trip
/// between pieces. A worker that runs out takes a range nobody holds (a gap of the part file, or
/// one a failing connection gave up), else the second half of what the busiest worker has left,
/// when both halves are still worth a connection. All connections stay busy until the end.
async fn run_workers(spec: &TransferSpec, ctx: &Ctx, part: &Arc<PartFile>) -> Result<(), SegmentError> {
    let workers = spec.connections.max(1);
    let plan = Arc::new(Plan::new(part.missing(), workers, spec.min_split));

    let mut set = JoinSet::new();
    let spec = Arc::new(spec.clone());
    for id in 0..workers {
        let (plan, part, ctx, spec) = (plan.clone(), part.clone(), ctx.clone(), spec.clone());
        set.spawn(async move {
            let result = worker(id, &spec, &plan, &part, &ctx).await;
            ctx.progress.set_active(id, None);
            result
        });
    }
    let mut failure = None;
    while let Some(joined) = set.join_next().await {
        let result = joined.unwrap_or_else(|e| Err(CoreError::Protocol(format!("worker crashed: {e}")).into()));
        if let Err(e) = result {
            set.abort_all();
            failure.get_or_insert(e);
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Show what `worker` is fetching (or waiting to retry) in the segment picture.
fn mark(ctx: &Ctx, worker: usize, range: &Range<u64>, retrying: bool) {
    ctx.progress.set_active(worker, Some(ActiveRange { start: range.start, end: range.end, retrying }));
}

/// A worker's range splits only when the other worker keeps at least this many seconds of
/// its own speed: less is over before a new connection gets going.
const SPLIT_SECONDS: f64 = 2.0;

/// Who fetches what. One lock for all of it: a range handed back is always taken by a worker
/// that has not left yet, and a split never races the bytes it moves.
struct Plan(Mutex<PlanState>);

struct PlanState {
    /// Ranges nobody holds.
    free: Vec<Range<u64>>,
    /// What each worker has left (`start` is the next byte it writes), by worker index.
    slots: Vec<Option<Slot>>,
    /// Workers that have not left.
    workers: usize,
}

struct Slot {
    left: Range<u64>,
    fetched: u64,
    since: Instant,
}

impl Slot {
    /// The least a split may leave this worker or give away.
    fn floor(&self, min_split: u64) -> u64 {
        // Its first moments say little about its speed.
        let secs = self.since.elapsed().as_secs_f64();
        let speed = if secs >= 1.0 { self.fetched as f64 / secs } else { 0.0 };
        min_split.max((speed * SPLIT_SECONDS) as u64)
    }
}

impl Plan {
    /// `free` split up front into a range per worker (halving the largest while that leaves
    /// `min_split` on each side), so no request starts out covering another worker's bytes.
    fn new(mut free: Vec<Range<u64>>, workers: usize, min_split: u64) -> Self {
        while free.len() < workers {
            let Some(i) = (0..free.len()).max_by_key(|&i| free[i].end - free[i].start) else { break };
            let range = free[i].clone();
            if range.end - range.start < 2 * min_split.max(1) {
                break;
            }
            let mid = range.start + (range.end - range.start) / 2;
            free[i] = range.start..mid;
            free.push(mid..range.end);
        }
        free.sort_by_key(|r| r.start);
        Plan(Mutex::new(PlanState { free, slots: (0..workers).map(|_| None).collect(), workers }))
    }

    /// The next range for `worker`, and the worker it was split from with what that one keeps.
    /// `None`: nothing worth taking; the worker leaves.
    fn next(&self, worker: usize, min_split: u64) -> Option<(Range<u64>, Option<(usize, Range<u64>)>)> {
        let mut st = self.0.lock().unwrap();
        // The largest free range; of equals, the first in the file.
        let largest = (0..st.free.len()).max_by_key(|&i| (st.free[i].end - st.free[i].start, std::cmp::Reverse(st.free[i].start)));
        let taken = if let Some(i) = largest {
            Some((st.free.swap_remove(i), None))
        } else {
            let busiest = (0..st.slots.len())
                .filter_map(|i| st.slots[i].as_ref().map(|s| (i, s.left.end.saturating_sub(s.left.start))))
                .max_by_key(|&(_, left)| left);
            busiest.and_then(|(victim, left)| {
                let slot = st.slots[victim].as_mut()?;
                if left < 2 * slot.floor(min_split) {
                    return None;
                }
                let mid = slot.left.start + left / 2;
                let given = mid..slot.left.end;
                slot.left.end = mid;
                Some((given, Some((victim, slot.left.clone()))))
            })
        };
        match taken {
            Some((range, robbed)) => {
                st.slots[worker] = Some(Slot { left: range.clone(), fetched: 0, since: Instant::now() });
                Some((range, robbed))
            }
            None => {
                st.slots[worker] = None;
                st.workers -= 1;
                None
            }
        }
    }

    /// What `worker` has left to fetch.
    fn left(&self, worker: usize) -> Range<u64> {
        self.0.lock().unwrap().slots[worker].as_ref().map_or(0..0, |s| s.left.clone())
    }

    /// Claim up to `len` bytes at `offset`, the worker's next byte; fewer (or none) when the
    /// range was split meanwhile.
    fn claim(&self, worker: usize, offset: u64, len: u64) -> u64 {
        let mut st = self.0.lock().unwrap();
        let Some(slot) = st.slots[worker].as_mut() else { return 0 };
        debug_assert_eq!(slot.left.start, offset, "a worker writes where it left off");
        let n = len.min(slot.left.end.saturating_sub(slot.left.start));
        slot.left.start += n;
        slot.fetched += n;
        n
    }

    /// Leave, giving what is left of the range to the remaining workers; false when this is the
    /// last worker (the range would be lost).
    fn hand_back(&self, worker: usize) -> bool {
        let mut st = self.0.lock().unwrap();
        if st.workers <= 1 {
            return false;
        }
        st.workers -= 1;
        if let Some(slot) = st.slots[worker].take() {
            if !slot.left.is_empty() {
                st.free.push(slot.left);
            }
        }
        true
    }
}

/// A connection the server stopped serving (it stalls, resets, or answers an error) while the
/// others still work: the server limits connections (Baidu Netdisk lets a free account's extra
/// connections stall), so give its range to the others instead of failing the download.
fn connection_refused(e: &CoreError) -> bool {
    matches!(e, CoreError::Http(_) | CoreError::Network(_) | CoreError::NoConnection(_))
}

async fn worker(id: usize, spec: &TransferSpec, plan: &Plan, part: &PartFile, ctx: &Ctx) -> Result<(), SegmentError> {
    while let Some((range, robbed)) = plan.next(id, spec.min_split) {
        if let Some((victim, kept)) = robbed {
            mark(ctx, victim, &kept, false);
        }
        mark(ctx, id, &range, false);
        let (retry, mut attempt) = (spec.retry, 0);
        loop {
            let left = plan.left(id);
            if left.is_empty() {
                break;
            }
            // Sites that refuse large ranges get several requests per range.
            let request = left.start..spec.max_request.map_or(left.end, |max| left.end.min(left.start + max.max(1)));
            let (result, reached) = fetch_range(spec, request.clone(), part, ctx, plan, id).await;
            let e = match result {
                Ok(()) => {
                    attempt = 0;
                    continue;
                }
                // Every byte arrived (or the rest was split off) before the connection failed.
                Err(SegmentError::Core(e)) if plan.left(id).is_empty() && e.is_retryable() => break,
                Err(SegmentError::Core(e)) if e.is_retryable() => {
                    // Progress earns a fresh set of attempts; only a stuck range gives up.
                    attempt = if reached > request.start { 1 } else { attempt + 1 };
                    if attempt < retry.max_attempts {
                        mark(ctx, id, &plan.left(id), true);
                        sleep_or_cancel(retry.delay(attempt), &ctx.cancel).await?;
                        mark(ctx, id, &plan.left(id), false);
                        continue;
                    }
                    e
                }
                Err(SegmentError::Core(e)) => e,
                Err(e) => return Err(e),
            };
            if connection_refused(&e) && plan.hand_back(id) {
                return Ok(());
            }
            return Err(e.into());
        }
    }
    Ok(())
}

/// Fetch `range` (where `worker` left off) into the part file, until it ends or the plan gives
/// its rest to another worker. Also returns how far it got.
async fn fetch_range(
    spec: &TransferSpec,
    range: Range<u64>,
    part: &PartFile,
    ctx: &Ctx,
    plan: &Plan,
    worker: usize,
) -> (Result<(), SegmentError>, u64) {
    let mut offset = range.start;
    let result = async {
        let (lease, response) = send(ctx, spec, Some(format!("bytes={}-{}", range.start, range.end - 1))).await?;
        match response.status() {
            StatusCode::PARTIAL_CONTENT => {}
            StatusCode::OK => return Err(SegmentError::RangeIgnored),
            status => {
                ctx.conn.report(&lease, Outcome::Failure);
                return Err(CoreError::Http(status.as_u16()).into());
            }
        }
        let content_range = response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok());
        match content_range.and_then(parse_content_range) {
            Some(cr) if cr.start == range.start && cr.end_inclusive == range.end - 1 => {}
            _ => {
                return Err(CoreError::Protocol(format!(
                    "asked for bytes {}-{}, got Content-Range {:?}",
                    range.start,
                    range.end - 1,
                    content_range
                ))
                .into())
            }
        }

        let mut body = response.bytes_stream();
        while offset < range.end {
            let chunk = tokio::select! {
                _ = ctx.cancel.cancelled() => return Err(CoreError::Cancelled.into()),
                next = body.next() => match next {
                    Some(chunk) => chunk.map_err(CoreError::from)?,
                    None => return Err(CoreError::Network("connection closed before segment end".into()).into()),
                },
            };
            let want = chunk.len().min((range.end - offset) as usize) as u64;
            let len = plan.claim(worker, offset, want) as usize;
            if len > 0 {
                ctx.rate.acquire(len as u64).await;
                part.write_at(offset, &spec.plain(offset, &chunk[..len]))?;
                offset += len as u64;
                ctx.progress.add(len as u64);
            }
            // The rest went to another worker: drop this response.
            if (len as u64) < want {
                return Ok(());
            }
        }
        Ok(())
    }
    .await;
    (result, offset)
}

/// Whole-body download from byte 0. Any failure restarts it, since the server cannot resume.
async fn stream(spec: &TransferSpec, ctx: &Ctx, mut first: Option<Response>) -> Result<PathBuf, CoreError> {
    let mut attempt = 0;
    loop {
        let result = async {
            let response = match first.take() {
                Some(r) => r,
                None => send(ctx, spec, None).await?.1,
            };
            if response.status() != StatusCode::OK {
                return Err(CoreError::Http(response.status().as_u16()));
            }
            let size = response.content_length();
            let validator = Validator { url: spec.origin.clone(), size, etag: None, last_modified: None };
            PartFile::discard(&spec.part_path)?;
            let part = PartFile::create(spec.part_path.clone(), validator)?;
            ctx.progress.reset(0, size);

            let mut offset = 0u64;
            let mut body = response.bytes_stream();
            loop {
                let chunk = tokio::select! {
                    _ = ctx.cancel.cancelled() => return Err(CoreError::Cancelled),
                    next = body.next() => match next {
                        Some(chunk) => chunk?,
                        None => break,
                    },
                };
                ctx.rate.acquire(chunk.len() as u64).await;
                part.write_at(offset, &spec.plain(offset, &chunk))?;
                offset += chunk.len() as u64;
                ctx.progress.add(chunk.len() as u64);
            }
            if size.is_some_and(|s| offset != s) {
                return Err(CoreError::Network(format!("connection closed at {offset} of {size:?} bytes")));
            }
            Ok(part.finalize(&spec.dir, &spec.name)?)
        }
        .await;

        match result {
            Err(e) if e.is_retryable() && attempt + 1 < spec.retry.max_attempts => {
                attempt += 1;
                sleep_or_cancel(spec.retry.delay(attempt), &ctx.cancel).await?;
            }
            other => return other,
        }
    }
}

async fn with_retry<T, F, Fut>(retry: RetryPolicy, cancel: &CancellationToken, mut f: F) -> Result<T, CoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CoreError>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Err(e) if e.is_retryable() && attempt + 1 < retry.max_attempts => {
                attempt += 1;
                sleep_or_cancel(retry.delay(attempt), cancel).await?;
            }
            other => return other,
        }
    }
}

async fn sleep_or_cancel(delay: Duration, cancel: &CancellationToken) -> Result<(), CoreError> {
    tokio::select! {
        _ = cancel.cancelled() => Err(CoreError::Cancelled),
        _ = tokio::time::sleep(delay) => Ok(()),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ContentRange {
    start: u64,
    end_inclusive: u64,
    total: Option<u64>,
}

/// Parse `bytes START-END/TOTAL` (TOTAL may be `*`).
fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.trim().strip_prefix("bytes ")?;
    let (span, total) = rest.split_once('/')?;
    let (start, end) = span.split_once('-')?;
    Some(ContentRange {
        start: start.trim().parse().ok()?,
        end_inclusive: end.trim().parse().ok()?,
        total: match total.trim() {
            "*" => None,
            t => Some(t.parse().ok()?),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range() {
        assert_eq!(
            parse_content_range("bytes 0-0/1234"),
            Some(ContentRange { start: 0, end_inclusive: 0, total: Some(1234) })
        );
        assert_eq!(parse_content_range("bytes 5-9/*").unwrap().total, None);
        assert_eq!(parse_content_range("bytes */0"), None);
        assert_eq!(parse_content_range("items 0-1/2"), None);
    }

    #[test]
    fn samples_spread_over_done_bytes() {
        assert!(samples(&[]).is_empty());
        assert_eq!(samples(&[(10, 20)]), vec![10..20]);
        let mb = 1024 * 1024;
        let s = samples(&[(0, mb), (3 * mb, 4 * mb)]);
        assert_eq!(s.len(), 4);
        assert_eq!(s[0], 0..SAMPLE_LEN);
        assert_eq!(s[3], 4 * mb - SAMPLE_LEN..4 * mb);
        assert!(s.iter().all(|r| r.end <= mb || r.start >= 3 * mb), "samples stay inside done ranges: {s:?}");
    }


    #[test]
    fn ranges_are_shared_out_before_anyone_starts() {
        let plan = Plan::new(vec![0..1000], 4, 100);
        let mut got: Vec<_> = (0..4).map(|w| plan.next(w, 100).unwrap()).collect();
        got.sort_by_key(|(r, _)| r.start);
        assert_eq!(got, vec![(0..250, None), (250..500, None), (500..750, None), (750..1000, None)]);
        // Small files get fewer connections, not slivers.
        let plan = Plan::new(vec![0..300], 4, 100);
        assert_eq!(plan.next(0, 100).unwrap().0.end - plan.left(0).start, 150);
    }

    #[test]
    fn a_free_worker_takes_half_of_the_busiest_range() {
        let plan = Plan::new(vec![0..1000], 2, 100);
        assert_eq!(plan.next(0, 100).unwrap().0, 0..500);
        assert_eq!(plan.next(1, 100).unwrap().0, 500..1000);
        assert_eq!(plan.claim(1, 500, 500), 500);
        // Worker 1 is done; it takes the second half of what worker 0 has left.
        assert_eq!(plan.next(1, 100), Some((250..500, Some((0, 0..250)))));
    }

    #[test]
    fn a_split_stops_the_worker_at_its_new_end() {
        let plan = Plan::new(vec![0..1000], 1, 100);
        plan.next(0, 100);
        assert_eq!(plan.claim(0, 0, 300), 300);
        let plan2 = &plan;
        // A second worker joins (as one that finished its own range would).
        plan2.0.lock().unwrap().slots.push(None);
        assert_eq!(plan.next(1, 100).unwrap().0, 650..1000);
        assert_eq!(plan.claim(0, 300, 400), 350, "only up to the new end");
        assert_eq!(plan.claim(0, 650, 10), 0);
        assert!(plan.left(0).is_empty());
    }

    #[test]
    fn ranges_too_small_to_split_are_left_alone() {
        let plan = Plan::new(vec![0..150], 2, 100);
        assert_eq!(plan.next(0, 100).unwrap().0, 0..150);
        assert_eq!(plan.next(1, 100), None, "75 bytes each would be under the floor");
    }

    #[test]
    fn a_range_handed_back_goes_to_a_worker_still_there() {
        let plan = Plan::new(vec![0..100, 100..1000], 2, 1000);
        assert_eq!(plan.next(0, 1000).unwrap().0, 100..1000, "the largest free range first");
        assert_eq!(plan.next(1, 1000).unwrap().0, 0..100);
        plan.claim(0, 100, 50);
        assert!(plan.hand_back(0));
        assert!(!plan.hand_back(1), "the last worker keeps its range");
        assert_eq!(plan.claim(1, 0, 100), 100);
        assert_eq!(plan.next(1, 1000).unwrap().0, 150..1000);
    }

    #[test]
    fn retry_delay_backs_off_and_caps() {
        let p = RetryPolicy { max_attempts: 9, base_delay: Duration::from_secs(1), max_delay: Duration::from_secs(10) };
        assert_eq!(p.delay(1), Duration::from_secs(1));
        assert_eq!(p.delay(2), Duration::from_secs(2));
        assert_eq!(p.delay(3), Duration::from_secs(4));
        assert_eq!(p.delay(5), Duration::from_secs(10));
        assert_eq!(p.delay(64), Duration::from_secs(10));
    }
}
