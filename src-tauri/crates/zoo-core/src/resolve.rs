//! Where the bytes of a task actually come from.
//!
//! A task's `url` is what the user gave: often a share page, not the file. Before every
//! transfer a `Resolver` turns it into a `Source`: the direct URL plus the headers it needs.
//! Adapters: `Direct` (the URL is the file) and the site-module resolver in zoo-module-client.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::cipher::Decrypt;
use crate::error::CoreError;
use crate::task::Phase;

/// The direct download a task's URL resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub url: String,
    /// Sent with every request of the transfer (e.g. Referer, Cookie).
    pub headers: Vec<(String, String)>,
    /// Suggested file name; used only if the user did not choose one.
    pub filename: Option<String>,
    /// Upper bound on parallel connections (some sites throttle or ban more).
    pub max_connections: Option<usize>,
    /// Upper bound on the bytes one request may ask for (some sites refuse larger ranges).
    pub max_segment_size: Option<u64>,
    /// Whether the link was produced by resolution (and so may expire); a direct link
    /// is never re-resolved.
    pub expires: bool,
    /// The server sends the file encrypted (MEGA); the transfer decrypts it.
    pub decrypt: Option<Decrypt>,
}

impl Source {
    pub fn direct(url: &str) -> Self {
        Self { url: url.to_string(), headers: Vec::new(), filename: None, max_connections: None, max_segment_size: None, expires: false, decrypt: None }
    }
}

/// What the user told a site module for one download (`zoo.ask` with `remember`), by question
/// id: retries, resumes and expired links resolve again without asking. Kept with the task;
/// secret answers only while the app runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answers {
    #[serde(default)]
    saved: BTreeMap<String, String>,
    #[serde(skip)]
    secret: BTreeMap<String, String>,
}

impl Answers {
    pub fn get(&self, id: &str) -> Option<&str> {
        self.secret.get(id).or_else(|| self.saved.get(id)).map(String::as_str)
    }

    pub fn set(&mut self, id: &str, value: String, secret: bool) {
        self.saved.remove(id);
        self.secret.remove(id);
        if secret { &mut self.secret } else { &mut self.saved }.insert(id.to_string(), value);
    }

    pub fn is_empty(&self) -> bool {
        self.saved.is_empty() && self.secret.is_empty()
    }

    /// The answers that may be stored or shown (not the secret ones).
    pub fn saved(&self) -> &BTreeMap<String, String> {
        &self.saved
    }

    pub fn from_saved(saved: BTreeMap<String, String>) -> Self {
        Self { saved, secret: BTreeMap::new() }
    }
}

/// A download's answers while it resolves: the resolver reads and adds to them, and the task
/// keeps whatever they hold afterwards, even when resolution failed.
pub type Memory = Arc<Mutex<Answers>>;

/// Tells the task's owner what resolution is doing, for display.
pub type PhaseReport = Box<dyn Fn(Phase) + Send + Sync>;

pub type Resolving<'a> = Pin<Box<dyn Future<Output = Result<Source, CoreError>> + Send + 'a>>;

pub trait Resolver: Send + Sync {
    /// Resolve `url`. Dropping the future abandons the resolution.
    fn resolve<'a>(&'a self, url: &'a str, memory: Memory, report: PhaseReport) -> Resolving<'a>;
}

/// Every URL is the file itself.
pub struct Direct;

impl Resolver for Direct {
    fn resolve<'a>(&'a self, url: &'a str, _memory: Memory, _report: PhaseReport) -> Resolving<'a> {
        Box::pin(async move { Ok(Source::direct(url)) })
    }
}
