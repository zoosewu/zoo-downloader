//! Wire format between the app and `zoo-module serve`: one JSON object per line.
//! App → host on stdin: `Request`. Host → app on stdout: `Reply`. Host diagnostics go to stderr.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::manifest::Text;

/// An installed module: its entry point (the bundled single-file ESM) and what the user let it do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleRef {
    /// `id@marketplace`; used in logs and errors.
    pub id: String,
    pub entry: PathBuf,
    #[serde(default)]
    pub grants: Grants,
    /// The manifest's `apiVersion`: the host offers only that level's API.
    #[serde(default = "first_level")]
    pub api_version: u32,
}

fn first_level() -> u32 {
    1
}

/// Permissions the user consented to (from the manifest). Everything not granted is denied.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grants {
    /// Hosts `fetch` may reach; each entry also covers its subdomains.
    pub allow_domains: Vec<String>,
    /// Credential values by key (`[A-Za-z0-9_]+`). Module code only ever sees placeholders.
    #[serde(default)]
    pub credentials: BTreeMap<String, Secret>,
}

/// A credential value. Never printed.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Resource limits for one `resolve` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub heap_mb: u32,
    /// Wall-clock budget for the call.
    pub timeout_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { heap_mb: 128, timeout_ms: 30_000 }
    }
}

/// What a module's `resolve(url, zoo)` returns: the direct download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resolved {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// The most parallel connections the site tolerates for this download.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_connections: Option<u32>,
    /// The largest byte range the site serves in one request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_segment_size: Option<u64>,
    /// The server sends the file encrypted; the app decrypts it as it downloads (API level 4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decrypt: Option<Encryption>,
}

/// How a resolved file is encrypted. Keys are hex.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cipher", deny_unknown_fields)]
pub enum Encryption {
    /// AES-128 in counter mode; `iv` is the 128-bit big-endian counter block for byte 0.
    #[serde(rename = "aes-128-ctr")]
    Aes128Ctr { key: String, iv: String },
}

impl std::fmt::Debug for Encryption {
    // Never the key.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Encryption::Aes128Ctr { .. } => f.write_str("Aes128Ctr"),
        }
    }
}

impl Encryption {
    /// AES-128-CTR's key and initial counter block, or why they are unusable.
    pub fn aes128_ctr(&self) -> Result<([u8; 16], [u8; 16]), String> {
        match self {
            Encryption::Aes128Ctr { key, iv } => Ok((
                bytes16(key).ok_or("decrypt.key must be 32 hex digits (16 bytes)")?,
                bytes16(iv).ok_or("decrypt.iv must be 32 hex digits (16 bytes)")?,
            )),
        }
    }
}

fn bytes16(hex: &str) -> Option<[u8; 16]> {
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    let mut out = [0; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(out)
}

/// Most files one `list` call may return.
pub const MAX_LISTED: usize = 1000;

/// One file of a link that holds several (`list(url)`). Its own `url` resolves to just this file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Listed {
    pub url: String,
    /// `None` only for the item the host makes up for a module without `list`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Folder inside the share (`a/b`, relative, `/`-separated); `None` at the top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ModuleError {
    /// The module file could not be read, parsed or linked, or has the wrong shape.
    #[error("module load failed: {message}")]
    Load { message: String, hint: Option<String> },
    /// `resolve` threw, rejected, or returned something invalid.
    #[error("{message}")]
    Script { message: String, hint: Option<String>, stack: Option<String> },
    #[error("module exceeded its {limit_ms} ms time limit")]
    Timeout { limit_ms: u64 },
    #[error("module exceeded its {limit_mb} MB memory limit")]
    OutOfMemory { limit_mb: u32 },
    /// The host process died, could not start, or stopped answering.
    #[error("module host failed: {message}")]
    Crashed { message: String },
    /// The app cancelled the call.
    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Request {
    Resolve { id: u64, module: ModuleRef, url: String, limits: Limits },
    /// The files behind `url` (the module's `list`; one item, `url` itself, if it has none).
    List { id: u64, module: ModuleRef, url: String, limits: Limits },
    /// The user's answer to a `CallEvent::Captcha`; `None` if they declined.
    Answer { id: u64, ticket: u64, solution: Option<String> },
    /// The app no longer wants the result: stop the call now.
    Cancel { id: u64 },
}

impl Request {
    pub fn id(&self) -> u64 {
        match self {
            Request::Resolve { id, .. } | Request::List { id, .. } | Request::Answer { id, .. } | Request::Cancel { id } => *id,
        }
    }
}

/// `id` is the call's id. A call gets any number of `Event`s, then one `Resolved` / `Listed`
/// (matching its request) or `Failed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Reply {
    Event { id: u64, event: CallEvent },
    Resolved { id: u64, resolved: Resolved },
    Listed { id: u64, items: Vec<Listed> },
    Failed { id: u64, error: ModuleError },
}

impl Reply {
    pub fn id(&self) -> u64 {
        match self {
            Reply::Event { id, .. } | Reply::Resolved { id, .. } | Reply::Listed { id, .. } | Reply::Failed { id, .. } => *id,
        }
    }
}

/// Something the user should see while a call runs. Neither counts against the time limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CallEvent {
    /// `zoo.wait`: the site makes free users wait.
    Waiting { ms: u64, reason: Option<String> },
    /// The module needs a captcha solved; answer with `Request::Answer` and this ticket.
    Captcha { ticket: u64, challenge: Captcha },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Captcha {
    /// Solve in a webview showing `page_url`; the solution is the g-recaptcha-response token.
    Recaptcha { site_key: String, page_url: String },
    /// Show the image (already fetched through the module's session); the solution is the text.
    Image { mime: String, base64: String },
    /// `zoo.ask`: something only the user knows, such as a share's extraction code; the solution
    /// is what they type. `id` names the question within the module. With `remember` the app keeps
    /// the answer for this download and gives it again without asking; `error` (the answer the
    /// module had did not work) always asks.
    Ask { id: String, message: Text, secret: bool, remember: bool, error: Option<Text> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_wire_shape() {
        let req = Request::Resolve {
            id: 7,
            module: ModuleRef {
                id: "k2s@official".into(),
                entry: "m/index.js".into(),
                grants: Grants {
                    allow_domains: vec!["k2s.cc".into()],
                    credentials: BTreeMap::from([("password".into(), Secret("hunter2".into()))]),
                },
                api_version: 3,
            },
            url: "https://example.com/f".into(),
            limits: Limits::default(),
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({
                "type": "resolve",
                "id": 7,
                "module": { "id": "k2s@official", "entry": "m/index.js", "grants": { "allowDomains": ["k2s.cc"], "credentials": { "password": "hunter2" } }, "apiVersion": 3 },
                "url": "https://example.com/f",
                "limits": { "heapMb": 128, "timeoutMs": 30000 }
            })
        );
    }

    #[test]
    fn decrypt_keys_are_hex() {
        let wire = json!({ "url": "https://x", "decrypt": { "cipher": "aes-128-ctr", "key": "000102030405060708090a0b0c0d0e0f", "iv": "ff".repeat(16) } });
        let resolved: Resolved = serde_json::from_value(wire).unwrap();
        let (key, iv) = resolved.decrypt.unwrap().aes128_ctr().unwrap();
        assert_eq!(key, std::array::from_fn::<u8, 16, _>(|i| i as u8));
        assert_eq!(iv, [0xff; 16]);
        let short = Encryption::Aes128Ctr { key: "00".into(), iv: "ff".repeat(16) };
        assert!(short.aes128_ctr().unwrap_err().contains("decrypt.key"));
        let unknown = json!({ "url": "https://x", "decrypt": { "cipher": "rot13" } });
        assert!(serde_json::from_value::<Resolved>(unknown).is_err());
        // Never printed.
        assert_eq!(format!("{short:?}"), "Aes128Ctr");
    }

    #[test]
    fn reply_wire_shape_and_roundtrip() {
        let replies = [
            Reply::Resolved {
                id: 1,
                resolved: Resolved {
                    url: "https://cdn/x".into(),
                    filename: Some("x.zip".into()),
                    headers: BTreeMap::from([("Referer".into(), "https://a".into())]),
                    size: Some(10),
                    max_connections: Some(2),
                    max_segment_size: Some(4 << 20),
                    decrypt: Some(Encryption::Aes128Ctr { key: "00".repeat(16), iv: "ff".repeat(16) }),
                },
            },
            Reply::Failed { id: 2, error: ModuleError::Timeout { limit_ms: 5 } },
            Reply::Listed {
                id: 4,
                items: vec![Listed { url: "https://s/1#path=a%2Fb.zip".into(), filename: Some("b.zip".into()), size: Some(3), folder: Some("a".into()) }],
            },
            Reply::Failed {
                id: 3,
                error: ModuleError::Script { message: "m".into(), hint: Some("h".into()), stack: None },
            },
        ];
        for reply in &replies {
            let text = serde_json::to_string(reply).unwrap();
            assert_eq!(&serde_json::from_str::<Reply>(&text).unwrap(), reply);
        }
        assert_eq!(
            serde_json::to_value(&replies[1]).unwrap(),
            json!({ "type": "failed", "id": 2, "error": { "kind": "timeout", "limitMs": 5 } })
        );
        assert_eq!(
            serde_json::to_value(&replies[2]).unwrap(),
            json!({ "type": "listed", "id": 4, "items": [{ "url": "https://s/1#path=a%2Fb.zip", "filename": "b.zip", "size": 3, "folder": "a" }] })
        );
    }

    #[test]
    fn secrets_never_print() {
        let grants = Grants { allow_domains: vec![], credentials: BTreeMap::from([("p".into(), Secret("hunter2".into()))]) };
        assert!(!format!("{grants:?}").contains("hunter2"));
    }

    #[test]
    fn event_wire_shape() {
        let reply = Reply::Event {
            id: 4,
            event: CallEvent::Captcha {
                ticket: 1,
                challenge: Captcha::Recaptcha { site_key: "k".into(), page_url: "https://p".into() },
            },
        };
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({ "type": "event", "id": 4, "event": { "kind": "captcha", "ticket": 1,
                    "challenge": { "type": "recaptcha", "siteKey": "k", "pageUrl": "https://p" } } })
        );
        let ask = Captcha::Ask { id: "pwd".into(), message: "Code".into(), secret: false, remember: true, error: None };
        assert_eq!(
            serde_json::to_value(&ask).unwrap(),
            json!({ "type": "ask", "id": "pwd", "message": "Code", "secret": false, "remember": true, "error": null })
        );
        let answer = Request::Answer { id: 4, ticket: 1, solution: None };
        assert_eq!(serde_json::to_value(&answer).unwrap(), json!({ "type": "answer", "id": 4, "ticket": 1, "solution": null }));
    }

    #[test]
    fn resolved_rejects_unknown_fields() {
        // Typos like `fileName` must fail loudly instead of being dropped.
        assert!(serde_json::from_value::<Resolved>(json!({ "url": "u", "fileName": "x" })).is_err());
        let min: Resolved = serde_json::from_value(json!({ "url": "u" })).unwrap();
        assert_eq!(min.filename, None);
        assert!(min.headers.is_empty());
    }
}
