//! zoo-core's `Resolver` backed by site modules: the URL goes to the first installed module
//! whose `urlPatterns` match; other URLs download as they are.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use zoo_core::cipher::Decrypt;
use zoo_core::resolve::{Memory, PhaseReport, Resolving};
use zoo_core::{Answers, CoreError, Phase, Resolver, Source};

use crate::manifest::handles;
use crate::protocol::{Captcha, Limits, Listed, ModuleError, ModuleRef};
use crate::supervisor::{Interaction, Supervisor};

/// An enabled module, as the resolver needs it.
#[derive(Debug, Clone)]
pub struct InstalledModule {
    pub module: ModuleRef,
    pub url_patterns: Vec<String>,
    /// The manifest's `urlRegex`: a URL must match it too.
    pub url_regex: Option<regex::Regex>,
    pub limits: Limits,
}

impl InstalledModule {
    pub fn matches(&self, url: &str) -> bool {
        handles(&self.url_patterns, self.url_regex.as_ref(), url)
    }
}

pub struct ModuleResolver {
    supervisor: Arc<Supervisor>,
    /// In priority order: the first match resolves.
    modules: RwLock<Vec<InstalledModule>>,
    /// The user, for countdowns and captchas.
    user: Arc<dyn Interaction>,
}

impl ModuleResolver {
    pub fn new(supervisor: Arc<Supervisor>, user: Arc<dyn Interaction>) -> Self {
        Self { supervisor, modules: RwLock::new(Vec::new()), user }
    }

    /// Replace the enabled modules (after install, removal, reordering or new credentials).
    pub fn set_modules(&self, modules: Vec<InstalledModule>) {
        *self.modules.write().unwrap() = modules;
    }

    /// The module that would resolve `url`, if any.
    pub fn module_for(&self, url: &str) -> Option<InstalledModule> {
        let modules = self.modules.read().unwrap();
        modules.iter().find(|m| m.matches(url)).cloned()
    }

    /// The files behind `url`, each with a URL of its own (see `Request::List`), and what the
    /// user answered the module meanwhile (for the downloads of these files). A URL no module
    /// handles is one file. Every item must go back to the same module, or `resolve` could not
    /// download it.
    pub async fn list(&self, url: &str) -> Result<(Vec<Listed>, Answers), CoreError> {
        let Some(installed) = self.module_for(url) else {
            return Ok((vec![Listed { url: url.into(), filename: None, size: None, folder: None }], Answers::default()));
        };
        let id = installed.module.id.clone();
        let report: PhaseReport = Box::new(|_| {});
        let memory = Memory::default();
        let call = Reporting { user: self.user.as_ref(), report: &report, module: &id, memory: &memory };
        let items = self
            .supervisor
            .list(installed.module, url, installed.limits, &call)
            .await
            .map_err(|e| core_error(&id, e))?;
        if let Some(stray) = items.iter().find(|item| self.module_for(&item.url).is_none_or(|m| m.module.id != id)) {
            let message = format!("{id}: list returned {:?}, which this module's urlPatterns do not match", stray.url);
            return Err(CoreError::Resolve { message, hint: None, retryable: false });
        }
        let answers = memory.lock().unwrap().clone();
        Ok((items, answers))
    }
}

impl Resolver for ModuleResolver {
    fn site(&self, url: &str) -> Option<String> {
        self.module_for(url).map(|installed| installed.module.id)
    }

    fn resolve<'a>(&'a self, url: &'a str, memory: Memory, report: PhaseReport) -> Resolving<'a> {
        Box::pin(async move {
            let Some(installed) = self.module_for(url) else { return Ok(Source::direct(url)) };
            let id = installed.module.id.clone();
            report(Phase::Resolving { module: Some(id.clone()) });
            let call = Reporting { user: self.user.as_ref(), report: &report, module: &id, memory: &memory };
            let resolved = self
                .supervisor
                .resolve(installed.module, url, installed.limits, &call)
                .await
                .map_err(|e| core_error(&id, e))?;
            // The host checked the keys; saving the encrypted bytes is never a fallback.
            let decrypt = match resolved.decrypt.as_ref().map(|e| e.aes128_ctr()).transpose() {
                Ok(keys) => keys.map(|(key, iv)| Decrypt::Aes128Ctr { key, iv }),
                Err(why) => return Err(CoreError::Resolve { message: format!("{id}: {why}"), hint: None, retryable: false }),
            };
            Ok(Source {
                url: resolved.url,
                headers: resolved.headers.into_iter().collect(),
                filename: resolved.filename,
                max_connections: resolved.max_connections.map(|n| n as usize),
                max_segment_size: resolved.max_segment_size,
                expires: true,
                decrypt,
            })
        })
    }
}

/// Passes countdowns and captchas to the user and shows them as the task's phase. A question
/// the user already answered for this download (`zoo.ask` with `remember`) is answered from
/// `memory` without asking, unless the module says that answer did not work.
struct Reporting<'a> {
    user: &'a dyn Interaction,
    report: &'a PhaseReport,
    module: &'a str,
    memory: &'a Memory,
}

impl Interaction for Reporting<'_> {
    fn waiting(&self, ms: u64, reason: Option<&str>) {
        let until = chrono::Utc::now() + Duration::from_millis(ms);
        (self.report)(Phase::Waiting { until, reason: reason.map(String::from) });
        self.user.waiting(ms, reason);
    }

    fn solve_captcha(
        &self,
        challenge: Captcha,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send + '_>> {
        Box::pin(async move {
            let remember = match &challenge {
                Captcha::Ask { id, secret, remember: true, error, .. } => {
                    if error.is_none() {
                        if let Some(known) = self.memory.lock().unwrap().get(id) {
                            return Some(known.to_string());
                        }
                    }
                    Some((id.clone(), *secret))
                }
                _ => None,
            };
            (self.report)(Phase::AwaitingUser);
            let solution = self.user.solve_captcha(challenge).await;
            (self.report)(Phase::Resolving { module: Some(self.module.to_string()) });
            if let (Some((id, secret)), Some(answer)) = (remember, &solution) {
                self.memory.lock().unwrap().set(&id, answer.clone(), secret);
            }
            solution
        })
    }
}

/// How a module failure shows on the task. Failures that may pass on their own (a flaky site,
/// a busy host) are retryable; a broken module is not.
fn core_error(module: &str, error: ModuleError) -> CoreError {
    let resolve = |message: String, hint: Option<String>, retryable| CoreError::Resolve { message, hint, retryable };
    match error {
        ModuleError::Cancelled => CoreError::Cancelled,
        ModuleError::Script { message, hint, retry_after: Some(retry_after), .. } => {
            CoreError::SiteLimit { message: format!("{module}: {message}"), hint, retry_after }
        }
        ModuleError::Script { message, hint, .. } => resolve(format!("{module}: {message}"), hint, true),
        ModuleError::Load { message, hint } => resolve(format!("{module} is broken: {message}"), hint, false),
        ModuleError::OutOfMemory { .. } => resolve(format!("{module}: {error}"), None, false),
        ModuleError::Timeout { .. } | ModuleError::Crashed { .. } => resolve(format!("{module}: {error}"), None, true),
    }
}
