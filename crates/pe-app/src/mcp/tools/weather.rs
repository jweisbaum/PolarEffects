//! The weather group (spec.md 3.7): reanalysis wind, waves and current for
//! a track's positions. The one group whose tools start a download, on the
//! person's agent's asking, through `pe-env` as the interface's own Fetch
//! weather does (invariant 4).

use std::time::Duration;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::Manager;

use super::{BoatParams, PolarExplorer, ToolResult, boat_id, json};
use crate::commands::AppState;
use crate::env::EnvJobsStatus;

/// How often `weather_fetch` looks at the job queue.
const POLL: Duration = Duration::from_millis(500);

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeatherSourcesParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The track sources to fetch for, by id from sources_list.
    pub sources: Vec<u64>,
    /// Fetch everything again, not only what is missing.
    #[serde(default)]
    pub restart: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeatherFetchParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The track sources to fetch for, by id from sources_list.
    pub sources: Vec<u64>,
    /// Only "hourly" is supported. Omit to use hourly sampling.
    #[serde(default)]
    pub interval: Option<String>,
    /// Fetch everything again, not only what is missing.
    #[serde(default)]
    pub restart: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WeatherCancelParams {
    /// The boat's id, from boats_list. Without it, the first boat.
    #[serde(default)]
    pub boat: Option<u64>,
    /// The tracks whose fetches to cancel; without it, every one of the
    /// boat's.
    #[serde(default)]
    pub sources: Option<Vec<u64>>,
}

/// Cancels a fetch whose tool call ended without it finishing: the client
/// went away or cancelled the request (invariant 6), or the queue could not
/// be read. Disarmed when the fetch runs to its end.
struct CancelOnDrop<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    /// The boat's own id, never "every boat": source ids are each boat's.
    boat: u64,
    sources: Vec<u64>,
    armed: bool,
}

impl<R: tauri::Runtime> Drop for CancelOnDrop<R> {
    fn drop(&mut self) {
        if self.armed {
            // The command itself, so the interface's queue display hears it.
            let _ = crate::env::cancel_env_fetch(
                self.app.clone(),
                self.app.state::<AppState>(),
                Some(self.boat),
                Some(std::mem::take(&mut self.sources)),
            );
        }
    }
}

/// Whether a queued or running fetch is `boat`'s fetch of one of `sources`.
/// Source ids are each boat's own, so another boat's source 1 is not this
/// boat's.
fn ours(track: &crate::env::EnvJobTrack, boat: u64, sources: &[u64]) -> bool {
    track.boat_id == Some(boat) && sources.contains(&track.source_id)
}

/// Whether any of `boat`'s `sources` is still queued or fetching.
fn pending(status: &EnvJobsStatus, boat: u64, sources: &[u64]) -> bool {
    status.tracks.iter().any(|track| ours(track, boat, sources))
}

#[tool_router(router = tool_router_weather, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "How much fetching weather for these tracks would download, worked out from their positions without fetching anything: the samples to fetch, the hourly download size and what the cache already holds. Call it before weather_fetch and tell the user when the download is large."
    )]
    async fn weather_estimate(
        &self,
        Parameters(p): Parameters<WeatherSourcesParams>,
    ) -> ToolResult {
        let estimate = self
            .run("weather_estimate", move |app| {
                tauri::async_runtime::block_on(crate::env::env_estimate(
                    app.state(),
                    p.boat,
                    p.sources,
                    p.restart,
                ))
            })
            .await?;
        json(&estimate)
    }

    #[tool(
        description = "Fetches reanalysis wind, waves and current for the given tracks' positions: a download from the public ERA5 and ocean-current archives, which gives each sample its place in the polar. Waits until those tracks are done, reporting progress, and answers the job queue's final state (a `failure` there names what went wrong) and the `project`. Cancelling the call cancels the fetch; what was already fetched is kept."
    )]
    async fn weather_fetch(
        &self,
        Parameters(p): Parameters<WeatherFetchParams>,
        ctx: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> ToolResult {
        let (boat, sources, restart) = (p.boat, p.sources.clone(), p.restart);
        let interval = p.interval;
        let (owner, queued) = {
            let sources = sources.clone();
            self.run("weather_fetch", move |app| {
                let owner = boat_id(&app.state::<AppState>(), boat)?;
                let interval = interval.unwrap_or_else(|| "hourly".into());
                let queued = crate::env::start_env_fetch(
                    app.clone(),
                    app.state(),
                    boat,
                    sources,
                    interval,
                    restart,
                )?;
                Ok((owner, queued))
            })
            .await?
        };
        let mut guard = CancelOnDrop {
            app: self.app.clone(),
            boat: owner,
            sources: sources.clone(),
            armed: true,
        };

        let token = ctx.meta.get_progress_token();
        let mut status = queued;
        let mut told = -1.0;
        while pending(&status, owner, &sources) {
            // The fraction done over these tracks: a finished one has left
            // the queue, so it counts whole.
            let waiting: f64 = status
                .tracks
                .iter()
                .filter(|track| ours(track, owner, &sources))
                .map(|track| 1.0 - track.fraction)
                .sum();
            let done = sources.len() as f64 - waiting;
            if let Some(token) = &token
                && done != told
            {
                told = done;
                let mut param = rmcp::model::ProgressNotificationParam::new(token.clone(), done);
                param.total = Some(sources.len() as f64);
                param.message = status
                    .tracks
                    .iter()
                    .find(|track| ours(track, owner, &sources))
                    .map(|track| track.label.clone());
                let _ = ctx.peer.notify_progress(param).await;
            }
            // `rmcp` does not drop this future when the client cancels the
            // request or goes away: it cancels the request's token, which a
            // long tool has to watch. Returning here drops `guard`, armed,
            // which cancels the fetch (invariant 6).
            tokio::select! {
                () = ctx.ct.cancelled() => {
                    return Err(super::ToolError::Refused(
                        "the weather fetch was cancelled with its request; what was already fetched is kept".to_owned(),
                    ));
                }
                () = tokio::time::sleep(POLL) => {}
            }
            status =
                crate::env::env_jobs(self.app.state(), boat).map_err(super::ToolError::from)?;
        }
        guard.armed = false;

        // The fetch wrote samples into the project: the frontend heard it
        // from the job itself, and the client reads the result here.
        let project = self
            .run("weather_fetch", move |app| {
                crate::projects::project_summary(app.state(), boat)
            })
            .await?;
        json(&serde_json::json!({ "jobs": status, "project": project }))
    }

    #[tool(
        description = "Cancels the weather fetches of the given tracks of the boat, or all of that boat's. What was already fetched is kept."
    )]
    async fn weather_cancel(&self, Parameters(p): Parameters<WeatherCancelParams>) -> ToolResult {
        let status = self
            .run("weather_cancel", move |app| {
                // The boat by its id: the command reads no boat as every
                // boat's fetches, and a tool's missing boat is the first.
                let boat = boat_id(&app.state::<AppState>(), p.boat)?;
                crate::env::cancel_env_fetch(app.clone(), app.state(), Some(boat), p.sources)
            })
            .await?;
        json(&status)
    }

    #[tool(
        description = "The weather job queue: the track being fetched and how far along it is, the ones waiting, and the last `failure` or `warning`."
    )]
    async fn weather_jobs(&self, Parameters(p): Parameters<BoatParams>) -> ToolResult {
        let status = self
            .run("weather_jobs", move |app| {
                crate::env::env_jobs(app.state(), p.boat)
            })
            .await?;
        json(&status)
    }
}
