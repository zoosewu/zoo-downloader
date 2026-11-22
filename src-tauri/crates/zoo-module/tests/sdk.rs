//! The SDK module authors code against (sdk/*.d.ts, sdk/manifest.schema.json) must describe the
//! real host and the real manifest parser: nothing promised that is missing, nothing present
//! that is undocumented.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{eval, host};
use serde_json::{json, Value};
use zoo_module_client::manifest::Manifest;

const GLOBALS_DTS: &str = include_str!("../sdk/globals.d.ts");
const SCHEMA: &str = include_str!("../sdk/manifest.schema.json");
const TEMPLATE_MANIFEST: &str = include_str!("../template/manifest.json");

/// Globals V8 itself provides (ECMAScript and WebAssembly); the TypeScript lib covers these.
const ECMASCRIPT: &[&str] = &[
    "AggregateError", "Array", "ArrayBuffer", "AsyncDisposableStack", "Atomics", "BigInt", "BigInt64Array",
    "BigUint64Array", "Boolean", "DataView", "Date", "DisposableStack", "Error", "EvalError", "FinalizationRegistry",
    "Float16Array", "Float32Array", "Float64Array", "Function", "Infinity", "Int16Array", "Int32Array", "Int8Array",
    "Intl", "Iterator", "JSON", "Map", "Math", "NaN", "Number", "Object", "Promise", "Proxy", "RangeError",
    "ReferenceError", "Reflect", "RegExp", "Set", "SharedArrayBuffer", "String", "SuppressedError", "Symbol",
    "SyntaxError", "Temporal", "TypeError", "URIError", "Uint16Array", "Uint32Array", "Uint8Array",
    "Uint8ClampedArray", "WeakMap", "WeakRef", "WeakSet", "WebAssembly", "decodeURI", "decodeURIComponent",
    "encodeURI", "encodeURIComponent", "escape", "eval", "globalThis", "isFinite", "isNaN", "parseFloat",
    "parseInt", "undefined", "unescape",
];
/// Interfaces describing what callers pass in; there is no runtime object to inspect.
const OPTION_BAGS: &[&str] = &["RequestInit", "ResponseInit"];
/// Host plumbing visible to module code but not part of the API (calls the entry's resolve).
const INTERNAL: &[&str] = &["__zooRun"];

/// `declare var|function|class NAME` in globals.d.ts.
fn declared_globals() -> BTreeSet<String> {
    GLOBALS_DTS
        .lines()
        .filter_map(|line| {
            let rest = ["declare var ", "declare function ", "declare class "].iter().find_map(|p| line.strip_prefix(p))?;
            Some(rest.split(|c: char| !(c.is_alphanumeric() || c == '_')).next()?.to_string())
        })
        .collect()
}

#[derive(Default, serde::Serialize)]
struct Members {
    instance: Vec<String>,
    #[serde(rename = "static")]
    statics: Vec<String>,
}

/// Members of every `declare class` / `interface` block, by name.
fn declared_members() -> BTreeMap<String, Members> {
    let mut all = BTreeMap::new();
    let mut current: Option<(String, Members)> = None;
    for line in GLOBALS_DTS.lines() {
        if let Some((name, members)) = current.as_mut() {
            if line == "}" {
                let (name, members) = current.take().unwrap();
                all.insert(name, members);
                continue;
            }
            let _ = name;
            let mut text = line.trim();
            if text.starts_with("//") || text.starts_with("/*") || text.starts_with('*') || text.is_empty() {
                continue;
            }
            let is_static = text.starts_with("static ");
            for prefix in ["static ", "readonly "] {
                text = text.strip_prefix(prefix).unwrap_or(text);
            }
            if text.starts_with("private ") || text.starts_with("constructor") {
                continue;
            }
            let member = if text.starts_with("[Symbol.iterator]") {
                "@@iterator".to_string()
            } else {
                text.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap_or_default().to_string()
            };
            if !member.is_empty() {
                if is_static { &mut members.statics } else { &mut members.instance }.push(member);
            }
            continue;
        }
        let header = line.strip_prefix("declare class ").or_else(|| line.strip_prefix("interface "));
        if let Some(header) = header.filter(|_| line.ends_with('{')) {
            let name = header.split(|c: char| !(c.is_alphanumeric() || c == '_')).next().unwrap().to_string();
            current = Some((name, Members::default()));
        }
    }
    all
}

#[tokio::test]
async fn globals_are_exactly_the_declared_ones() {
    let runtime: BTreeSet<String> =
        serde_json::from_value(eval(&host(), &[], "return Object.getOwnPropertyNames(globalThis);").await).unwrap();
    let extra: BTreeSet<String> =
        runtime.into_iter().filter(|g| !ECMASCRIPT.contains(&g.as_str()) && !INTERNAL.contains(&g.as_str())).collect();
    assert_eq!(extra, declared_globals(), "globals.d.ts and the host disagree");
}

#[tokio::test]
async fn every_declared_member_exists() {
    let members = declared_members();
    // How to get a real instance of each declared type.
    let samples = r#"
        const doc = new DOMParser().parseFromString('<html><body><p id="x" data-k="v">t</p></body></html>', 'text/html');
        const samples = {
          URL: new URL('https://a/'), URLSearchParams: new URLSearchParams(), Headers: new Headers(),
          FormData: new FormData(), Response: new Response('x'), TextEncoder: new TextEncoder(),
          TextDecoder: new TextDecoder(), AbortController: new AbortController(),
          AbortSignal: new AbortController().signal, Event: new Event('x'), EventTarget: new EventTarget(),
          DOMException: new DOMException('m'), DOMParser: new DOMParser(),
          Console: console, Performance: performance, Crypto: crypto, SubtleCrypto: crypto.subtle,
          DOMDocument: doc, DOMParentNode: doc, DOMNode: doc.getElementById('x'), DOMElement: doc.getElementById('x'),
        };
    "#;
    let body = format!(
        "{samples}
        const declared = {declared};
        const missing = [];
        const has = (target, name) => name === '@@iterator' ? Symbol.iterator in Object(target) : name in Object(target);
        for (const [type, {{ instance, static: statics }}] of Object.entries(declared)) {{
          const sample = samples[type];
          if (sample === undefined) {{ missing.push(type + ': no sample'); continue; }}
          for (const name of instance) if (!has(sample, name)) missing.push(type + '.' + name);
          for (const name of statics) if (!has(globalThis[type], name)) missing.push(type + '.' + name + ' (static)');
        }}
        return missing;",
        declared = serde_json::to_string(&members.iter().filter(|(name, _)| !OPTION_BAGS.contains(&name.as_str())).collect::<BTreeMap<_, _>>()).unwrap(),
    );
    let missing = eval(&host(), &[], &body).await;
    assert_eq!(missing, json!([]), "declared in globals.d.ts but missing at runtime");
}

fn schema_accepts(validator: &jsonschema::Validator, manifest: &Value) -> bool {
    validator.is_valid(manifest)
}

#[test]
fn schema_and_parser_agree() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    let validator = jsonschema::validator_for(&schema).expect("valid JSON Schema");
    let base: Value = serde_json::from_str(TEMPLATE_MANIFEST).unwrap();

    let variant = |path: &[&str], value: Option<Value>| {
        let mut m = base.clone();
        let (last, parents) = path.split_last().unwrap();
        let mut target = &mut m;
        for key in parents {
            target = target.get_mut(*key).unwrap();
        }
        match value {
            Some(v) => target[*last] = v,
            None => {
                target.as_object_mut().unwrap().remove(*last);
            }
        }
        m
    };
    let cases = [
        ("template", base.clone(), true),
        ("homepage", variant(&["homepage"], Some(json!("https://example.com"))), true),
        ("prerelease", variant(&["version"], Some(json!("1.0.0-beta.1"))), true),
        ("wildcard domain", variant(&["permissions", "network", "allowDomains"], Some(json!(["*.example.com"]))), true),
        ("no credentials", variant(&["permissions", "credentials"], None), true),
        ("translated label and help", variant(&["permissions", "credentials"], Some(json!([{
            "key": "token", "label": { "en": "Token", "zh-TW": "權杖" },
            "help": { "en": "Account → API", "ja": "アカウント → API" }, "helpUrl": "https://example.com/help"
        }]))), true),
        ("module help", variant(&["credentialsHelp"], Some(json!({ "en": "Sign in first." }))), true),
        ("module help page", variant(&["credentialsHelpUrl"], Some(json!("https://example.com/help"))), true),
        ("no languages", variant(&["permissions", "credentials"], Some(json!([{ "key": "t", "label": {} }]))), false),
        ("bad language tag", variant(&["permissions", "credentials"], Some(json!([{ "key": "t", "label": { "English": "T" } }]))), false),
        ("blank label", variant(&["permissions", "credentials"], Some(json!([{ "key": "t", "label": "  " }]))), false),
        ("long translated help", variant(&["permissions", "credentials"], Some(json!([{ "key": "t", "label": "T", "help": { "en": "x".repeat(301) } }]))), false),
        ("long module help", variant(&["credentialsHelp"], Some(json!("x".repeat(501)))), false),
        ("help page not https", variant(&["credentialsHelpUrl"], Some(json!("http://example.com/help"))), false),
        ("api 6", variant(&["apiVersion"], Some(json!(6))), false),
        ("unknown field", variant(&["className"], Some(json!("X"))), false),
        ("uppercase id", variant(&["id"], Some(json!("Example"))), false),
        ("no id", variant(&["id"], None), false),
        ("short version", variant(&["version"], Some(json!("1.0"))), false),
        ("leading zero", variant(&["version"], Some(json!("01.0.0"))), false),
        ("empty patterns", variant(&["urlPatterns"], Some(json!([]))), false),
        ("pattern without path", variant(&["urlPatterns"], Some(json!(["https://example.com"]))), false),
        ("ftp pattern", variant(&["urlPatterns"], Some(json!(["ftp://example.com/*"]))), false),
        ("domain with scheme", variant(&["permissions", "network", "allowDomains"], Some(json!(["https://example.com"]))), false),
        ("domain with path", variant(&["permissions", "network", "allowDomains"], Some(json!(["example.com/x"]))), false),
        ("duplicate domain", variant(&["permissions", "network", "allowDomains"], Some(json!(["a.com", "a.com"]))), false),
        ("bad credential key", variant(&["permissions", "credentials"], Some(json!([{ "key": "user-name", "label": "U" }]))), false),
        ("credential without label", variant(&["permissions", "credentials"], Some(json!([{ "key": "user" }]))), false),
        ("entry not js", variant(&["entry"], Some(json!("dist/index.ts"))), false),
        ("absolute entry", variant(&["entry"], Some(json!("/dist/index.js"))), false),
        ("no network", variant(&["permissions", "network"], None), false),
    ];
    for (name, manifest, valid) in cases {
        let parsed = Manifest::parse(&manifest.to_string());
        assert_eq!(schema_accepts(&validator, &manifest), valid, "{name}: schema verdict");
        assert_eq!(parsed.is_ok(), valid, "{name}: parser verdict ({parsed:?})");
    }
}
