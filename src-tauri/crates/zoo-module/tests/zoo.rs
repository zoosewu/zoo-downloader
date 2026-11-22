//! The `zoo` API: countdowns, captchas and credentials, through a mock user.

mod common;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use common::{limits, loopback_host, module, module_allowing, Resp, Server};
use tempfile::TempDir;
use zoo_module_client::{Captcha, Interaction, ModuleError, Secret};

/// Answers every captcha with `answer` after `delay`, and records what it was shown.
#[derive(Default)]
struct User {
    answer: Option<String>,
    delay: Duration,
    waits: Mutex<Vec<(u64, Option<String>)>>,
    challenges: Mutex<Vec<Captcha>>,
}

impl Interaction for User {
    fn waiting(&self, ms: u64, reason: Option<&str>) {
        self.waits.lock().unwrap().push((ms, reason.map(String::from)));
    }
    fn solve_captcha(&self, challenge: Captcha) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        self.challenges.lock().unwrap().push(challenge);
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            self.answer.clone()
        })
    }
}

#[tokio::test]
async fn waits_are_reported_and_do_not_count_against_the_limit() {
    let dir = TempDir::new().unwrap();
    let m = module(&dir, "wait", "export default { async resolve(url, zoo) { await zoo.wait(0.6, 'free user'); return { url: 'https://x/' }; } };");
    let user = User::default();
    let started = Instant::now();
    let resolved = loopback_host().resolve(m, "https://a/", limits(300), &user).await.unwrap();
    assert_eq!(resolved.url, "https://x/");
    assert!(started.elapsed() >= Duration::from_millis(600));
    assert_eq!(*user.waits.lock().unwrap(), vec![(600, Some("free user".to_string()))]);
}

#[tokio::test]
async fn recaptcha_is_answered_by_the_user_outside_the_time_limit() {
    let dir = TempDir::new().unwrap();
    let m = module(
        &dir,
        "recaptcha",
        "export default { async resolve(url, zoo) {
            const token = await zoo.captcha.recaptcha({ siteKey: 'site-key', pageUrl: url });
            return { url: 'https://x/?t=' + token };
        } };",
    );
    // The user takes longer than the module's whole time limit.
    let user = User { answer: Some("solved".into()), delay: Duration::from_millis(800), ..Default::default() };
    let resolved = loopback_host().resolve(m, "https://page/", limits(300), &user).await.unwrap();
    assert_eq!(resolved.url, "https://x/?t=solved");
    assert_eq!(
        *user.challenges.lock().unwrap(),
        vec![Captcha::Recaptcha { site_key: "site-key".into(), page_url: "https://page/".into() }]
    );
}

#[tokio::test]
async fn ask_passes_the_question_and_returns_the_answer() {
    let dir = TempDir::new().unwrap();
    let m = module(
        &dir,
        "ask",
        "export default { async resolve(url, zoo) {
            const code = await zoo.ask({ id: 'pwd', message: { en: 'Code', 'zh-TW': '提取碼' }, remember: true, error: 'wrong code' });
            return { url: 'https://x/?pwd=' + code };
        } };",
    );
    let user = User { answer: Some("8k3t".into()), ..Default::default() };
    let resolved = loopback_host().resolve(m, "https://page/", limits(5_000), &user).await.unwrap();
    assert_eq!(resolved.url, "https://x/?pwd=8k3t");
    let message = zoo_module_client::manifest::Text::Translated([("en".into(), "Code".into()), ("zh-TW".into(), "提取碼".into())].into());
    assert_eq!(
        *user.challenges.lock().unwrap(),
        vec![Captcha::Ask { id: "pwd".into(), message, secret: false, remember: true, error: Some("wrong code".into()) }]
    );

    // Declining fails the call; a bad id says what is wrong.
    let declined = module(&dir, "ask-declined", "export default { async resolve(url, zoo) { await zoo.ask({ id: 'pwd', message: 'Code' }); } };");
    match loopback_host().resolve(declined, "https://page/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("did not answer"), "{message}"),
        other => panic!("expected script error, got {other:?}"),
    }
    let bad = module(&dir, "ask-bad", "export default { async resolve(url, zoo) { await zoo.ask({ id: 'a b', message: 'Code' }); } };");
    match loopback_host().resolve(bad, "https://page/", limits(5_000), &user).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("id \"a b\""), "{message}"),
        other => panic!("expected script error, got {other:?}"),
    }
}

#[tokio::test]
async fn ask_needs_api_level_3() {
    let dir = TempDir::new().unwrap();
    let m = zoo_module_client::ModuleRef {
        api_version: 2,
        ..module(&dir, "old", "export default { async resolve(url, zoo) { await zoo.ask({ id: 'pwd', message: 'Code' }); } };")
    };
    let user = User { answer: Some("x".into()), ..Default::default() };
    match loopback_host().resolve(m, "https://page/", limits(5_000), &user).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("\"apiVersion\": 3"), "{message}"),
        other => panic!("expected script error, got {other:?}"),
    }
    assert!(user.challenges.lock().unwrap().is_empty(), "the user was not asked");
}

#[tokio::test]
async fn decrypt_reaches_the_app_from_api_level_4() {
    let dir = TempDir::new().unwrap();
    let code = "export default { async resolve() { return { url: 'https://cdn.example/f', decrypt: { cipher: 'aes-128-ctr', key: '00'.repeat(16), iv: 'ff'.repeat(16) } }; } };";
    let resolved = loopback_host().resolve(module(&dir, "mega", code), "https://page/", limits(5_000), &User::default()).await.unwrap();
    assert_eq!(resolved.decrypt.unwrap().aes128_ctr().unwrap(), ([0; 16], [0xff; 16]));

    let old = zoo_module_client::ModuleRef { api_version: 3, ..module(&dir, "old", code) };
    match loopback_host().resolve(old, "https://page/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("\"apiVersion\": 4"), "{message}"),
        other => panic!("expected script error, got {other:?}"),
    }
    let bad = module(&dir, "bad", &code.replace("'00'.repeat(16)", "'zz'"));
    match loopback_host().resolve(bad, "https://page/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("decrypt.key"), "{message}"),
        other => panic!("expected script error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_site_limit_tells_the_app_when_to_try_again() {
    let dir = TempDir::new().unwrap();
    let code = "export default { async resolve(url, zoo) { throw zoo.fail('one free download every 2 hours', { retryAfter: 59.2 }); } };";
    match loopback_host().resolve(module(&dir, "limited", code), "https://page/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { message, retry_after, .. }) => {
            assert_eq!(message, "one free download every 2 hours");
            assert_eq!(retry_after, Some(60), "whole seconds, rounded up");
        }
        other => panic!("expected a site limit, got {other:?}"),
    }
    let old = zoo_module_client::ModuleRef { api_version: 4, ..module(&dir, "old", code) };
    match loopback_host().resolve(old, "https://page/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { message, retry_after: None, .. }) => assert!(message.contains("\"apiVersion\": 5"), "{message}"),
        other => panic!("expected a plain script error, got {other:?}"),
    }
    let bad = module(&dir, "bad", &code.replace("59.2", "-1"));
    match loopback_host().resolve(bad, "https://page/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("retryAfter must be a positive"), "{message}"),
        other => panic!("expected a script error, got {other:?}"),
    }
}

#[tokio::test]
async fn declined_captcha_fails_the_call() {
    let dir = TempDir::new().unwrap();
    let m = module(&dir, "declined", "export default { async resolve(url, zoo) { await zoo.captcha.recaptcha({ siteKey: 'k', pageUrl: url }); } };");
    match loopback_host().resolve(m, "https://page/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { message, .. }) => assert!(message.contains("not solved"), "{message}"),
        other => panic!("expected script error, got {other:?}"),
    }
}

#[tokio::test]
async fn image_captcha_is_fetched_through_the_session() {
    let server = Server::start(|req| match req.path.as_str() {
        "/page" => Resp::ok("page").header("Set-Cookie", "sid=7"),
        "/captcha.png" if req.headers.get("cookie").map(String::as_str) == Some("sid=7") => {
            Resp::ok(vec![1u8, 2, 3]).header("Content-Type", "image/png")
        }
        _ => Resp { status: 403, headers: vec![], body: vec![] },
    })
    .await;
    let dir = TempDir::new().unwrap();
    let m = module_allowing(
        &dir,
        "image",
        &format!(
            "export default {{ async resolve(url, zoo) {{
                await fetch('{base}/page');
                return {{ url: 'https://x/?c=' + await zoo.captcha.image('{base}/captcha.png') }};
            }} }};",
            base = server.url("")
        ),
        &["127.0.0.1"],
    );
    let user = User { answer: Some("abc".into()), ..Default::default() };
    let resolved = loopback_host().resolve(m, "https://a/", limits(5_000), &user).await.unwrap();
    assert_eq!(resolved.url, "https://x/?c=abc");
    assert_eq!(*user.challenges.lock().unwrap(), vec![Captcha::Image { mime: "image/png".into(), base64: "AQID".into() }]);
}

#[tokio::test]
async fn credentials_reach_the_site_but_never_the_module() {
    let server = Server::start(|req| match req.path.as_str() {
        "/login" => Resp::ok("welcome alice, your password is p&ss wörd").header("Set-Cookie", "sid=1; Path=/"),
        _ => Resp::ok("ok"),
    })
    .await;
    let base = server.url("");
    let dir = TempDir::new().unwrap();
    let mut m = module_allowing(
        &dir,
        "login",
        &format!(
            r#"export default {{ async resolve(url, zoo) {{
                const user = zoo.credentials.placeholder('user');
                const pass = zoo.credentials.placeholder('pass');
                const token = zoo.credentials.placeholder('token');
                const page = await (await fetch('{base}/login', {{ method: 'POST', body: new URLSearchParams({{ user, pass }}) }})).text();
                await fetch('{base}/api?k=' + token, {{
                  method: 'POST',
                  headers: {{ 'Content-Type': 'application/json', 'X-Token': token }},
                  body: JSON.stringify({{ pass }}),
                }});
                return {{
                  url: '{base}/file?key=' + token,
                  headers: {{ 'X-Auth': token }},
                  filename: JSON.stringify({{
                    leaked: page.includes('p&ss'),
                    scrubbed: page.includes(pass),
                    has: [zoo.credentials.has('user'), zoo.credentials.has('nope')],
                  }}),
                }};
            }} }};"#
        ),
        &["127.0.0.1"],
    );
    m.grants.credentials = BTreeMap::from([
        ("user".into(), Secret("alice".into())),
        ("pass".into(), Secret("p&ss wörd".into())),
        ("token".into(), Secret("tok/12 345".into())),
    ]);

    let resolved = loopback_host().resolve(m, "https://a/", limits(5_000), &User::default()).await.unwrap();

    let seen: serde_json::Value = serde_json::from_str(resolved.filename.as_deref().unwrap()).unwrap();
    assert_eq!(seen, serde_json::json!({ "leaked": false, "scrubbed": true, "has": [true, false] }));

    let requests = server.requests();
    let login = requests.iter().find(|r| r.path == "/login").unwrap();
    assert_eq!(String::from_utf8_lossy(&login.body), "user=alice&pass=p%26ss+w%C3%B6rd", "form-encoded");
    let api = requests.iter().find(|r| r.path.starts_with("/api")).unwrap();
    assert_eq!(api.path, "/api?k=tok%2F12%20345", "percent-encoded in the url");
    assert_eq!(api.headers["x-token"], "tok/12 345", "verbatim in headers");
    assert_eq!(String::from_utf8_lossy(&api.body), r#"{"pass":"p&ss wörd"}"#, "JSON-escaped in JSON");

    // The app gets real values for the download, plus the session cookie.
    assert_eq!(resolved.url, format!("{base}/file?key=tok%2F12%20345"));
    assert_eq!(resolved.headers["X-Auth"], "tok/12 345");
    assert_eq!(resolved.headers["Cookie"], "sid=1");
}

#[tokio::test]
async fn missing_credentials_say_how_to_fix() {
    let dir = TempDir::new().unwrap();
    let m = module(&dir, "nocred", "export default { resolve(url, zoo) { zoo.credentials.placeholder('password'); } };");
    match loopback_host().resolve(m, "https://a/", limits(5_000), &User::default()).await {
        Err(ModuleError::Script { hint: Some(hint), .. }) => assert!(hint.contains("permissions.credentials"), "{hint}"),
        other => panic!("expected script error with hint, got {other:?}"),
    }
}
