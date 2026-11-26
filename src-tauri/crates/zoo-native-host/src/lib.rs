//! What the browser extension, `zoo-native-host` and the app say to each other.
//!
//! The extension sends one native messaging message per request; the host forwards its JSON
//! as one line over the local socket and passes the one-line answer back. See
//! docs/architecture/browser-extension.md.

pub mod host;

use interprocess::local_socket::Name;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;

/// The app's folder name, also the stem of its socket name. Dev (debug) builds use their own,
/// so a dev build runs beside the installed app without touching its downloads, settings or
/// browser bridge.
pub const APP_DIR: &str = if cfg!(debug_assertions) { "zoo-downloader-dev" } else { "zoo-downloader" };

/// The native messaging host name the extension asks the browser for.
pub const HOST_NAME: &str = "com.zoo.downloader";

/// The extensions allowed to start the host: the copy loaded from the app's folder (ID fixed by
/// the `key` in its manifest) and the Edge Add-ons listing.
pub const EXTENSION_IDS: [&str; 2] = ["oibblcgobgdenncecbfgdcpeegliejcj", "bhgmngdlfelamchfhlahkkclffhpgfma"];

/// `chrome-extension://<id>/` of each allowed extension, as browsers name the caller.
pub fn extension_origins() -> impl Iterator<Item = String> {
    EXTENSION_IDS.iter().map(|id| format!("chrome-extension://{id}/"))
}

pub const PROTOCOL_VERSION: u64 = 1;

/// Longest request line either side accepts (cookies can be long, but not this long).
pub const MAX_LINE: u64 = 1 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Request {
    Ping,
    Add(AddDownload),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDownload {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
}

impl AddDownload {
    /// The request headers the download is made with; empty values are left out.
    pub fn headers(&self) -> Vec<(String, String)> {
        [("Referer", &self.referer), ("User-Agent", &self.user_agent), ("Cookie", &self.cookie)]
            .into_iter()
            .filter_map(|(name, value)| {
                let value = value.as_deref()?.trim();
                (!value.is_empty()).then(|| (name.to_string(), value.to_string()))
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    /// The request was understood but refused (bad URL, filename or header).
    Invalid,
    /// The app is not installed, or did not start in time.
    AppNotRunning,
    UnsupportedVersion,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub ok: bool,
    /// The file name the app chose (`add` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Response {
    pub fn ok(name: Option<String>) -> Self {
        Self { ok: true, name, code: None, message: None }
    }

    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { ok: false, name: None, code: Some(code), message: Some(message.into()) }
    }

    /// One line, as sent over the socket.
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self).expect("a response always serializes");
        line.push('\n');
        line
    }
}

/// Parse a request, checking the version before anything else so an extension newer than
/// the app gets `unsupportedVersion` rather than a parse error.
pub fn parse_request(json: &str) -> Result<Request, Response> {
    let value: Value = serde_json::from_str(json).map_err(|e| Response::error(ErrorCode::Internal, format!("bad request: {e}")))?;
    match value.get("v").and_then(Value::as_u64) {
        Some(PROTOCOL_VERSION) => {}
        v => {
            return Err(Response::error(
                ErrorCode::UnsupportedVersion,
                format!("protocol version {} is not supported (expected {PROTOCOL_VERSION})", v.map_or("missing".into(), |v| v.to_string())),
            ))
        }
    }
    serde_json::from_value(value).map_err(|e| Response::error(ErrorCode::Internal, format!("bad request: {e}")))
}

/// The app's socket. Per user: a named pipe with the user name in it on Windows, a socket file
/// in the user's own app folder elsewhere.
pub fn socket_name() -> io::Result<Name<'static>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::{GenericNamespaced, ToNsName};
        let user: String = std::env::var("USERNAME").unwrap_or_default().chars().filter(char::is_ascii_alphanumeric).collect();
        format!("{APP_DIR}-bridge-{user}").to_ns_name::<GenericNamespaced>().map(Name::into_owned)
    }
    #[cfg(unix)]
    {
        use interprocess::local_socket::{GenericFilePath, ToFsName};
        socket_dir()?.join("bridge.sock").to_fs_name::<GenericFilePath>().map(Name::into_owned)
    }
}

/// The folder of the app's socket file: the app's own folder. Only the user may enter it, which
/// is what keeps others off the socket (macOS cannot set a socket file's mode).
#[cfg(unix)]
pub fn socket_dir() -> io::Result<std::path::PathBuf> {
    let dir = dirs::config_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config directory"))?;
    Ok(dir.join(APP_DIR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_ping_and_add_in_camel_case() {
        assert_eq!(parse_request(r#"{"v":1,"type":"ping"}"#), Ok(Request::Ping));
        let add = parse_request(r#"{"v":1,"type":"add","url":"https://a/b.zip","filename":"b.zip","userAgent":"UA","cookie":"a=1"}"#);
        assert_eq!(
            add,
            Ok(Request::Add(AddDownload {
                url: "https://a/b.zip".into(),
                filename: Some("b.zip".into()),
                user_agent: Some("UA".into()),
                cookie: Some("a=1".into()),
                referer: None,
            }))
        );
    }

    #[test]
    fn checks_the_version_first() {
        for json in [r#"{"v":2,"type":"teleport"}"#, r#"{"type":"ping"}"#] {
            assert_eq!(parse_request(json).unwrap_err().code, Some(ErrorCode::UnsupportedVersion));
        }
    }

    #[test]
    fn rejects_malformed_requests() {
        for json in ["not json", r#"{"v":1,"type":"add"}"#, r#"{"v":1,"type":"teleport"}"#] {
            assert_eq!(parse_request(json).unwrap_err().code, Some(ErrorCode::Internal));
        }
    }

    #[test]
    fn request_round_trips() {
        let req = Request::Add(AddDownload { url: "https://a/".into(), referer: Some("https://r/".into()), ..Default::default() });
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json, json!({ "type": "add", "url": "https://a/", "referer": "https://r/" }));
        assert_eq!(serde_json::from_value::<Request>(json).unwrap(), req);
    }

    #[test]
    fn responses_are_one_camel_case_line() {
        assert_eq!(Response::ok(Some("a.zip".into())).to_line(), "{\"ok\":true,\"name\":\"a.zip\"}\n");
        let err = serde_json::to_value(Response::error(ErrorCode::AppNotRunning, "x")).unwrap();
        assert_eq!(err, json!({ "ok": false, "code": "appNotRunning", "message": "x" }));
    }

    #[test]
    fn headers_skip_missing_and_blank_values() {
        let add = AddDownload {
            url: "https://a/".into(),
            referer: Some("https://r/".into()),
            user_agent: Some("  ".into()),
            cookie: Some("a=1; b=2".into()),
            ..Default::default()
        };
        assert_eq!(add.headers(), vec![("Referer".into(), "https://r/".into()), ("Cookie".into(), "a=1; b=2".into())]);
    }

    #[test]
    fn socket_name_is_per_user() {
        let name = format!("{:?}", socket_name().unwrap());
        #[cfg(windows)]
        assert!(name.contains(&format!("{APP_DIR}-bridge-")), "{name}");
        #[cfg(unix)]
        assert!(name.contains("bridge.sock"), "{name}");
    }
}
