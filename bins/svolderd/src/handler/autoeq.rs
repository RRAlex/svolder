// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! AutoEq in the daemon: the one service that reaches AutoEq's repository, and owns AutoEq's
//! jobs.
//!
//! Placement rule: everything of this family lives here and in the module of the work it runs.
//! [`Daemon`] holds the service in one field and routes `Request::AutoEq` to it
//! in one line of `handle_from` and one of `stream`. The web and MCP reach the family through
//! `Fleet::request` and `Fleet::stream` only: `Fleet` has no method per feature, and a stub
//! answers each in one arm.
//!
//! The service owns AutoEq's job slots, [`JOBS`] at once, the shared state of the ops ([`core`]),
//! serves the stream that follows its jobs, and is stopped first when the daemon stops. `search`
//! and `upload` are plain requests; `preview` and `import` download a file, so they are jobs: the
//! request answers with the job's id and the result rides in the job's `done`. A front end tells
//! whether a daemon has the ops by the feature `autoeq`.
//!
//! The repository the daemon reaches is the [`Client`] it is given ([`Daemon::with_autoeq`]):
//! main reads `SVOLDER_AUTOEQ_URL` and the cache directory once, and a test hands in a loopback
//! server's. A daemon given none answers every op that needs the repository with `internal`.
//!
//! - `core`: the index loaded one at a time, the downloaded corrections, the fits.
//! - `search`: the search answer, grouped by headphone.

mod core;
mod search;

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_autoeq::{Client, Refresh};
use svolder_ipc::config::RefreshWeeks;
use svolder_ipc::{
    AUTOEQ_QUERY_MAX, AutoEqOp, AutoEqPreview, AutoEqSaved, AutoEqTarget, AutoEqVariant,
    CODE_USAGE, Event, Reason, Response,
};
use svolder_profiles::tone::apo;
use svolder_profiles::tone::preset::{self, Imported, Preset};

use self::core::{Core, Failure, Side};
use super::Daemon;
use super::jobs::{JobCtx, JobError, Jobs, Slots};
use crate::writer::Origin;

/// How many AutoEq jobs run at once.
pub(super) const JOBS: usize = 4;

/// AutoEq: its job slots and the state its ops share.
pub(super) struct AutoEqService {
    jobs: Jobs,
    core: Arc<Core>,
}

impl AutoEqService {
    /// A service with no job, running up to [`JOBS`] at a time, reaching the repository through
    /// `client` (none: it reaches nothing).
    pub(super) fn new(client: Option<Client>) -> AutoEqService {
        AutoEqService {
            jobs: Jobs::new(Slots::Up(JOBS)),
            core: Arc::new(Core::new(client)),
        }
    }

    /// The stream `op` turns its connection into, or the answer that refuses it.
    pub(super) fn stream(&self, op: AutoEqOp) -> Result<BoxStream<'static, Event>, Box<Response>> {
        match op {
            AutoEqOp::Follow => Ok(self.jobs.follow().map(Event::AutoEq).boxed()),
            AutoEqOp::Cancel { .. } => Err(Box::new(Jobs::cancel_is_no_stream("auto_eq"))),
            _ => Err(Box::new(Response::error(
                CODE_USAGE,
                "that auto_eq op is not a stream",
            ))),
        }
    }

    /// Cancel AutoEq's jobs and wait for them no longer than `bound`; how many were given up
    /// on.
    pub(super) async fn shutdown(&self, bound: Duration) -> usize {
        self.jobs.stop_all(bound).await
    }

    /// AutoEq's jobs, for the tests that start one.
    #[cfg(test)]
    pub(super) fn jobs(&self) -> &Jobs {
        &self.jobs
    }
}

impl Daemon {
    /// The daemon reaching AutoEq's repository through `client`.
    pub fn with_autoeq(mut self, client: Client) -> Self {
        self.autoeq = AutoEqService::new(Some(client));
        self
    }

    /// Answer one `auto_eq` op for `origin`. The follow op is a stream, which this refuses as
    /// `handle_from` refuses every stream.
    pub(super) async fn autoeq_op(&self, origin: Origin, op: AutoEqOp) -> Response {
        match op {
            AutoEqOp::Cancel { job } => self.autoeq.jobs.cancel_answer(job),
            AutoEqOp::Follow => Jobs::follow_needs_a_stream("auto_eq"),
            AutoEqOp::Search { query, refresh } => self.autoeq_search(&query, refresh).await,
            AutoEqOp::Preview {
                variant,
                target,
                compare,
            } => self.autoeq_preview(origin, variant, target, compare),
            AutoEqOp::Import {
                variant,
                name,
                force,
                target,
            } => self.autoeq_import(origin, variant, name, force, target),
            AutoEqOp::Upload { text, name, force } => self.autoeq_upload(&text, &name, force).await,
        }
    }

    /// When the index is fetched again: now for `refresh`, else as the config's
    /// `[autoeq] refresh_weeks` says.
    fn autoeq_policy(&self, refresh: bool) -> Refresh {
        let weeks = match self.config_snapshot().autoeq.refresh_weeks {
            RefreshWeeks::Never => None,
            RefreshWeeks::Weeks(weeks) => Some(weeks),
        };
        Refresh::of(refresh, weeks)
    }

    async fn autoeq_search(&self, query: &str, refresh: bool) -> Response {
        if query.chars().count() > AUTOEQ_QUERY_MAX {
            return Response::error(
                CODE_USAGE,
                format!("a search is at most {AUTOEQ_QUERY_MAX} characters"),
            )
            .with_reason(Reason::Invalid);
        }
        match self.autoeq.core.index(self.autoeq_policy(refresh)).await {
            Ok(index) => Response::AutoEqSearch(Box::new(search::answer(&index, query))),
            Err(failure) => failure.answer(),
        }
    }

    /// The side `target` names, judged now: a device that is configured and has a tone control
    /// (its profile is the live one, or the registry's while it is offline), or the computer.
    fn autoeq_side(&self, target: &AutoEqTarget) -> Result<Side, Failure> {
        let AutoEqTarget::Device { device } = target else {
            return Ok(Side::Computer);
        };
        let handle = self
            .resolve(Some(device))
            .map_err(|refused| Failure::new(refused.message, Reason::Missing))?;
        let info = handle.describe();
        // The live profile; while the device is not found, the one the config names for it, or
        // the registry's under the name the actor last resolved.
        let live = handle.feed.borrow().profile.clone();
        let registry = self.registry();
        let named = self
            .config_snapshot()
            .devices
            .iter()
            .find(|configured| configured.name == info.name)
            .and_then(|configured| configured.profile.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or(info.profile.clone());
        let profile = match live {
            Some(profile) => Some((*profile).clone()),
            None => registry.get(&named).cloned(),
        };
        let Some(profile) = profile else {
            return Err(Failure::new(
                svolder_ipc::not_found_yet(&info.name),
                Reason::NotFoundYet,
            ));
        };
        profile.eq.map(Side::Device).ok_or_else(|| {
            Failure::new(
                format!("{} has no tone control", info.name),
                Reason::Invalid,
            )
        })
    }

    fn autoeq_preview(
        &self,
        origin: Origin,
        variant: Box<AutoEqVariant>,
        target: AutoEqTarget,
        compare: Option<Box<AutoEqVariant>>,
    ) -> Response {
        let side = match self.autoeq_side(&target) {
            Ok(side) => side,
            Err(failure) => return failure.answer(),
        };
        let (core, policy) = (Arc::clone(&self.autoeq.core), self.autoeq_policy(false));
        self.autoeq
            .jobs
            .start_answer("AutoEq job", origin.who(), move |ctx| async move {
                let work = async {
                    let first = core.entry(&variant, policy).await?;
                    let second = match &compare {
                        Some(other) => Some(core.entry(other, policy).await?),
                        None => None,
                    };
                    let parsed = core.correction(&first).await?;
                    let preview = core
                        .preview(side.clone(), parsed.correction.clone())
                        .await?;
                    let compare = match second {
                        Some(entry) => {
                            let parsed = core.correction(&entry).await?;
                            Some(core.preview(side, parsed.correction.clone()).await?)
                        }
                        None => None,
                    };
                    Ok::<_, Failure>(AutoEqPreview { preview, compare })
                };
                raced(&ctx, work).await
            })
    }

    fn autoeq_import(
        &self,
        origin: Origin,
        variant: Box<AutoEqVariant>,
        name: Option<String>,
        force: bool,
        target: AutoEqTarget,
    ) -> Response {
        let side = match self.autoeq_side(&target) {
            Ok(side) => side,
            Err(failure) => return failure.answer(),
        };
        let Some(daemon) = self.me.get().and_then(std::sync::Weak::upgrade) else {
            return Failure::new("the daemon was not made shared", Reason::Internal).answer();
        };
        let (core, policy) = (Arc::clone(&self.autoeq.core), self.autoeq_policy(false));
        self.autoeq
            .jobs
            .start_answer("AutoEq job", origin.who(), move |ctx| async move {
                // Everything before the save can be cancelled; the save cannot, so a preset that
                // was written is answered as saved.
                let prepare = async {
                    let entry = core.entry(&variant, policy).await?;
                    let parsed = core.correction(&entry).await?;
                    let named = name
                        .map(|name| name.trim().to_string())
                        .filter(|name| !name.is_empty())
                        .or_else(|| {
                            Some(preset::default_name(&entry.name)).filter(|name| !name.is_empty())
                        })
                        .unwrap_or_else(|| entry.name.clone());
                    let imported = Imported {
                        origin: Some(entry.origin()),
                        correction: parsed.correction.clone(),
                    };
                    if matches!(side, Side::Computer)
                        && let Some(why) =
                            preset::computer_refusal(&Preset::imported(&named, imported.clone()))
                    {
                        return Err(Failure::new(why, Reason::Invalid));
                    }
                    Ok((named, imported, parsed.notes.len()))
                };
                let (named, imported, skipped) = tokio::select! {
                    prepared = prepare => prepared.map_err(Failure::job)?,
                    () = ctx.cancelled() => return Err(JobError::Cancelled),
                };
                let saved = daemon
                    .autoeq_save(&named, imported, skipped, force)
                    .await
                    .map_err(Failure::job)?;
                serde_json::to_value(saved)
                    .map_err(|error| JobError::failed(error.to_string(), Reason::Internal))
            })
    }

    async fn autoeq_upload(&self, text: &str, name: &str, force: bool) -> Response {
        if text.len() > apo::MAX_BYTES {
            return Failure::new(
                format!("the text is more than the {} bytes read", apo::MAX_BYTES),
                Reason::Invalid,
            )
            .answer();
        }
        let parsed = match apo::parse(text) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Failure::new(
                    format!("the text is not a parametric EQ: {error}"),
                    Reason::Invalid,
                )
                .answer();
            }
        };
        let imported = Imported {
            origin: None,
            correction: parsed.correction,
        };
        match self
            .autoeq_save(name.trim(), imported, parsed.notes.len(), force)
            .await
        {
            Ok(saved) => Response::AutoEqSaved(saved),
            Err(failure) => failure.answer(),
        }
    }

    /// Save `imported` as the user's preset `name`; refused in the configuration writer's words
    /// and reasons.
    async fn autoeq_save(
        &self,
        name: &str,
        imported: Imported,
        skipped: usize,
        force: bool,
    ) -> Result<AutoEqSaved, Failure> {
        let filters = imported.correction.filters.len();
        let saved = self
            .save_preset(name, Preset::imported(name, imported), force)
            .await
            .map_err(|error| match error.answer() {
                Response::Error {
                    message, reason, ..
                } => Failure::new(message, reason.unwrap_or(Reason::Internal)),
                _ => Failure::new("the preset was not saved", Reason::Internal),
            })?;
        Ok(AutoEqSaved {
            name: saved,
            filters,
            skipped,
        })
    }
}

/// Run a job's `work` until it ends or the job is asked to stop, and give its result as the
/// job's.
async fn raced<T: serde::Serialize>(
    ctx: &JobCtx,
    work: impl std::future::Future<Output = Result<T, Failure>>,
) -> Result<serde_json::Value, JobError> {
    let done = tokio::select! {
        done = work => done.map_err(Failure::job)?,
        () = ctx.cancelled() => return Err(JobError::Cancelled),
    };
    serde_json::to_value(done)
        .map_err(|error| JobError::failed(error.to_string(), Reason::Internal))
}

#[cfg(test)]
mod tests;
