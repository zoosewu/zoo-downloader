//! The `zoo-module` command line: `serve` for the app, `new` / `check` / `test` for module
//! authors (most of them AI agents, hence `--json` and hints that say what to change).

mod check;
mod new;
mod project;
mod test;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use tokio::sync::oneshot;
use zoo_module_client::protocol::{Captcha, ModuleError};

use crate::budget::Budget;
use crate::hooks::{FetchOutcome, Hooks, Step};
use crate::runtime::HostOptions;
use project::{Diagnostic, Level};

/// Build and test zoo-downloader site modules.
///
/// Typical loop: edit src/index.ts, `pnpm build`, `zoo-module check`, then `zoo-module test`.
/// Record each case against the real site once with `zoo-module test --record`; later runs
/// replay the recording offline.
#[derive(Parser)]
#[command(name = "zoo-module", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a module from the template (TypeScript, types, AGENTS.md, a test case).
    New {
        /// Directory to create; must be empty or not exist.
        dir: PathBuf,
        /// Module id (lowercase letters, digits, '-'); defaults to the directory name.
        #[arg(long)]
        id: Option<String>,
        /// Display name; defaults to the id.
        #[arg(long)]
        name: Option<String>,
    },
    /// Check manifest.json, the bundle and the test cases without running resolve.
    Check {
        /// Module directory.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Print diagnostics as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run the test cases in tests/cases.json (or one URL) and show every request and result.
    Test {
        /// Module directory.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Resolve this URL instead of the test cases (against the real site, not recorded).
        #[arg(long)]
        url: Option<String>,
        /// Run only the case with this name.
        #[arg(long)]
        case: Option<String>,
        /// Run against the real site and save tests/cassettes/<case>.json.
        #[arg(long)]
        record: bool,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
        /// Let fetch reach 127.0.0.1 (for local fixture sites).
        #[arg(long, hide = true)]
        allow_loopback: bool,
    },
    /// Serve resolve requests over stdio for the app. Not for interactive use.
    Serve {
        #[arg(long, hide = true)]
        allow_loopback: bool,
    },
}

pub fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Serve { allow_loopback } => {
            crate::serve::run(HostOptions { allow_loopback });
            ExitCode::SUCCESS
        }
        Command::New { dir, id, name } => match new::create(&dir, id, name) {
            Ok(id) => {
                println!("created {} ({id})", dir.display());
                println!("next: cd {}; pnpm install; pnpm build; zoo-module check; zoo-module test --record", dir.display());
                println!("AGENTS.md explains the module API and the workflow.");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
        Command::Check { dir, json } => {
            let diagnostics = check::diagnostics(&dir);
            if json {
                let ok = !check::has_errors(&diagnostics);
                println!("{}", serde_json::json!({ "ok": ok, "diagnostics": diagnostics }));
            } else if diagnostics.is_empty() {
                println!("ok");
            } else {
                print_diagnostics(&diagnostics);
            }
            exit(!check::has_errors(&diagnostics))
        }
        Command::Test { dir, url, case, record, json, allow_loopback } => {
            let report = test::run(&dir, &test::Options { url, case, record, allow_loopback });
            if json {
                println!("{}", serde_json::to_string_pretty(&report).expect("reports serialize"));
            } else {
                print_report(&report);
            }
            exit(report.failed == 0 && !check::has_errors(&report.diagnostics))
        }
    }
}

/// `0.3s`, `30s`.
pub(crate) fn seconds(ms: u64) -> String {
    if ms < 10_000 { format!("{:.1}s", ms as f64 / 1000.0) } else { format!("{}s", ms / 1000) }
}

fn exit(ok: bool) -> ExitCode {
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// What to do about a failed call, beyond its message.
fn hint_for(error: &ModuleError) -> Option<String> {
    match error {
        // What the app will do: shown with the module's own hint.
        ModuleError::Script { hint, retry_after: Some(seconds), .. } => Some(format!(
            "{}the app waits {seconds} s (retryAfter) without a download slot, holding this site's other downloads",
            hint.as_ref().map(|h| format!("{h}; ")).unwrap_or_default()
        )),
        ModuleError::Load { hint, .. } | ModuleError::Script { hint, .. } => hint.clone(),
        ModuleError::Timeout { limit_ms } => Some(format!(
            "resolve ran longer than {} s (zoo.wait and captchas do not count): look for a loop that never ends \
             or a request that never answers",
            limit_ms / 1000
        )),
        ModuleError::OutOfMemory { limit_mb } => {
            Some(format!("resolve used more than {limit_mb} MB: avoid keeping many large responses or parsed documents"))
        }
        ModuleError::Crashed { .. } | ModuleError::Cancelled => None,
    }
}

/// For calls that should never wait or ask (`check` loads the module only).
struct NullHooks;

impl Hooks for NullHooks {
    fn waiting(&self, _ms: u64, _reason: Option<String>) -> bool {
        false
    }
    fn ask(&self, _challenge: Captcha) -> oneshot::Receiver<Option<String>> {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(None);
        rx
    }
    fn log(&self, _level: &str, _message: &str) {}
    fn attach(&self, _budget: Arc<Budget>) {}
}

fn print_diagnostics(diagnostics: &[Diagnostic]) {
    for d in diagnostics {
        let level = if d.level == Level::Error { "error" } else { "warning" };
        println!("{level}: {}", d.message);
        if let Some(hint) = &d.hint {
            println!("  hint: {hint}");
        }
    }
}

fn print_report(report: &test::Report) {
    print_diagnostics(&report.diagnostics);
    for case in &report.cases {
        let mode = match case.mode {
            test::Mode::Replay => "replay",
            test::Mode::Record => "record",
            test::Mode::Live => "live",
        };
        println!("{} {}  {}  [{mode}]", if case.passed { "PASS" } else { "FAIL" }, case.name, case.url);
        for step in &case.steps {
            match step {
                Step::Fetch { method, url, outcome, .. } => match outcome {
                    FetchOutcome::Status(status) => println!("  {method} {url} -> {status}"),
                    FetchOutcome::Blocked(why) => println!("  {method} {url} BLOCKED: {why}"),
                    FetchOutcome::Failed(why) => println!("  {method} {url} FAILED: {why}"),
                },
                Step::Log { level, message } => println!("  log {level}: {message}"),
                Step::Wait { ms, reason, skipped } => println!(
                    "  wait {}{}{}",
                    seconds(*ms),
                    reason.as_deref().map(|r| format!(" ({r})")).unwrap_or_default(),
                    if *skipped { " [skipped in replay]" } else { "" }
                ),
                Step::Captcha { challenge, answered } => {
                    println!("  captcha {challenge}: {}", if *answered { "answered" } else { "declined" })
                }
            }
        }
        if let Some(items) = &case.listed {
            println!("  listed {} file(s):", items.len());
            for item in items {
                println!("    {}", serde_json::to_string(item).expect("serializes"));
            }
        }
        if let Some(resolved) = &case.resolved {
            println!("  result: {}", serde_json::to_string(resolved).expect("serializes"));
        }
        if let Some(error) = &case.error {
            println!("  error: {error}");
            if let ModuleError::Script { stack: Some(stack), .. } = error {
                // The module's own frames; host internals (zoo:, ext:) only add noise.
                let frames = stack.lines().skip(1).map(str::trim).filter(|l| !l.contains("(zoo:") && !l.contains("(ext:"));
                for line in frames.take(3) {
                    println!("    {line}");
                }
            }
        }
        if let Some(hint) = &case.hint {
            println!("  hint: {hint}");
        }
        for check in &case.checks {
            let actual = check.actual.as_deref().unwrap_or("nothing");
            if check.ok {
                println!("  ok   expect.{} {:?}", check.field, check.expected);
            } else {
                println!("  FAIL expect.{} {:?}, got {actual:?}", check.field, check.expected);
            }
        }
    }
    if !report.cases.is_empty() {
        println!("{} passed, {} failed", report.passed, report.failed);
    }
}
