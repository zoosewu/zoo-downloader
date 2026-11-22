//! The one error shape every new command returns: `{ code, message }`.
//! The frontend can always show `message`; `code` is for branching.

use serde::Serialize;
use ts_rs::TS;
use zoo_core::CoreError;
use zoo_marketplace::MarketError;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }
}

impl From<CoreError> for CommandError {
    fn from(e: CoreError) -> Self {
        let code = match &e {
            CoreError::Http(_) => "http",
            CoreError::Network(_) => "network",
            CoreError::Protocol(_) => "protocol",
            CoreError::Io(_) => "io",
            CoreError::NoConnection(_) => "noConnection",
            CoreError::Cancelled => "cancelled",
            CoreError::NotFound => "notFound",
            CoreError::Invalid(_) => "invalid",
            CoreError::Resolve { .. } => "resolve",
            CoreError::SiteLimit { .. } => "siteLimit",
            CoreError::SourceChanged(_) => "sourceChanged",
            CoreError::InsufficientSpace { .. } => "insufficientSpace",
        };
        Self::new(code, e.to_string())
    }
}

impl From<MarketError> for CommandError {
    fn from(e: MarketError) -> Self {
        let code = match &e {
            MarketError::Invalid(_) => "invalid",
            MarketError::NotFound(_) => "notFound",
            MarketError::Network(_) => "network",
            MarketError::Git(_) => "git",
            MarketError::Io(_) => "io",
            MarketError::Integrity(_) => "integrity",
            MarketError::Secrets(_) => "secrets",
        };
        Self::new(code, e.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io", e.to_string())
    }
}

pub type CmdResult<T> = Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_message() {
        let json = serde_json::to_value(CommandError::from(CoreError::Http(404))).unwrap();
        assert_eq!(json, serde_json::json!({ "code": "http", "message": "HTTP 404" }));
    }
}
