//! Data that crosses the zoo-core interface: tasks, events, requests, configuration.

use crate::category::Classifier;
use crate::error::CoreError;
use crate::resolve::Answers;
use crate::transfer::RetryPolicy;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct TaskId(Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TaskId {
    type Err = CoreError;
    fn from_str(s: &str) -> Result<Self, CoreError> {
        Uuid::parse_str(s).map(Self).map_err(|_| CoreError::Invalid(format!("not a task id: {s}")))
    }
}

/// ```text
/// Scheduled ─time─▶ Queued ─slot─▶ Running ──▶ Completed
///                    ▲  ▲             │
///             resume │  └─auto retry─ Failed
///                  Paused ◀─pause─────┘   (Queued / Scheduled / Failed can pause too)
/// any unfinished ─cancel─▶ Cancelled
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Scheduled,
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn is_finished(self) -> bool {
        matches!(self, TaskState::Completed | TaskState::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: TaskId,
    pub url: String,
    /// Sanitized file name the download will be saved as (before conflict renaming).
    pub name: String,
    /// The user chose `name`; a site module's suggested name does not replace it.
    #[serde(default)]
    pub custom_name: bool,
    pub dir: PathBuf,
    /// Sent with every request of the download (User-Agent, Referer, Cookie, ...). Headers a
    /// site module returns take precedence.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Where the finished file actually landed (may differ from `dir/name` after renaming).
    pub final_path: Option<PathBuf>,
    pub state: TaskState,
    /// What a running task is doing; `None` otherwise.
    #[serde(default)]
    pub phase: Option<Phase>,
    pub error: Option<CoreError>,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Bytes per second; zero unless running.
    #[serde(default)]
    pub speed: u64,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub retry_count: u32,
    /// When a failed task will be re-queued automatically.
    pub next_retry_at: Option<DateTime<Utc>>,
    /// Increases with every change; of two copies of a task, the higher `rev` is newer.
    #[serde(default)]
    pub rev: u64,
    /// Place in the list and the queue: queued tasks start lowest first. Unique per task.
    #[serde(default)]
    pub order: f64,
    /// Ids of the user categories the task is in (see `category`).
    #[serde(default)]
    pub tags: Vec<String>,
    /// The user set `tags` by hand; category rules no longer change them.
    #[serde(default)]
    pub custom_tags: bool,
    /// The site module that resolved the URL (`id@marketplace`); `None` for a direct download.
    #[serde(default)]
    pub module: Option<String>,
    /// What the user answered the module for this download (see `resolve::Answers`).
    #[serde(default, skip_serializing_if = "Answers::is_empty")]
    #[ts(skip)]
    pub answers: Answers,
    /// The user's note; empty for none.
    #[serde(default)]
    pub note: String,
    /// The page the link came from (the browser tab that started the download).
    #[serde(default)]
    pub page_url: Option<String>,
}

/// Steps of a running task. Resolution happens before every transfer; only site modules
/// report `Resolving`, `Waiting` and `AwaitingUser`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Phase {
    /// A site module is turning the page URL into the download (`module` is `id@marketplace`).
    Resolving { module: Option<String> },
    /// The site makes the user wait, e.g. a free-download countdown.
    Waiting { until: DateTime<Utc>, reason: Option<String> },
    /// A captcha needs the user.
    AwaitingUser,
    Transferring,
}

/// Which bytes of a task are done and which are being fetched; byte ranges are `[start, end)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SegmentMap {
    pub total: Option<u64>,
    pub done: Vec<(u64, u64)>,
    pub active: Vec<ActiveRange>,
}

/// A range one connection is fetching right now.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRange {
    pub start: u64,
    pub end: u64,
    /// The last attempt failed and the connection is waiting to retry.
    pub retrying: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProgressTick {
    pub id: TaskId,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Event {
    /// A task was added or changed state.
    Upserted(Task),
    Removed(TaskId),
    /// Counters of running tasks, batched a few times per second.
    Progress(Vec<ProgressTick>),
    /// The task just finished: it completed, or failed with no automatic retry left. Sent once
    /// per such change, after its `Upserted`; later edits of a finished task (categories, a
    /// rename) do not send it again.
    Finished(Task),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "mode", content = "at", rename_all = "camelCase")]
pub enum StartMode {
    /// Queue now; starts as soon as a slot is free.
    Now,
    /// Add paused; the user starts it later.
    Later,
    At(DateTime<Utc>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AddRequest {
    pub url: String,
    pub filename: Option<String>,
    pub dir: PathBuf,
    pub start: StartMode,
    /// See `Task::headers`.
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Categories the user chose; `None` lets the category rules decide.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Answers the module already got for this URL (from listing its share), so it does not ask again.
    #[serde(default)]
    pub answers: BTreeMap<String, String>,
    /// See `Task::note`.
    #[serde(default)]
    pub note: String,
    /// See `Task::page_url`.
    #[serde(default)]
    pub page_url: Option<String>,
}

/// Changes to a task that is not running. Missing fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdate {
    /// A new URL for the same file (a mirror, a fresh share link). Progress is kept only if
    /// the new URL serves the same bytes; otherwise the task fails with `SourceChanged`.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Option<Vec<(String, String)>>,
}

/// The engine's slice of the app settings. Applied immediately by `update_config`.
#[derive(Debug, Clone)]
pub struct Config {
    /// `None` means unlimited.
    pub max_concurrent: Option<usize>,
    pub connections_per_task: usize,
    /// The least a connection's range is split into (see `transfer::run_workers`).
    pub min_split: u64,
    /// Bytes per second across all downloads; `None` means unlimited.
    pub speed_limit: Option<u64>,
    pub auto_retry: AutoRetry,
    /// Per-request retries inside a transfer.
    pub transfer_retry: RetryPolicy,
    /// User-Agent for requests that do not set one; `None` means zoo-downloader's own.
    pub user_agent: Option<String>,
    /// Tags new tasks; tags of categories that no longer exist are dropped.
    pub categories: Classifier,
}

#[derive(Debug, Clone, Copy)]
pub struct AutoRetry {
    pub enabled: bool,
    pub max_attempts: u32,
    /// Delay before the first automatic retry; doubles each time.
    pub interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_concurrent: Some(3),
            connections_per_task: 8,
            min_split: 4 * 1024 * 1024,
            speed_limit: None,
            auto_retry: AutoRetry { enabled: true, max_attempts: 3, interval: Duration::from_secs(30) },
            transfer_retry: RetryPolicy::default(),
            user_agent: None,
            categories: Classifier::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shapes() {
        let id = TaskId::new();
        assert_eq!(serde_json::to_value(id).unwrap(), serde_json::Value::String(id.to_string()));
        assert_eq!(serde_json::from_value::<TaskId>(serde_json::json!(id.to_string())).unwrap(), id);
        assert_eq!(
            serde_json::to_value(StartMode::Now).unwrap(),
            serde_json::json!({ "mode": "now" })
        );
        assert_eq!(
            serde_json::to_value(Event::Removed(id)).unwrap(),
            serde_json::json!({ "type": "removed", "data": id.to_string() })
        );
    }
}
