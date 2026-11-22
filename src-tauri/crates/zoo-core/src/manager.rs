//! DownloadManager: the single owner of every task.
//!
//! One actor task holds all state; the handle only sends it commands. Each running task has
//! a transfer job with its own cancellation token. A job that is stopping stays in `jobs`
//! until it reports back, so a task is never transferred by two jobs at once even if the
//! user pauses and resumes quickly.
//!
//! A job resolves the task's URL (see `resolve`) and then transfers; it reports phases and a
//! module-suggested file name back to the actor as it goes.
//!
//! Commands change the visible state immediately (pause shows Paused at once); the job's
//! final report only fills in what the command could not know (completion, failure).

use crate::connection::ConnectionSource;
use crate::error::CoreError;
use crate::naming;
use crate::part_file::PartFile;
use crate::rate::RateLimiter;
use crate::resolve::{self, Answers, Memory, Resolver};
use crate::store::JsonStore;
use crate::task::{AddRequest, Config, Event, Phase, ProgressTick, SegmentMap, StartMode, Task, TaskId, TaskState, TaskUpdate};
use crate::transfer::{self, Ctx, Progress, TransferSpec};
use chrono::Utc;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const TICK: Duration = Duration::from_millis(250);
const EVENT_BUFFER: usize = 1024;
/// Re-resolutions after the site's bandwidth limit (HTTP 509) in one run of a task.
const MAX_QUOTA_ROUNDS: u32 = 20;

#[derive(Clone)]
pub struct DownloadManager {
    tx: mpsc::UnboundedSender<Cmd>,
    events: broadcast::Sender<Event>,
}

type Reply<T> = oneshot::Sender<Result<T, CoreError>>;

enum Cmd {
    Add(AddRequest, Reply<Task>),
    Pause(TaskId, Reply<Task>),
    Resume(TaskId, Reply<Task>),
    Cancel(TaskId, Reply<Task>),
    Retry(TaskId, Reply<Task>),
    Restart(TaskId, Reply<Task>),
    Update(TaskId, TaskUpdate, Reply<Task>),
    Remove(TaskId, Reply<()>),
    MoveBefore(TaskId, Option<TaskId>, Reply<Task>),
    SetTags(TaskId, Vec<String>, Reply<Task>),
    SetNote(TaskId, String, Reply<Task>),
    PauseAll(oneshot::Sender<Vec<Task>>),
    ResumeAll(oneshot::Sender<Vec<Task>>),
    ClearFinished(oneshot::Sender<Vec<TaskId>>),
    List(oneshot::Sender<Vec<Task>>),
    Segments(TaskId, Reply<SegmentMap>),
    UpdateConfig(Config),
    SetConnection(Arc<dyn ConnectionSource>),
    Shutdown(oneshot::Sender<()>),
}

impl DownloadManager {
    /// Load tasks from `store_dir/tasks.json` and start the actor on the current Tokio runtime.
    /// Tasks that were queued or running when the app last stopped are queued again.
    /// Every URL is downloaded as is; see `start_with` for site-module resolution.
    pub fn start(cfg: Config, store_dir: &Path, conn: Arc<dyn ConnectionSource>) -> Result<Self, CoreError> {
        Self::start_with(cfg, store_dir, conn, Arc::new(resolve::Direct))
    }

    /// Like `start`, with `resolver` turning each task's URL into its download.
    pub fn start_with(
        cfg: Config,
        store_dir: &Path,
        conn: Arc<dyn ConnectionSource>,
        resolver: Arc<dyn Resolver>,
    ) -> Result<Self, CoreError> {
        let store = JsonStore::new(store_dir.join("tasks.json"));
        let mut tasks: Vec<Task> = store.load()?.unwrap_or_default();
        tasks.sort_by(|a, b| a.order.total_cmp(&b.order));
        // Saved before tasks had an order (all zero): keep the saved sequence.
        if tasks.windows(2).any(|w| w[0].order >= w[1].order) {
            renumber(&mut tasks);
        }
        let rev = tasks.iter().map(|t| t.rev).max().unwrap_or(0);
        for task in &mut tasks {
            task.tags.retain(|tag| cfg.categories.ids().any(|id| id == tag));
            task.speed = 0;
            task.phase = None;
            if matches!(task.state, TaskState::Running | TaskState::Queued) {
                task.state = TaskState::Queued;
            }
        }

        let (tx, cmd_rx) = mpsc::unbounded_channel();
        let (job_tx, job_rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let actor = Actor {
            rate: Arc::new(RateLimiter::new(cfg.speed_limit)),
            cfg,
            conn,
            resolver,
            tasks,
            jobs: HashMap::new(),
            store,
            events: events.clone(),
            job_tx,
            shutting_down: false,
            rev,
        };
        tokio::spawn(actor.run(cmd_rx, job_rx));
        Ok(Self { tx, events })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub async fn add(&self, req: AddRequest) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::Add(req, r)).await?
    }
    pub async fn pause(&self, id: TaskId) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::Pause(id, r)).await?
    }
    pub async fn resume(&self, id: TaskId) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::Resume(id, r)).await?
    }
    /// Stop and delete partial data.
    pub async fn cancel(&self, id: TaskId) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::Cancel(id, r)).await?
    }
    /// Start a failed or cancelled task again, resetting its retry count.
    pub async fn retry(&self, id: TaskId) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::Retry(id, r)).await?
    }
    /// Delete the task's partial data and download it again from the start.
    pub async fn restart(&self, id: TaskId) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::Restart(id, r)).await?
    }
    /// Change the URL or headers of a task that is not running (pause it first).
    pub async fn update(&self, id: TaskId, update: TaskUpdate) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::Update(id, update, r)).await?
    }
    /// Put a task in exactly these categories, in any state. Category rules stop changing it.
    pub async fn set_tags(&self, id: TaskId, tags: Vec<String>) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::SetTags(id, tags, r)).await?
    }

    /// The user's note on the task, in any state; empty removes it.
    pub async fn set_note(&self, id: TaskId, note: String) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::SetNote(id, note, r)).await?
    }
    /// Drop the task from the list. Partial data is deleted; a finished file is kept.
    pub async fn remove(&self, id: TaskId) -> Result<(), CoreError> {
        self.ask(|r| Cmd::Remove(id, r)).await?
    }
    /// Move a task in front of `before`, or to the end. Queued tasks start in list order.
    pub async fn move_before(&self, id: TaskId, before: Option<TaskId>) -> Result<Task, CoreError> {
        self.ask(|r| Cmd::MoveBefore(id, before, r)).await?
    }
    pub async fn pause_all(&self) -> Result<Vec<Task>, CoreError> {
        self.ask(Cmd::PauseAll).await
    }
    pub async fn resume_all(&self) -> Result<Vec<Task>, CoreError> {
        self.ask(Cmd::ResumeAll).await
    }
    /// Remove completed and cancelled tasks from the list.
    pub async fn clear_finished(&self) -> Result<Vec<TaskId>, CoreError> {
        self.ask(Cmd::ClearFinished).await
    }
    /// Done and in-flight byte ranges of a task, for the segment view.
    pub async fn segments(&self, id: TaskId) -> Result<SegmentMap, CoreError> {
        self.ask(|r| Cmd::Segments(id, r)).await?
    }
    pub async fn list(&self) -> Result<Vec<Task>, CoreError> {
        self.ask(Cmd::List).await
    }
    pub fn update_config(&self, cfg: Config) {
        let _ = self.tx.send(Cmd::UpdateConfig(cfg));
    }
    pub fn set_connection_source(&self, conn: Arc<dyn ConnectionSource>) {
        let _ = self.tx.send(Cmd::SetConnection(conn));
    }
    /// Stop all transfers, keeping their progress and state for the next start.
    pub async fn shutdown(&self) {
        let _ = self.ask(Cmd::Shutdown).await;
    }

    async fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Cmd) -> Result<T, CoreError> {
        let (reply, rx) = oneshot::channel();
        self.tx.send(make(reply)).map_err(|_| stopped())?;
        rx.await.map_err(|_| stopped())
    }
}

/// Number the tasks 1, 2, 3, ... in their current sequence. Returns the ids whose order changed.
fn renumber(tasks: &mut [Task]) -> Vec<TaskId> {
    let mut changed = Vec::new();
    for (i, t) in tasks.iter_mut().enumerate() {
        let order = (i + 1) as f64;
        if t.order != order {
            t.order = order;
            changed.push(t.id);
        }
    }
    changed
}

fn stopped() -> CoreError {
    CoreError::Invalid("download manager has stopped".into())
}

/// Why a job was told to stop; decides what its cancellation means.
#[derive(Clone, Copy, PartialEq)]
enum Stop {
    Pause,
    Cancel,
    Remove,
    Shutdown,
}

/// What a running job tells the actor.
enum JobMsg {
    Phase(TaskId, Phase),
    /// Resolution suggested a file name and the task had none of its own.
    Renamed(TaskId, String),
    /// What the user answered the site module, after each resolution.
    Answers(TaskId, Answers),
    Done(TaskId, Result<PathBuf, CoreError>),
}

struct Job {
    cancel: CancellationToken,
    progress: Arc<Progress>,
    part_path: PathBuf,
    stop: Option<Stop>,
    speed: SpeedMeter,
}

struct Actor {
    cfg: Config,
    conn: Arc<dyn ConnectionSource>,
    resolver: Arc<dyn Resolver>,
    rate: Arc<RateLimiter>,
    /// Sorted by `Task::order`, which is also the queue order.
    tasks: Vec<Task>,
    jobs: HashMap<TaskId, Job>,
    store: JsonStore<Vec<Task>>,
    events: broadcast::Sender<Event>,
    job_tx: mpsc::UnboundedSender<JobMsg>,
    shutting_down: bool,
    /// Last revision handed out; see `Task::rev`.
    rev: u64,
}

impl Actor {
    async fn run(mut self, mut cmd_rx: mpsc::UnboundedReceiver<Cmd>, mut job_rx: mpsc::UnboundedReceiver<JobMsg>) {
        self.wake_due();
        self.pump();
        self.save();
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let wake = self.next_wakeup();
            tokio::select! {
                cmd = cmd_rx.recv() => match cmd {
                    Some(Cmd::Shutdown(reply)) => {
                        self.shutdown(&mut job_rx).await;
                        let _ = reply.send(());
                        return;
                    }
                    Some(cmd) => self.handle(cmd),
                    // Every handle dropped: stop like a shutdown.
                    None => return self.shutdown(&mut job_rx).await,
                },
                Some(msg) = job_rx.recv() => match msg {
                    JobMsg::Done(id, result) => self.finished(id, result),
                    msg => self.job_update(msg),
                },
                // Progress is batched while something transfers; with nothing running the
                // actor sleeps until a command, a job message or a scheduled time.
                _ = tick.tick(), if !self.jobs.is_empty() => self.progress_tick(),
                _ = sleep_for(wake) => self.wake_due(),
            }
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Add(req, reply) => {
                let _ = reply.send(self.add(req));
            }
            Cmd::Pause(id, reply) => {
                let _ = reply.send(self.pause(id));
            }
            Cmd::Resume(id, reply) => {
                let _ = reply.send(self.resume(id));
            }
            Cmd::Cancel(id, reply) => {
                let _ = reply.send(self.cancel(id));
            }
            Cmd::Retry(id, reply) => {
                let _ = reply.send(self.retry(id));
            }
            Cmd::Restart(id, reply) => {
                let _ = reply.send(self.restart(id));
            }
            Cmd::Update(id, update, reply) => {
                let _ = reply.send(self.update(id, update));
            }
            Cmd::Remove(id, reply) => {
                let _ = reply.send(self.remove(id));
            }
            Cmd::MoveBefore(id, before, reply) => {
                let _ = reply.send(self.move_before(id, before));
            }
            Cmd::SetNote(id, note, reply) => {
                let _ = reply.send(self.set_note(id, &note));
            }
            Cmd::SetTags(id, tags, reply) => {
                let _ = reply.send(self.set_tags(id, tags));
            }
            Cmd::PauseAll(reply) => {
                let ids = self.ids_where(|t| matches!(t.state, TaskState::Running | TaskState::Queued | TaskState::Scheduled));
                let _ = reply.send(ids.into_iter().filter_map(|id| self.pause(id).ok()).collect());
            }
            Cmd::ResumeAll(reply) => {
                let ids = self.ids_where(|t| matches!(t.state, TaskState::Paused | TaskState::Failed));
                let _ = reply.send(ids.into_iter().filter_map(|id| self.resume(id).ok()).collect());
            }
            Cmd::ClearFinished(reply) => {
                let ids = self.ids_where(|t| t.state.is_finished());
                for id in &ids {
                    let _ = self.remove(*id);
                }
                let _ = reply.send(ids);
            }
            Cmd::Segments(id, reply) => {
                let _ = reply.send(self.segments(id));
                return; // read-only: nothing to save
            }
            Cmd::List(reply) => {
                let _ = reply.send(self.tasks.clone());
            }
            Cmd::UpdateConfig(cfg) => {
                self.rate.set_rate(cfg.speed_limit);
                self.cfg = cfg;
                self.drop_unknown_tags();
                self.pump();
            }
            Cmd::SetConnection(conn) => self.conn = conn,
            Cmd::Shutdown(_) => unreachable!("handled in run"),
        }
        self.save();
    }

    fn add(&mut self, req: AddRequest) -> Result<Task, CoreError> {
        check_url(&req.url)?;
        check_headers(&req.headers)?;
        if let Some(tags) = &req.tags {
            self.check_tags(tags)?;
        }
        let (state, scheduled_at) = match req.start {
            StartMode::Now => (TaskState::Queued, None),
            StartMode::Later => (TaskState::Paused, None),
            StartMode::At(at) => (TaskState::Scheduled, Some(at)),
        };
        self.rev += 1;
        let name = naming::filename_for(req.filename.as_deref(), &req.url);
        let task = Task {
            id: TaskId::new(),
            custom_tags: req.tags.is_some(),
            tags: req.tags.unwrap_or_else(|| self.cfg.categories.tags(&req.url, &name)),
            module: None,
            answers: Answers::from_saved(req.answers),
            name,
            custom_name: req.filename.is_some(),
            url: req.url,
            dir: req.dir,
            headers: req.headers,
            final_path: None,
            state,
            phase: None,
            error: None,
            downloaded: 0,
            total: None,
            speed: 0,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            scheduled_at,
            retry_count: 0,
            next_retry_at: None,
            rev: self.rev,
            order: self.tasks.last().map_or(1.0, |t| t.order + 1.0),
            note: tidy_note(&req.note),
            page_url: req.page_url.filter(|u| !u.trim().is_empty()),
        };
        self.tasks.push(task.clone());
        self.emit(Event::Upserted(task.clone()));
        self.pump();
        self.wake_due();
        self.snapshot(task.id)
    }

    fn check_tags(&self, tags: &[String]) -> Result<(), CoreError> {
        match tags.iter().find(|tag| !self.cfg.categories.ids().any(|id| id == tag.as_str())) {
            Some(unknown) => Err(CoreError::Invalid(format!("no category {unknown:?}"))),
            None => Ok(()),
        }
    }

    fn set_tags(&mut self, id: TaskId, mut tags: Vec<String>) -> Result<Task, CoreError> {
        self.check_tags(&tags)?;
        tags.dedup();
        self.set(id, |t| {
            t.tags = tags;
            t.custom_tags = true;
        })
    }

    fn set_note(&mut self, id: TaskId, note: &str) -> Result<Task, CoreError> {
        let note = tidy_note(note);
        self.set(id, |t| t.note = note)
    }

    /// After categories were deleted.
    fn drop_unknown_tags(&mut self) {
        let stale: Vec<TaskId> = self
            .tasks
            .iter()
            .filter(|t| t.tags.iter().any(|tag| !self.cfg.categories.ids().any(|id| id == tag)))
            .map(|t| t.id)
            .collect();
        for id in stale {
            let categories = self.cfg.categories.clone();
            let _ = self.set(id, |t| t.tags.retain(|tag| categories.ids().any(|id| id == tag)));
        }
    }

    fn move_before(&mut self, id: TaskId, before: Option<TaskId>) -> Result<Task, CoreError> {
        let from = self.tasks.iter().position(|t| t.id == id).ok_or(CoreError::NotFound)?;
        if before == Some(id) {
            return self.snapshot(id);
        }
        let task = self.tasks.remove(from);
        let to = match before {
            Some(b) => match self.tasks.iter().position(|t| t.id == b) {
                Some(i) => i,
                None => {
                    self.tasks.insert(from, task);
                    return Err(CoreError::NotFound);
                }
            },
            None => self.tasks.len(),
        };
        let prev = to.checked_sub(1).map(|i| self.tasks[i].order);
        let next = self.tasks.get(to).map(|t| t.order);
        self.tasks.insert(to, task);
        let order = match (prev, next) {
            (Some(p), Some(n)) => p + (n - p) / 2.0,
            (Some(p), None) => p + 1.0,
            (None, Some(n)) => n - 1.0,
            (None, None) => 1.0,
        };
        if prev.is_some_and(|p| order <= p) || next.is_some_and(|n| order >= n) {
            // Out of room between the neighbours: space everything out again.
            for changed in renumber(&mut self.tasks).into_iter().filter(|&c| c != id) {
                let _ = self.set(changed, |_| {});
            }
            return self.set(id, |_| {});
        }
        self.set(id, |t| t.order = order)
    }

    fn pause(&mut self, id: TaskId) -> Result<Task, CoreError> {
        let state = self.task(id)?.state;
        match state {
            TaskState::Running => self.stop_job(id, Stop::Pause),
            TaskState::Queued | TaskState::Scheduled | TaskState::Failed => {}
            TaskState::Paused => return self.snapshot(id),
            s @ (TaskState::Completed | TaskState::Cancelled) => return Err(invalid_transition(s, "pause")),
        }
        self.set(id, |t| {
            t.state = TaskState::Paused;
            t.phase = None;
            t.next_retry_at = None;
            t.speed = 0;
        })
    }

    fn resume(&mut self, id: TaskId) -> Result<Task, CoreError> {
        let state = self.task(id)?.state;
        match state {
            TaskState::Paused | TaskState::Scheduled => self.set(id, |t| t.state = TaskState::Queued)?,
            TaskState::Failed => return self.retry(id),
            TaskState::Queued | TaskState::Running => return self.snapshot(id),
            s @ (TaskState::Completed | TaskState::Cancelled) => return Err(invalid_transition(s, "resume")),
        };
        self.pump();
        self.snapshot(id)
    }

    fn retry(&mut self, id: TaskId) -> Result<Task, CoreError> {
        let state = self.task(id)?.state;
        match state {
            TaskState::Failed | TaskState::Cancelled => {}
            s => return Err(invalid_transition(s, "retry")),
        }
        self.set(id, |t| {
            t.state = TaskState::Queued;
            t.error = None;
            t.retry_count = 0;
            t.next_retry_at = None;
            t.finished_at = None;
        })?;
        self.pump();
        self.snapshot(id)
    }

    fn restart(&mut self, id: TaskId) -> Result<Task, CoreError> {
        let (state, part) = self.task(id).map(|t| (t.state, part_path_of(t)))?;
        match state {
            TaskState::Running | TaskState::Completed => return Err(invalid_transition(state, "restart")),
            // A previous job still stopping deletes the part when it reports back.
            _ if self.jobs.contains_key(&id) => self.stop_job(id, Stop::Cancel),
            _ => discard_part(&part),
        }
        self.set(id, |t| {
            t.state = TaskState::Queued;
            t.phase = None;
            t.error = None;
            t.downloaded = 0;
            t.total = None;
            t.retry_count = 0;
            t.next_retry_at = None;
            t.scheduled_at = None;
            t.finished_at = None;
        })?;
        self.pump();
        self.snapshot(id)
    }

    fn update(&mut self, id: TaskId, update: TaskUpdate) -> Result<Task, CoreError> {
        let state = self.task(id)?.state;
        if matches!(state, TaskState::Running | TaskState::Completed | TaskState::Cancelled) {
            return Err(invalid_transition(state, "update"));
        }
        if let Some(url) = &update.url {
            check_url(url)?;
        }
        if let Some(headers) = &update.headers {
            check_headers(headers)?;
        }
        self.set(id, |t| {
            if let Some(url) = update.url {
                // Answers belong to the old link (a new share has its own code).
                if url != t.url {
                    t.answers = Answers::default();
                }
                t.url = url;
            }
            if let Some(headers) = update.headers {
                t.headers = headers;
            }
        })
    }

    fn cancel(&mut self, id: TaskId) -> Result<Task, CoreError> {
        let (state, part) = self.task(id).map(|t| (t.state, part_path_of(t)))?;
        match state {
            TaskState::Running => self.stop_job(id, Stop::Cancel),
            TaskState::Completed | TaskState::Cancelled => return Err(invalid_transition(state, "cancel")),
            // A previous job may still be stopping (e.g. paused then cancelled at once).
            _ if self.jobs.contains_key(&id) => self.stop_job(id, Stop::Cancel),
            _ => discard_part(&part),
        }
        self.set(id, |t| {
            t.state = TaskState::Cancelled;
            t.phase = None;
            t.downloaded = 0;
            t.speed = 0;
            t.next_retry_at = None;
            t.finished_at = Some(Utc::now());
        })
    }

    fn remove(&mut self, id: TaskId) -> Result<(), CoreError> {
        let (state, part) = self.task(id).map(|t| (t.state, part_path_of(t)))?;
        match state {
            TaskState::Completed => {}
            // Running, or a previous job still stopping: its report deletes the part.
            _ if self.jobs.contains_key(&id) => self.stop_job(id, Stop::Remove),
            _ => discard_part(&part),
        }
        self.tasks.retain(|t| t.id != id);
        self.emit(Event::Removed(id));
        Ok(())
    }

    fn segments(&self, id: TaskId) -> Result<SegmentMap, CoreError> {
        let task = self.task(id)?;
        let mut map = SegmentMap {
            total: task.total,
            done: Vec::new(),
            active: Vec::new(),
        };
        if let Some(job) = self.jobs.get(&id) {
            map.total = job.progress.total().or(task.total);
            match job.progress.segments() {
                Some((done, active)) => (map.done, map.active) = (done, active),
                // Stream mode fills the file front to back.
                None => map.done = Some((0, job.progress.downloaded())).filter(|&(_, end)| end > 0).into_iter().collect(),
            }
        } else if task.state == TaskState::Completed {
            map.done = task.total.map(|t| (0, t)).into_iter().collect();
        } else if let Some((size, done)) = PartFile::read_record(&part_path_of(task)) {
            map.total = size.or(task.total);
            map.done = done;
        }
        Ok(map)
    }

    fn stop_job(&mut self, id: TaskId, stop: Stop) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.stop = Some(stop);
            job.cancel.cancel();
        }
    }

    /// Start queued tasks while there are free slots.
    fn pump(&mut self) {
        if self.shutting_down {
            return;
        }
        loop {
            if self.cfg.max_concurrent.is_some_and(|max| self.jobs.len() >= max) {
                return;
            }
            let next = self
                .tasks
                .iter()
                .find(|t| t.state == TaskState::Queued && !self.jobs.contains_key(&t.id))
                .map(|t| t.id);
            let Some(id) = next else { return };
            self.start_job(id);
        }
    }

    fn start_job(&mut self, id: TaskId) {
        let task = self.tasks.iter().find(|t| t.id == id).expect("queued task exists");
        // Nothing fetched yet and no name from the user: resolution may name the file.
        let may_rename = !task.custom_name && task.downloaded == 0;
        let spec = TransferSpec {
            origin: task.url.clone(),
            url: task.url.clone(),
            headers: with_user_agent(task.headers.clone(), self.cfg.user_agent.as_deref()),
            dir: task.dir.clone(),
            name: task.name.clone(),
            part_path: part_path_of(task),
            connections: self.cfg.connections_per_task,
            min_split: self.cfg.min_split,
            max_request: None,
            retry: self.cfg.transfer_retry,
            decrypt: None,
        };
        let ctx = Ctx {
            conn: self.conn.clone(),
            rate: self.rate.clone(),
            progress: Arc::new(Progress::starting_at(task.downloaded, task.total)),
            cancel: CancellationToken::new(),
        };
        self.jobs.insert(
            id,
            Job {
                cancel: ctx.cancel.clone(),
                progress: ctx.progress.clone(),
                part_path: spec.part_path.clone(),
                stop: None,
                speed: SpeedMeter::default(),
            },
        );
        let (msgs, resolver) = (self.job_tx.clone(), self.resolver.clone());
        let memory: Memory = Arc::new(std::sync::Mutex::new(task.answers.clone()));
        tokio::spawn(async move {
            let result = run_job(id, spec, &ctx, resolver.as_ref(), &memory, may_rename, &msgs).await;
            let _ = msgs.send(JobMsg::Done(id, result));
        });
        let _ = self.set(id, |t| {
            t.state = TaskState::Running;
            t.error = None;
            t.started_at.get_or_insert_with(Utc::now);
        });
    }

    fn job_update(&mut self, msg: JobMsg) {
        match msg {
            // A stopping job's phase is no longer what the user sees.
            JobMsg::Phase(id, phase) if self.jobs.get(&id).is_some_and(|j| j.stop.is_none()) => {
                let _ = self.set(id, |t| {
                    if let Phase::Resolving { module: Some(module) } = &phase {
                        t.module = Some(module.clone());
                    }
                    t.phase = Some(phase);
                });
            }
            JobMsg::Answers(id, answers) => {
                if self.tasks.iter().any(|t| t.id == id && t.answers != answers) {
                    let _ = self.set(id, |t| t.answers = answers);
                }
            }
            JobMsg::Renamed(id, name) => {
                let dir = self.tasks.iter().find(|t| t.id == id).map(|t| t.dir.clone());
                if let (Some(job), Some(dir)) = (self.jobs.get_mut(&id), dir) {
                    job.part_path = naming::part_path(&dir, &name, &id.to_string());
                }
                let categories = &self.cfg.categories;
                let tags = self.tasks.iter().find(|t| t.id == id && !t.custom_tags).map(|t| categories.tags(&t.url, &name));
                let _ = self.set(id, |t| {
                    t.name = name;
                    if let Some(tags) = tags {
                        t.tags = tags;
                    }
                });
            }
            _ => {}
        }
    }

    fn finished(&mut self, id: TaskId, result: Result<PathBuf, CoreError>) {
        let Some(job) = self.jobs.remove(&id) else { return };
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.phase = None; // announced with the state change below
        }
        let still_listed = self.tasks.iter().any(|t| t.id == id);

        match (&result, job.stop) {
            (Ok(_), _) if still_listed => {}
            // Removed while running: nothing left to update, but its partial data must go.
            (_, Some(Stop::Remove)) | (Err(_), Some(Stop::Cancel)) => discard_part(&job.part_path),
            _ => {}
        }

        if still_listed {
            let downloaded = job.progress.downloaded();
            let total = job.progress.total();
            match result {
                Ok(path) => {
                    let _ = self.set(id, |t| {
                        t.state = TaskState::Completed;
                        t.final_path = Some(path);
                        t.total = total.or(Some(downloaded));
                        t.downloaded = downloaded;
                        t.speed = 0;
                        t.error = None;
                        t.finished_at = Some(Utc::now());
                    });
                }
                // The stop command already set the visible state.
                Err(CoreError::Cancelled) => {
                    if job.stop != Some(Stop::Cancel) {
                        let _ = self.set(id, |t| {
                            t.downloaded = downloaded;
                            t.total = total.or(t.total);
                        });
                    }
                }
                Err(e) => self.fail(id, e, downloaded, total),
            }
        }
        self.save();
        self.pump();
    }

    fn fail(&mut self, id: TaskId, error: CoreError, downloaded: u64, total: Option<u64>) {
        let auto = self.cfg.auto_retry;
        let _ = self.set(id, |t| {
            t.state = TaskState::Failed;
            t.downloaded = downloaded;
            t.total = total.or(t.total);
            t.speed = 0;
            t.finished_at = Some(Utc::now());
            t.next_retry_at = None;
            if auto.enabled && error.is_retryable() && t.retry_count < auto.max_attempts {
                t.retry_count += 1;
                let factor = 1u32.checked_shl(t.retry_count - 1).unwrap_or(u32::MAX);
                let delay = auto.interval.saturating_mul(factor);
                t.next_retry_at = chrono::Duration::from_std(delay).ok().map(|d| Utc::now() + d);
            }
            t.error = Some(error);
        });
    }

    /// Move scheduled and retry-pending tasks whose time has come into the queue.
    fn wake_due(&mut self) {
        let now = Utc::now();
        let due = self.ids_where(|t| match t.state {
            TaskState::Scheduled => t.scheduled_at.is_none_or(|at| at <= now),
            TaskState::Failed => t.next_retry_at.is_some_and(|at| at <= now),
            _ => false,
        });
        for id in due {
            let _ = self.set(id, |t| {
                t.state = TaskState::Queued;
                t.next_retry_at = None;
            });
        }
        self.pump();
    }

    fn next_wakeup(&self) -> Option<Duration> {
        let now = Utc::now();
        self.tasks
            .iter()
            .filter_map(|t| match t.state {
                TaskState::Scheduled => t.scheduled_at,
                TaskState::Failed => t.next_retry_at,
                _ => None,
            })
            .min()
            .map(|at| (at - now).to_std().unwrap_or(Duration::ZERO))
    }

    fn progress_tick(&mut self) {
        let mut ticks = Vec::new();
        for (id, job) in &mut self.jobs {
            if job.stop.is_some() {
                continue;
            }
            let downloaded = job.progress.downloaded();
            let total = job.progress.total();
            let speed = job.speed.sample(downloaded);
            if let Some(task) = self.tasks.iter_mut().find(|t| t.id == *id) {
                // A stalled transfer (waiting, resolving, a slow server) sends nothing new.
                if (task.downloaded, task.total, task.speed) == (downloaded, total, speed) {
                    continue;
                }
                task.downloaded = downloaded;
                task.total = total;
                task.speed = speed;
                ticks.push(ProgressTick { id: *id, downloaded, total, speed });
            }
        }
        if !ticks.is_empty() {
            self.emit(Event::Progress(ticks));
        }
    }

    async fn shutdown(&mut self, job_rx: &mut mpsc::UnboundedReceiver<JobMsg>) {
        self.shutting_down = true;
        for job in self.jobs.values_mut() {
            job.stop.get_or_insert(Stop::Shutdown);
            job.cancel.cancel();
        }
        while !self.jobs.is_empty() {
            match job_rx.recv().await {
                Some(JobMsg::Done(id, result)) => self.finished_on_shutdown(id, result),
                Some(msg) => self.job_update(msg),
                None => break,
            }
        }
        self.save();
    }

    /// Like `finished`, but a cancelled transfer keeps its Running state so the next start resumes it.
    fn finished_on_shutdown(&mut self, id: TaskId, result: Result<PathBuf, CoreError>) {
        match &result {
            Err(CoreError::Cancelled) if self.jobs.get(&id).is_some_and(|j| j.stop == Some(Stop::Shutdown)) => {
                let job = self.jobs.remove(&id).unwrap();
                let (downloaded, total) = (job.progress.downloaded(), job.progress.total());
                let _ = self.set(id, |t| {
                    t.downloaded = downloaded;
                    t.total = total.or(t.total);
                    t.speed = 0;
                });
            }
            _ => self.finished(id, result),
        }
    }

    // --- helpers -------------------------------------------------------------------------

    fn task(&self, id: TaskId) -> Result<&Task, CoreError> {
        self.tasks.iter().find(|t| t.id == id).ok_or(CoreError::NotFound)
    }

    fn snapshot(&self, id: TaskId) -> Result<Task, CoreError> {
        self.task(id).cloned()
    }

    fn ids_where(&self, pred: impl Fn(&Task) -> bool) -> Vec<TaskId> {
        self.tasks.iter().filter(|t| pred(t)).map(|t| t.id).collect()
    }

    /// Mutate a task and announce the change.
    fn set(&mut self, id: TaskId, f: impl FnOnce(&mut Task)) -> Result<Task, CoreError> {
        let task = self.tasks.iter_mut().find(|t| t.id == id).ok_or(CoreError::NotFound)?;
        let was_finished = finished(task);
        f(task);
        self.rev += 1;
        task.rev = self.rev;
        let task = task.clone();
        self.emit(Event::Upserted(task.clone()));
        if !was_finished && finished(&task) {
            self.emit(Event::Finished(task.clone()));
        }
        Ok(task)
    }

    fn emit(&self, event: Event) {
        // No subscribers is fine.
        let _ = self.events.send(event);
    }

    fn save(&self) {
        if let Err(e) = self.store.save(&self.tasks) {
            log::error!("failed to save {}: {e}", self.store.path().display());
        }
    }
}

/// Resolve, then transfer. A resolved link the server refuses as expired or forbidden is
/// resolved again once: share sites hand out short-lived links.
async fn run_job(
    id: TaskId,
    mut spec: TransferSpec,
    ctx: &Ctx,
    resolver: &dyn Resolver,
    memory: &Memory,
    may_rename: bool,
    msgs: &mpsc::UnboundedSender<JobMsg>,
) -> Result<PathBuf, CoreError> {
    // `first`: only the first resolution may name the file.
    let (mut first, mut resolved_again, mut over_quota) = (true, false, 0);
    let (connections, own_headers) = (spec.connections, std::mem::take(&mut spec.headers));
    loop {
        let report: resolve::PhaseReport = {
            let msgs = msgs.clone();
            Box::new(move |phase| {
                let _ = msgs.send(JobMsg::Phase(id, phase));
            })
        };
        let resolved = tokio::select! {
            _ = ctx.cancel.cancelled() => Err(CoreError::Cancelled),
            source = resolver.resolve(&spec.origin, memory.clone(), report) => source,
        };
        // Kept whatever happened: a code the user typed before a later step failed still counts.
        let _ = msgs.send(JobMsg::Answers(id, memory.lock().unwrap().clone()));
        let source = resolved?;
        // The module's name, else the server's (Content-Disposition), else the URL's.
        let may_name = may_rename && std::mem::take(&mut first);
        if let Some(suggested) = source.filename.as_deref().filter(|_| may_name) {
            rename(&mut spec, id, suggested, msgs);
        }
        let server_may_name = may_name && source.filename.is_none();
        spec.url = source.url;
        spec.headers = merge_headers(source.headers, &own_headers);
        spec.connections = source.max_connections.map_or(connections, |max| connections.min(max.max(1)));
        spec.max_request = source.max_segment_size;
        spec.decrypt = source.decrypt;
        let _ = msgs.send(JobMsg::Phase(id, Phase::Transferring));
        let result = match transfer::probe_source(&spec, ctx).await {
            Ok(probed) => {
                if let Some(suggested) = probed.filename.as_deref().filter(|_| server_may_name) {
                    rename(&mut spec, id, suggested, msgs);
                }
                transfer::fetch(&spec, ctx, probed).await
            }
            Err(e) => Err(e),
        };
        match result {
            Err(CoreError::Http(401 | 403 | 410)) if source.expires && !resolved_again => resolved_again = true,
            // Bandwidth Limit Exceeded (MEGA's transfer quota): the module waits until the site
            // serves again, then the transfer resumes where it stopped.
            Err(CoreError::Http(509)) if source.expires && over_quota < MAX_QUOTA_ROUNDS => {
                over_quota += 1;
            }
            other => return other,
        }
    }
}

/// Give a task that has no data yet a better name; the actor updates the task to match.
fn rename(spec: &mut TransferSpec, id: TaskId, suggested: &str, msgs: &mpsc::UnboundedSender<JobMsg>) {
    let name = naming::filename_for(Some(suggested), &spec.origin);
    if name != spec.name {
        spec.part_path = naming::part_path(&spec.dir, &name, &id.to_string());
        spec.name = name.clone();
        let _ = msgs.send(JobMsg::Renamed(id, name));
    }
}

/// A note as kept: without surrounding blank space.
fn tidy_note(note: &str) -> String {
    note.trim().to_string()
}

fn check_url(url: &str) -> Result<(), CoreError> {
    let parsed = url::Url::parse(url).map_err(|e| CoreError::Invalid(format!("invalid URL: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(CoreError::Invalid("URL must be http(s) with a host".into()));
    }
    Ok(())
}

/// Headers the transfer sets itself.
const RESERVED_HEADERS: [&str; 3] = ["range", "host", "content-length"];

fn check_headers(headers: &[(String, String)]) -> Result<(), CoreError> {
    for (name, value) in headers {
        let header = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| CoreError::Invalid(format!("invalid header name {name:?}")))?;
        if RESERVED_HEADERS.contains(&header.as_str()) {
            return Err(CoreError::Invalid(format!("the {name} header is set by the download itself")));
        }
        reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| CoreError::Invalid(format!("invalid value for header {name}")))?;
    }
    Ok(())
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
}

/// The configured User-Agent, unless the task sets its own.
fn with_user_agent(mut headers: Vec<(String, String)>, user_agent: Option<&str>) -> Vec<(String, String)> {
    if let Some(ua) = user_agent.filter(|_| !has_header(&headers, "user-agent")) {
        headers.push(("User-Agent".into(), ua.into()));
    }
    headers
}

/// `resolved` (from the site module) wins over the task's own headers of the same name.
fn merge_headers(mut resolved: Vec<(String, String)>, own: &[(String, String)]) -> Vec<(String, String)> {
    let missing: Vec<_> = own.iter().filter(|(name, _)| !has_header(&resolved, name)).cloned().collect();
    resolved.extend(missing);
    resolved
}

/// Done for good: completed, or failed with no automatic retry coming (see `Event::Finished`).
fn finished(task: &Task) -> bool {
    task.state == TaskState::Completed || (task.state == TaskState::Failed && task.next_retry_at.is_none())
}

fn part_path_of(task: &Task) -> PathBuf {
    naming::part_path(&task.dir, &task.name, &task.id.to_string())
}

fn discard_part(path: &Path) {
    if let Err(e) = PartFile::discard(path) {
        log::warn!("failed to delete {}: {e}", path.display());
    }
}

fn invalid_transition(state: TaskState, action: &str) -> CoreError {
    CoreError::Invalid(format!("cannot {action} a {state:?} task"))
}

async fn sleep_for(wait: Option<Duration>) {
    match wait {
        Some(d) => tokio::time::sleep(d).await,
        None => std::future::pending().await,
    }
}

/// Bytes per second over the last `SPEED_WINDOW`: the slope of the straight line that best fits
/// the (time, downloaded) samples (least squares). Every sample counts, so a burst, or one
/// leaving the window, barely moves it, and a stalled transfer falls smoothly to 0.
#[derive(Default)]
struct SpeedMeter {
    samples: VecDeque<(Instant, u64)>,
}

const SPEED_WINDOW: Duration = Duration::from_secs(5);

impl SpeedMeter {
    fn sample(&mut self, downloaded: u64) -> u64 {
        self.sample_at(Instant::now(), downloaded)
    }

    fn sample_at(&mut self, now: Instant, downloaded: u64) -> u64 {
        // Fewer bytes than before: the transfer started over.
        if self.samples.back().is_some_and(|&(_, bytes)| downloaded < bytes) {
            self.samples.clear();
        }
        self.samples.push_back((now, downloaded));
        while self.samples.front().is_some_and(|&(at, _)| now.duration_since(at) > SPEED_WINDOW) {
            self.samples.pop_front();
        }
        let Some(&(t0, b0)) = self.samples.front() else { return 0 };
        let points: Vec<(f64, f64)> =
            self.samples.iter().map(|&(t, b)| (t.duration_since(t0).as_secs_f64(), (b - b0) as f64)).collect();
        let n = points.len() as f64;
        let (mx, my) = (points.iter().map(|p| p.0).sum::<f64>() / n, points.iter().map(|p| p.1).sum::<f64>() / n);
        let spread: f64 = points.iter().map(|p| (p.0 - mx).powi(2)).sum();
        if spread <= 0.0 {
            return 0;
        }
        let slope = points.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>() / spread;
        slope.max(0.0).round() as u64
    }
}

#[cfg(test)]
mod speed_tests {
    use super::*;

    /// `rate(i)` bytes in tick `i`, ticks 250 ms apart; the speed after each.
    fn run(ticks: usize, rate: impl Fn(usize) -> u64) -> Vec<u64> {
        let (mut meter, start, mut bytes) = (SpeedMeter::default(), Instant::now(), 0);
        (0..ticks)
            .map(|i| {
                bytes += rate(i);
                meter.sample_at(start + Duration::from_millis(250 * i as u64), bytes)
            })
            .collect()
    }

    #[test]
    fn a_steady_transfer_reads_its_rate() {
        // 250 KB per 250 ms = 1 MB/s.
        assert!(run(40, |_| 250_000)[1..].iter().all(|&s| s == 1_000_000));
    }

    #[test]
    fn bursts_barely_move_it() {
        // 1 MB/s, but the bytes arrive in bursts every fourth tick.
        let speeds = run(60, |i| if i % 4 == 0 { 1_000_000 } else { 0 });
        let settled = &speeds[25..];
        let (lo, hi) = (*settled.iter().min().unwrap(), *settled.iter().max().unwrap());
        assert!(lo > 900_000 && hi < 1_100_000, "{lo}..{hi}");
    }

    #[test]
    fn a_stall_falls_to_zero_within_the_window() {
        let speeds = run(60, |i| if i < 30 { 250_000 } else { 0 });
        assert!(speeds[33] > 0 && speeds[33] < 1_000_000, "falls gradually: {}", speeds[33]);
        assert!(speeds[30..].windows(2).all(|w| w[1] <= w[0]), "never jumps back up");
        assert_eq!(speeds[30 + 21], 0, "flat for a whole window: 0");
    }

    #[test]
    fn starting_over_forgets_the_old_count() {
        let (mut meter, t) = (SpeedMeter::default(), Instant::now());
        meter.sample_at(t, 10_000_000);
        meter.sample_at(t + Duration::from_millis(250), 10_250_000);
        assert_eq!(meter.sample_at(t + Duration::from_millis(500), 0), 0);
    }
}
