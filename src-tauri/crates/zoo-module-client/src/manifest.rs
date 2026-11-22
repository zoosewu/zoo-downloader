//! `manifest.json` of a site module. `apiVersion` is the API level the module is written
//! against; this app runs levels `MIN_API_VERSION..=API_VERSION`. A level adds to the one
//! before it (level 2: help texts and translations for account fields; level 3: `zoo.ask`; level 4: `decrypt` in `resolve`'s result); a change that breaks
//! old modules would raise `MIN_API_VERSION`.
//! The editor-facing contract is `crates/zoo-module/sdk/manifest.schema.json`; tests keep the
//! two in agreement.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The newest API level this app runs (the SDK's).
pub const API_VERSION: u32 = 4;
/// The oldest API level this app still runs.
pub const MIN_API_VERSION: u32 = 1;

/// Whether this app runs modules written against `level`, and if not, why.
pub fn check_api_version(level: u32) -> Result<(), String> {
    if level > API_VERSION {
        Err(format!("apiVersion {level} needs a newer Zoo Downloader (this one runs apiVersion {MIN_API_VERSION} to {API_VERSION})"))
    } else if level < MIN_API_VERSION {
        Err(format!("apiVersion {level} is no longer supported (this app runs apiVersion {MIN_API_VERSION} to {API_VERSION})"))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default = "default_entry")]
    pub entry: String,
    pub url_patterns: Vec<String>,
    /// Narrows `urlPatterns`: a URL must also match this regular expression (Rust `regex`
    /// syntax, no lookaround or backreferences).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_regex: Option<String>,
    /// A copied link may start downloading without the add-download page, when the user allows
    /// it. Leave it off for sites that usually ask for a captcha.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_start: bool,
    /// Shown above the account fields: where the values come from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_help: Option<Text>,
    /// A page explaining it further (https).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_help_url: Option<String>,
    pub permissions: Permissions,
}

/// Text shown to the user: one string, or one per language (`{"en": "...", "zh-TW": "..."}`);
/// the app shows the user's language, else English, else the first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum Text {
    Plain(String),
    Translated(BTreeMap<String, String>),
}

impl Text {
    /// English, else the first language: for logs and terminals.
    pub fn fallback(&self) -> &str {
        match self {
            Text::Plain(s) => s,
            Text::Translated(by_language) => by_language.get("en").or_else(|| by_language.values().next()).map_or("", String::as_str),
        }
    }

    /// Every value is 1 to `max` characters; languages are tags such as `en` or `zh-TW`.
    pub fn check(&self, what: &str, max: usize) -> Result<(), String> {
        let values: Vec<&String> = match self {
            Text::Plain(s) => vec![s],
            Text::Translated(by_language) => {
                check(!by_language.is_empty(), || format!("{what}: give at least one language"))?;
                for language in by_language.keys() {
                    check(is_language(language), || format!("{what}: {language:?} is not a language tag such as en or zh-TW"))?;
                }
                by_language.values().collect()
            }
        };
        let ok = values.iter().all(|v| !v.trim().is_empty() && v.chars().count() <= max);
        check(ok, || format!("{what}: 1 to {max} characters"))
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Text::Plain(s.into())
    }
}

fn is_language(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let primary = parts.next().unwrap_or_default();
    (2..=3).contains(&primary.len())
        && primary.bytes().all(|b| b.is_ascii_lowercase())
        && parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn check_help_url(what: &str, url: &Option<String>) -> Result<(), String> {
    let ok = url.as_ref().is_none_or(|u| u.strip_prefix("https://").is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace)));
    check(ok, || format!("{what}: an https:// URL"))
}

fn default_entry() -> String {
    "dist/index.js".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Permissions {
    pub network: Network,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<CredentialField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Network {
    pub allow_domains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialField {
    pub key: String,
    pub label: Text,
    #[serde(default = "yes")]
    pub secret: bool,
    #[serde(default)]
    pub optional: bool,
    /// Shown under the field: how to get the value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<Text>,
    /// A page explaining it further (https).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help_url: Option<String>,
}

fn yes() -> bool {
    true
}

impl Manifest {
    /// Parses and checks what serde cannot; errors say how to fix the file.
    pub fn parse(json: &str) -> Result<Self, String> {
        // The level first: a newer module may use fields this app does not know, and "needs a
        // newer app" says more than "unknown field".
        #[derive(Deserialize)]
        struct Level {
            #[serde(rename = "apiVersion")]
            api_version: u32,
        }
        let level: Level = serde_json::from_str(json).map_err(|e| format!("manifest.json: {e}"))?;
        check_api_version(level.api_version).map_err(|e| format!("manifest.json: {e}"))?;
        let manifest: Manifest = serde_json::from_str(json).map_err(|e| format!("manifest.json: {e}"))?;
        let m = &manifest;
        if m.api_version < 2 {
            let uses = m.credentials_help.is_some()
                || m.credentials_help_url.is_some()
                || m.permissions.credentials.iter().any(|c| c.help.is_some() || c.help_url.is_some() || matches!(c.label, Text::Translated(_)));
            check(!uses, || "credentialsHelp, help, helpUrl and translated labels need \"apiVersion\": 2".into())?;
        }
        let id_ok = m.id.len() <= 63
            && m.id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && m.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        check(id_ok, || format!("id {:?}: use lowercase letters, digits and '-' (at most 63)", m.id))?;
        check(!m.name.is_empty() && m.name.chars().count() <= 80, || "name: 1 to 80 characters".into())?;
        check(is_semver(&m.version), || format!("version {:?}: use MAJOR.MINOR.PATCH, e.g. 1.0.0", m.version))?;
        check(m.description.as_ref().is_none_or(|d| d.chars().count() <= 500), || "description: at most 500 characters".into())?;
        check(m.author.as_ref().is_none_or(|a| a.chars().count() <= 100), || "author: at most 100 characters".into())?;
        check(m.homepage.as_ref().is_none_or(|h| h.starts_with("http://") || h.starts_with("https://")), || {
            "homepage: must be an http(s) URL".into()
        })?;
        let entry_ok = m.entry.ends_with(".js") && !m.entry.starts_with('/') && !m.entry.contains('\\');
        check(entry_ok, || format!("entry {:?}: a relative path to a .js file", m.entry))?;
        check(!m.url_patterns.is_empty(), || "urlPatterns is empty; list the share-page URLs the module handles".into())?;
        check(unique(&m.url_patterns), || "urlPatterns: duplicate entries".into())?;
        for pattern in &m.url_patterns {
            let rest = pattern.strip_prefix("https://").or_else(|| pattern.strip_prefix("http://"));
            let ok = rest.and_then(|r| r.split_once('/')).is_some_and(|(host, _)| !host.is_empty());
            check(ok, || format!("urlPatterns {pattern:?}: start with http(s)://host/, e.g. https://example.com/file/*"))?;
        }
        if let Some(regex) = &m.url_regex {
            url_regex(regex).map_err(|e| format!("manifest.json: urlRegex: {e}"))?;
        }
        check(unique(&m.permissions.network.allow_domains), || "allowDomains: duplicate entries".into())?;
        for domain in &m.permissions.network.allow_domains {
            check(is_domain(domain), || {
                format!("allowDomains {domain:?}: a bare host such as example.com (no scheme, path or port)")
            })?;
        }
        for field in &m.permissions.credentials {
            let key_ok = !field.key.is_empty() && field.key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            check(key_ok, || format!("credentials key {:?}: letters, digits and '_' only", field.key))?;
            let what = |name: &str| format!("credentials {:?}: {name}", field.key);
            field.label.check(&what("label"), 80)?;
            if let Some(help) = &field.help {
                help.check(&what("help"), 300)?;
            }
            check_help_url(&what("helpUrl"), &field.help_url)?;
        }
        if let Some(help) = &m.credentials_help {
            help.check("credentialsHelp", 500)?;
        }
        check_help_url("credentialsHelpUrl", &m.credentials_help_url)?;
        Ok(manifest)
    }
}

/// Longest `urlRegex` accepted; the compiled program is capped too, so no manifest can make
/// matching slow or large (the `regex` crate matches in linear time).
const MAX_URL_REGEX: usize = 1000;

/// Compile a `urlRegex`.
pub fn url_regex(source: &str) -> Result<regex::Regex, String> {
    if source.len() > MAX_URL_REGEX {
        return Err(format!("at most {MAX_URL_REGEX} characters"));
    }
    regex::RegexBuilder::new(source).size_limit(1 << 20).build().map_err(|e| e.to_string())
}

/// Whether a module with these `urlPatterns` and compiled `urlRegex` handles `url`: a pattern
/// and the regex must both match. The fragment is ignored.
pub fn handles(patterns: &[String], regex: Option<&regex::Regex>, url: &str) -> bool {
    let url = url.split('#').next().unwrap_or(url);
    patterns.iter().any(|p| url_matches(p, url)) && regex.is_none_or(|r| r.is_match(url))
}

/// Whether `url` matches a `urlPatterns` entry. Scheme, host and the rest are compared apart:
/// `*` matches any run of characters but never crosses from the host into the path, so
/// `https://*.k2s.cc/*` cannot match `https://evil.com/x.k2s.cc/`. Host and scheme ignore
/// case; the fragment is ignored.
pub fn url_matches(pattern: &str, url: &str) -> bool {
    let split = |s: &str| -> Option<(String, String, String)> {
        let (scheme, rest) = s.split_once("://")?;
        let (host, path) = rest.find('/').map_or((rest, "/"), |i| (&rest[..i], &rest[i..]));
        Some((scheme.to_ascii_lowercase(), host.to_ascii_lowercase(), path.to_string()))
    };
    let url = url.split('#').next().unwrap_or(url);
    match (split(pattern), split(url)) {
        (Some((ps, ph, pp)), Some((us, uh, up))) => ps == us && glob(&ph, &uh) && glob(&pp, &up),
        _ => false,
    }
}

/// `*` matches any run of characters; everything else literally (also used by test expectations).
pub fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            backtrack = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((star, matched)) = backtrack {
            pi = star + 1;
            ti = matched + 1;
            backtrack = Some((star, matched + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

fn check(ok: bool, message: impl FnOnce() -> String) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("manifest.json: {}", message()))
    }
}

fn unique(items: &[String]) -> bool {
    let mut seen = std::collections::HashSet::new();
    items.iter().all(|item| seen.insert(item))
}

fn is_semver(version: &str) -> bool {
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    let number = |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (p.len() == 1 || !p.starts_with('0'));
    parts.len() == 3
        && parts.iter().all(number)
        && pre.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'))
}

fn is_domain(domain: &str) -> bool {
    let labels: Vec<&str> = domain.strip_prefix("*.").unwrap_or(domain).split('.').collect();
    labels.len() >= 2 && labels.iter().all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{
        "$schema": "./types/manifest.schema.json",
        "apiVersion": 2,
        "id": "example-host",
        "name": "Example Host",
        "version": "1.2.3",
        "description": "d",
        "author": "a",
        "homepage": "https://example.com",
        "entry": "dist/index.js",
        "urlPatterns": ["https://example.com/file/*"],
        "credentialsHelp": { "en": "Sign in at example.com first.", "zh-TW": "先登入 example.com。" },
        "credentialsHelpUrl": "https://example.com/help",
        "permissions": {
            "network": { "allowDomains": ["example.com"] },
            "credentials": [{ "key": "password", "label": { "en": "Password", "ja": "パスワード" }, "secret": true, "optional": true,
                              "help": "Account settings → API", "helpUrl": "https://example.com/help#password" }]
        }
    }"#;

    #[test]
    fn url_patterns() {
        let cases = [
            ("https://k2s.cc/file/*", "https://k2s.cc/file/abc/name.zip", true),
            ("https://k2s.cc/file/*", "HTTPS://K2S.CC/file/abc", true),
            ("https://k2s.cc/file/*", "https://k2s.cc/file/abc#frag", true),
            ("https://k2s.cc/file/*", "http://k2s.cc/file/abc", false),
            ("https://k2s.cc/file/*", "https://k2s.cc/folder/abc", false),
            ("https://*.k2s.cc/file/*", "https://www.k2s.cc/file/abc", true),
            ("https://*.k2s.cc/file/*", "https://k2s.cc/file/abc", false),
            ("https://*.k2s.cc/*", "https://evil.com/x.k2s.cc/file", false),
            ("https://k2s.cc/*", "https://k2s.cc.evil.com/x", false),
            ("https://site.com/f/*/d*", "https://site.com/f/1/download", true),
            ("https://site.com/f/*", "https://site.com/f/", true),
            ("https://site.com/f/*?x=1", "https://site.com/f/a?x=1", true),
        ];
        for (pattern, url, expected) in cases {
            assert_eq!(url_matches(pattern, url), expected, "{pattern} vs {url}");
        }
    }

    #[test]
    fn parses_every_field() {
        let m = Manifest::parse(FULL).unwrap();
        assert_eq!(m.permissions.credentials[0].key, "password");
        assert!(m.permissions.credentials[0].optional);
        assert_eq!(m.permissions.credentials[0].help, Some("Account settings → API".into()));
        assert!(matches!(&m.credentials_help, Some(Text::Translated(t)) if t["zh-TW"] == "先登入 example.com。"));
    }

    #[test]
    fn defaults_and_rejections() {
        let minimal = r#"{ "apiVersion": 1, "id": "x", "name": "X", "version": "1.0.0",
            "urlPatterns": ["https://x.com/*"], "permissions": { "network": { "allowDomains": [] },
            "credentials": [{ "key": "k", "label": "K" }] } }"#;
        let m = Manifest::parse(minimal).unwrap();
        assert_eq!(m.entry, "dist/index.js");
        assert!(m.permissions.credentials[0].secret);

        let newer = Manifest::parse(&minimal.replace("\"apiVersion\": 1", "\"apiVersion\": 5")).unwrap_err();
        assert!(newer.contains("needs a newer Zoo Downloader"), "{newer}");
        let newer_with_new_fields = minimal.replace("\"apiVersion\": 1", "\"apiVersion\": 5").replace("\"id\"", "\"futureField\": 1, \"id\"");
        assert!(Manifest::parse(&newer_with_new_fields).unwrap_err().contains("needs a newer Zoo Downloader"), "the level is checked first");
        assert!(Manifest::parse(&minimal.replace("\"apiVersion\": 1", "\"apiVersion\": 0")).unwrap_err().contains("no longer supported"));
        assert!(Manifest::parse(&minimal.replace("\"id\"", "\"className\": \"X\", \"id\"")).is_err(), "unknown fields fail");
        assert!(!m.auto_start && m.url_regex.is_none());

        let with = |field: &str| minimal.replace("\"id\"", &format!("{field}, \"id\""));
        let m = Manifest::parse(&with(r#""autoStart": true, "urlRegex": "^https://x\\.com/[0-9]+$""#)).unwrap();
        assert!(m.auto_start && m.url_regex.is_some());
        assert!(Manifest::parse(&with(r#""urlRegex": "(""#)).unwrap_err().contains("urlRegex"));
        assert!(Manifest::parse(&with(r#""urlRegex": "(?=x)""#)).unwrap_err().contains("urlRegex"), "no lookaround");
        let long = format!(r#""urlRegex": "{}""#, "a".repeat(MAX_URL_REGEX + 1));
        assert!(Manifest::parse(&with(&long)).unwrap_err().contains("urlRegex"));
    }

    #[test]
    fn help_texts_are_checked() {
        let with_field = |field: &str| FULL.replace(r#""key": "password", "label": { "en": "Password", "ja": "パスワード" }"#, field);
        for (bad, why) in [
            (with_field(r#""key": "password", "label": """#), "empty label"),
            (with_field(r#""key": "password", "label": {}"#), "no language"),
            (with_field(r#""key": "password", "label": { "English": "Password" }"#), "not a language tag"),
            (with_field(r#""key": "password", "label": { "en": " " }"#), "blank translation"),
            (FULL.replace("https://example.com/help#password", "http://example.com/help"), "help page not https"),
            (FULL.replace(r#""https://example.com/help","#, r#""javascript:alert(1)","#), "credentialsHelpUrl not https"),
            (FULL.replace("Account settings → API", &"x".repeat(301)), "help too long"),
        ] {
            assert!(Manifest::parse(&bad).is_err(), "{why} should be rejected");
        }
    }

    #[test]
    fn level_2_fields_need_api_version_2() {
        assert!(FULL.contains("\"apiVersion\": 2"));
        let err = Manifest::parse(&FULL.replace("\"apiVersion\": 2", "\"apiVersion\": 1")).unwrap_err();
        assert!(err.contains("apiVersion\": 2"), "{err}");
    }
}
