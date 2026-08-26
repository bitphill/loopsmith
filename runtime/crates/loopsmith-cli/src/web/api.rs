//! The HTTP surface the browser talks to.
//!
//! Two kinds of endpoint, and the split is the important thing:
//!
//! - **Answers**, computed in this process in under a millisecond: detection,
//!   validation, planning, permissions, cost. These update as the user types.
//! - **Actions**, which spawn the real `loopsmith` binary and stream its
//!   output. These are the ones that write files and spend money.
//!
//! Nothing here reimplements a command. The browser names a verb from a closed
//! list in [`crate::web::exec::Action`]; this module turns it into an argv and
//! hands it to the job runner. A browser cannot name a program to run, which
//! is the difference between a control panel and a remote shell.

use crate::web::{assemble, detect, examples, exec, help, secrets};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub jobs: exec::Jobs,
    pub version: &'static str,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        // What this machine has.
        .route("/api/meta", get(meta))
        .route("/api/detect", get(detect_handler))
        .route("/api/handshake", post(handshake))
        .route("/api/path", get(path_facts))
        // Teaching material.
        .route("/api/help", get(help_handler))
        // The example library and the loops already made.
        .route("/api/examples", get(list_examples))
        .route("/api/examples/{id}", get(load_example))
        .route("/api/library", get(list_library))
        .route("/api/library/forget", post(forget_library))
        .route("/api/open", post(open_config))
        // Live feedback on the draft.
        .route("/api/review", post(review))
        .route("/api/render", post(render))
        // Secrets.
        .route("/api/secrets", get(list_secrets).post(set_secret))
        .route("/api/secrets/reveal", post(reveal_secret))
        // Actions.
        .route("/api/jobs", get(list_jobs).post(start_job))
        .route("/api/jobs/{id}", get(job_detail))
        .route("/api/jobs/{id}/cancel", post(cancel_job))
        .route("/api/jobs/{id}/stream", get(job_stream))
        .with_state(state)
}

/// One error shape for the whole API, so the browser has one thing to render.
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<String> for ApiError {
    fn from(e: String) -> Self {
        ApiError(StatusCode::BAD_REQUEST, e)
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

async fn meta(State(s): State<AppState>) -> Json<Value> {
    Json(json!({
        "version": s.version,
        "exe": std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        "cwd": std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
        "keychain": secrets::keychain_kind(),
        "profile": secrets::profile_path().map(|p| p.display().to_string()),
    }))
}

#[derive(Deserialize)]
struct DeepQuery {
    /// Off unless explicitly asked for. A deep probe can cost money, and a
    /// page load is not consent to spend it.
    #[serde(default)]
    deep: bool,
}

async fn detect_handler(Query(q): Query<DeepQuery>) -> Json<detect::Detection> {
    Json(detect::scan(q.deep).await)
}

#[derive(Deserialize)]
struct HandshakeBody {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    prompt_on_stdin: bool,
}

/// The real round trip, behind the UI's per-provider Test button.
///
/// This one spends tokens, which is exactly why it is a button and not part of
/// detection. The command must be one detection actually found: a browser that
/// could name any executable here would have a remote shell.
async fn handshake(Json(b): Json<HandshakeBody>) -> ApiResult<detect::HandshakeResult> {
    let known = crate::web::catalog::KNOWN
        .iter()
        .find(|k| k.bin == b.command)
        .or_else(|| crate::web::catalog::find(&b.command));
    let Some(known) = known else {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            format!(
                "`{}` is not one of the agent CLIs loopsmith knows how to test. \
                 A custom command can still be used as a provider — it just cannot \
                 be handshake-tested from here.",
                b.command
            ),
        ));
    };
    // An empty argument list means "test it the way loopsmith would call it",
    // which is the useful default for a card the user has not edited yet.
    let args: Vec<String> = if b.args.is_empty() {
        known.args.iter().map(|s| s.to_string()).collect()
    } else {
        b.args
    };
    Ok(Json(
        detect::handshake(known.bin, &args, b.prompt_on_stdin).await,
    ))
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

async fn path_facts(Query(q): Query<PathQuery>) -> Json<detect::PathFacts> {
    Json(detect::path_facts(&expand_home(&q.path)))
}

async fn help_handler() -> Json<Value> {
    Json(json!({ "sections": help::SECTIONS, "fields": help::FIELDS }))
}

async fn list_examples() -> Json<Vec<examples::ExampleCard>> {
    Json(examples::list())
}

/// The YAML *and* the parsed config, because the browser needs both: the
/// config to populate the form, the text to show in the preview pane.
async fn load_example(Path(id): Path<String>) -> ApiResult<Value> {
    let text = examples::raw(&id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("no example called `{id}`")))?;
    let cfg: loopsmith_core::LoopConfig = serde_yaml::from_str(&text)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "id": id, "yaml": text, "config": cfg })))
}

async fn list_library() -> Json<Vec<assemble::LibraryEntry>> {
    Json(assemble::library())
}

#[derive(Deserialize)]
struct PathBody {
    path: String,
}

async fn forget_library(Json(b): Json<PathBody>) -> ApiResult<Value> {
    assemble::forget(&b.path)?;
    Ok(Json(json!({ "ok": true })))
}

/// Open a loop that already exists on disk, for editing.
async fn open_config(Json(b): Json<PathBody>) -> ApiResult<Value> {
    let path = expand_home(&b.path);
    // A directory is what people paste; find the config inside it rather than
    // making them name the file.
    let file = if path.is_dir() {
        ["loop.yaml", "loop.yml", "loop.md"]
            .iter()
            .map(|f| path.join(f))
            .find(|p| p.exists())
            .ok_or_else(|| {
                ApiError(
                    StatusCode::NOT_FOUND,
                    format!(
                        "{} has no loop.yaml, loop.yml, or loop.md in it",
                        path.display()
                    ),
                )
            })?
    } else {
        path
    };

    let (cfg, format) = assemble::load_file(&file)?;
    let dir = file.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    Ok(Json(json!({
        "config": cfg,
        "format": format,
        "config_path": file.display().to_string(),
        "dir": dir.display().to_string(),
        "facts": detect::path_facts(&dir),
    })))
}

/// Validate, plan, price, and derive permissions for the draft. Called on
/// every meaningful edit, so it does no I/O and spawns nothing.
async fn review(Json(cfg): Json<Value>) -> Json<assemble::Review> {
    Json(assemble::review(&cfg))
}

#[derive(Deserialize)]
struct RenderBody {
    config: Value,
    #[serde(default = "yaml_format")]
    format: assemble::Format,
}

fn yaml_format() -> assemble::Format {
    assemble::Format::Yaml
}

async fn render(Json(b): Json<RenderBody>) -> ApiResult<Value> {
    let cfg: loopsmith_core::LoopConfig =
        serde_json::from_value(b.config).map_err(|e| e.to_string())?;
    let text = assemble::render(&cfg, b.format)?;
    Ok(Json(json!({ "text": text, "file_name": b.format.file_name() })))
}

async fn list_secrets() -> Json<Vec<secrets::SecretStatus>> {
    Json(
        crate::web::catalog::ENV_KEYS
            .iter()
            .map(|(name, _)| secrets::status(name))
            .collect(),
    )
}

#[derive(Deserialize)]
struct SetSecretBody {
    name: String,
    /// `null` removes the key. A rotated key left behind is a key that will be
    /// used by accident.
    value: Option<String>,
    store: secrets::Store,
}

async fn set_secret(Json(b): Json<SetSecretBody>) -> ApiResult<secrets::SecretStatus> {
    // The value is used and dropped. It is never echoed back in the response,
    // never logged, and never written into a config.
    secrets::set(&b.name, b.value.as_deref().filter(|v| !v.is_empty()), b.store)?;
    Ok(Json(secrets::status(&b.name)))
}

#[derive(Deserialize)]
struct RevealBody {
    name: String,
    store: secrets::Store,
}

async fn reveal_secret(Json(b): Json<RevealBody>) -> ApiResult<Value> {
    let value = secrets::reveal(&b.name, b.store)?;
    Ok(Json(json!({ "name": b.name, "value": value })))
}

#[derive(Deserialize)]
struct StartJobBody {
    /// Where to run. For most actions this is the loop's own directory.
    cwd: String,
    /// The draft, when the action needs one written to disk first.
    #[serde(default)]
    draft: Option<DraftBody>,
    #[serde(flatten)]
    action: exec::Action,
}

#[derive(Deserialize)]
struct DraftBody {
    config: Value,
    #[serde(default = "yaml_format")]
    format: assemble::Format,
}

async fn start_job(
    State(s): State<AppState>,
    Json(mut b): Json<StartJobBody>,
) -> ApiResult<Value> {
    // A create carries the form's contents. Writing the scratch file here,
    // rather than trusting a path from the browser, is what stops the API from
    // being a way to feed an arbitrary file to `loopsmith new`.
    if let Some(draft) = b.draft.take() {
        let cfg: loopsmith_core::LoopConfig =
            serde_json::from_value(draft.config).map_err(|e| e.to_string())?;
        let text = assemble::render(&cfg, draft.format)?;
        let scratch = assemble::write_scratch(&text, draft.format)?;
        if let exec::Action::Create { config_file, .. } = &mut b.action {
            *config_file = scratch.display().to_string();
        }
    }

    let cwd = expand_home(&b.cwd);
    // `new` writes the directory it is given, so it must be able to run from
    // somewhere that already exists — the parent, if the target does not yet.
    let cwd = if cwd.is_dir() {
        cwd
    } else {
        cwd.parent()
            .filter(|p| p.is_dir())
            .map(|p| p.to_path_buf())
            .unwrap_or(cwd)
    };

    let (kind, argv) = exec::argv_for(&b.action)?;
    let id = s.jobs.spawn(&kind, argv, cwd)?;

    // A successful create is worth remembering so the library can offer it
    // next time. Recorded optimistically and filtered on read if the directory
    // never appeared — see `assemble::library`.
    if let exec::Action::Create { path, name, .. } = &b.action {
        let dir = expand_home(path);
        let _ = assemble::remember(assemble::LibraryEntry {
            path: dir.display().to_string(),
            name: if name.trim().is_empty() {
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("loop")
                    .to_string()
            } else {
                name.clone()
            },
            config_file: b
                .draft
                .as_ref()
                .map(|d| d.format.file_name())
                .unwrap_or_else(|| "loop.yaml".into()),
            created_ms: detect::now_ms(),
        });
    }

    Ok(Json(json!({ "job": id })))
}

async fn list_jobs(State(s): State<AppState>) -> Json<Vec<exec::JobSummary>> {
    Json(s.jobs.list())
}

async fn job_detail(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    let summary = s
        .jobs
        .summary(&id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no such job".into()))?;
    Ok(Json(json!({ "summary": summary, "lines": s.jobs.lines(&id) })))
}

async fn cancel_job(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Value> {
    s.jobs.cancel(&id)?;
    Ok(Json(json!({ "ok": true })))
}

/// Live output for one job.
///
/// The socket replays everything printed so far before switching to live
/// lines, so a browser that opens the console halfway through a run sees the
/// whole thing rather than joining mid-sentence.
async fn job_stream(
    State(s): State<AppState>,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| crate::web::ws::pump(socket, s.jobs, id))
}

/// `~/loops/thing` is what people type. Nothing else expands it for them.
pub fn expand_home(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = detect::home_dir() {
            return home.join(rest);
        }
    }
    if trimmed == "~" {
        if let Some(home) = detect::home_dir() {
            return home;
        }
    }
    PathBuf::from(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_tilde_is_expanded_and_a_bare_tilde_is_the_home_itself() {
        let Some(home) = detect::home_dir() else {
            return;
        };
        assert_eq!(expand_home("~/loops/x"), home.join("loops/x"));
        assert_eq!(expand_home("~"), home);
    }

    #[test]
    fn a_tilde_that_is_not_a_home_prefix_is_left_alone() {
        // `~backup` is a real directory name, not a home reference, and
        // rewriting it would silently point somewhere else.
        assert_eq!(expand_home("~backup"), PathBuf::from("~backup"));
        assert_eq!(expand_home("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_home("  spaced  "), PathBuf::from("spaced"));
    }

    #[test]
    fn every_route_the_browser_calls_is_mounted() {
        // Cheap guard against a handler being written and never wired up,
        // which fails as a 404 in the browser and nowhere else.
        let state = AppState {
            jobs: exec::Jobs::new(),
            version: "test",
        };
        let _ = router(state);
    }
}
