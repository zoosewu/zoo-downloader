//! zoo-core: the download core of zoo-downloader.
//!
//! No Tauri, no JS runtime. The Tauri app is a thin shell over this crate's interface:
//! `DownloadManager` plus the data types in `task`.
//! Design: docs/architecture/phase-2-download-core.md

pub mod category;
pub mod cipher;
pub mod connection;
pub mod error;
pub mod manager;
pub mod naming;
pub mod part_file;
pub mod proxy_pool;
pub mod rate;
pub mod resolve;
pub mod store;
pub mod task;
pub mod transfer;

pub use category::{Category, Classifier};
pub use error::CoreError;
pub use manager::DownloadManager;
pub use resolve::{Answers, Memory, Resolver, Source};
pub use task::{ActiveRange, AddRequest, AutoRetry, Config, Event, Phase, ProgressTick, SegmentMap, StartMode, Task, TaskId, TaskState, TaskUpdate};
