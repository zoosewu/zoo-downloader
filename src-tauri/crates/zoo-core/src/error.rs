//! Errors crossing the zoo-core interface.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum CoreError {
    /// The server answered with a non-success status.
    #[error("HTTP {0}")]
    Http(u16),
    /// Connection, DNS, TLS or timeout failure.
    #[error("network error: {0}")]
    Network(String),
    /// The server's answer does not make sense (bad Content-Range, length mismatch, ...).
    #[error("unexpected server response: {0}")]
    Protocol(String),
    /// Local filesystem failure.
    #[error("file error: {0}")]
    Io(String),
    /// No connection could be obtained (e.g. every proxy is blacklisted and direct fallback is off).
    #[error("no connection available: {0}")]
    NoConnection(String),
    #[error("cancelled")]
    Cancelled,
    #[error("task not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    /// A site module could not turn the URL into a download.
    #[error("{message}")]
    Resolve { message: String, hint: Option<String>, retryable: bool },
    /// The site refuses for `retry_after` seconds (a free download limit, a used-up quota).
    /// The task waits that out without a download slot, and the site's other tasks wait too.
    #[error("{message}")]
    SiteLimit {
        message: String,
        hint: Option<String>,
        #[serde(rename = "retryAfter")]
        retry_after: u64,
    },
    /// The task's URL was changed and the new one does not serve the bytes already
    /// downloaded. The partial data is kept; `restart` discards it.
    #[error("the new URL serves a different file: {0}")]
    SourceChanged(String),
    /// The rest of the file does not fit on the disk; checked before anything is written.
    #[error("not enough disk space: {needed} bytes needed, {available} available")]
    InsufficientSpace { needed: u64, available: u64 },
}

impl CoreError {
    /// Worth trying again later without user action.
    pub fn is_retryable(&self) -> bool {
        match self {
            CoreError::Http(code) => matches!(code, 408 | 425 | 429 | 500..=599),
            CoreError::Network(_) | CoreError::NoConnection(_) => true,
            CoreError::Resolve { retryable, .. } => *retryable,
            _ => false,
        }
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Io(e.to_string())
    }
}

impl From<reqwest::Error> for CoreError {
    fn from(e: reqwest::Error) -> Self {
        match e.status() {
            Some(status) => CoreError::Http(status.as_u16()),
            None => CoreError::Network(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryability() {
        assert!(CoreError::Http(503).is_retryable());
        assert!(CoreError::Http(429).is_retryable());
        assert!(!CoreError::Http(404).is_retryable());
        assert!(!CoreError::Http(403).is_retryable());
        assert!(CoreError::Network("reset".into()).is_retryable());
        assert!(!CoreError::Io("disk full".into()).is_retryable());
        assert!(!CoreError::Cancelled.is_retryable());
    }

    #[test]
    fn site_limits_cross_the_seam_in_camel_case() {
        let json = serde_json::to_value(CoreError::SiteLimit { message: "m".into(), hint: None, retry_after: 60 }).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "siteLimit", "detail": { "message": "m", "hint": null, "retryAfter": 60 } }));
    }

    #[test]
    fn serializes_with_kind_tag() {
        let json = serde_json::to_value(CoreError::Http(404)).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "http", "detail": 404 }));
    }
}
