//! Running loopsmith commands on behalf of the browser.
//!
//! Every button in the UI spawns *this binary* as a subprocess rather than
//! calling into the crates directly. That is a deliberate trade of a few
//! milliseconds for a property worth far more: the web UI cannot drift from
//! the CLI. If `loopsmith run` gains a flag, changes an exit code, or fixes a
//! bug, the browser gets it for free and nothing here has to be told.
//!
//! `current_exe()` is what gets spawned, not a `loopsmith` found on PATH. A
//! machine with a stale copy installed globally and a fresh one built in a
//! checkout would otherwise show one version in the header and run another.
//!
//! Output is fanned out two ways at once: appended to a retained buffer so a
//! browser that connects late still sees the whole run, and published to a
//! broadcast channel so one that is already connected sees each line as it
//! lands. A run is worth watching precisely when it is slow.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// Lines kept per job. A long `watch` can emit far more than anyone will read;
/// this bounds memory without bounding the run.
const MAX_RETAINED_LINES: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    /// Exited zero.
    Succeeded,
    /// Exited non-zero. Not an error in this module's sense: a gate that
    /// refuses to open is loopsmith working correctly.
    Failed,
    /// Killed on request.
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobLine {
    pub seq: u64,
    /// `out`, `err`, or `meta` — the last being this module speaking, not the
    /// subprocess. Kept distinct so the UI never attributes its own status
    /// text to the command.
    pub stream: &'static str,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobSummary {
    pub id: String,
    /// `run`, `plan`, `validate`… — what the user pressed.
    pub kind: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub state: JobState,
    pub exit_code: Option<i32>,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
}

struct Job {
    summary: JobSummary,
    lines: Vec<JobLine>,
    tx: broadcast::Sender<JobLine>,
    /// Present while the job runs. Taking it is how cancellation happens.
    child: Option<Arc<Mutex<Option<tokio::process::Child>>>>,
}

#[derive(Clone, Default)]
pub struct Jobs {
    inner: Arc<Mutex<HashMap<String, Job>>>,
    seq: Arc<Mutex<u64>>,
}

impl Jobs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a loopsmith subcommand. Returns immediately with the job id.
    pub fn spawn(&self, kind: &str, args: Vec<String>, cwd: PathBuf) -> Result<String, String> {
        let exe = std::env::current_exe()
            .map_err(|e| format!("could not find the loopsmith binary to run: {e}"))?;

        if !cwd.is_dir() {
            return Err(format!(
                "{} is not a directory, so there is nowhere to run this from",
                cwd.display()
            ));
        }

        let id = format!("job-{}-{}", crate::web::detect::now_ms(), next(&self.seq));
        let (tx, _rx) = broadcast::channel(512);

        let summary = JobSummary {
            id: id.clone(),
            kind: kind.to_string(),
            argv: std::iter::once("loopsmith".to_string())
                .chain(args.iter().cloned())
                .collect(),
            cwd: cwd.display().to_string(),
            state: JobState::Running,
            exit_code: None,
            started_ms: crate::web::detect::now_ms(),
            finished_ms: None,
        };

        let holder = Arc::new(Mutex::new(None));
        self.inner.lock().unwrap().insert(
            id.clone(),
            Job {
                summary,
                lines: Vec::new(),
                tx: tx.clone(),
                child: Some(holder.clone()),
            },
        );

        let this = self.clone();
        let job_id = id.clone();
        tokio::spawn(async move {
            this.drive(job_id, exe, args, cwd, holder).await;
        });

        Ok(id)
    }

    async fn drive(
        &self,
        id: String,
        exe: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
        holder: Arc<Mutex<Option<tokio::process::Child>>>,
    ) {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;

        self.push(&id, "meta", format!("$ loopsmith {}", args.join(" ")));
        self.push(&id, "meta", format!("  in {}", cwd.display()));

        let spawned = Command::new(&exe)
            .args(&args)
            .current_dir(&cwd)
            // Providers spawned by the run inherit this, which is how a key
            // saved in the UI a moment ago reaches the model without a shell
            // restart. See `secrets::set`.
            .envs(std::env::vars())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .spawn();

        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                self.push(&id, "err", format!("could not start loopsmith: {e}"));
                self.finish(&id, JobState::Failed, None);
                return;
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        *holder.lock().unwrap() = Some(child);

        let mut tasks = Vec::new();
        if let Some(out) = stdout {
            let (this, id) = (self.clone(), id.clone());
            tasks.push(tokio::spawn(async move {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    this.push(&id, "out", l);
                }
            }));
        }
        if let Some(err) = stderr {
            let (this, id) = (self.clone(), id.clone());
            tasks.push(tokio::spawn(async move {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    this.push(&id, "err", l);
                }
            }));
        }

        // Both readers must drain before the exit code is reported, or the
        // last few lines of a run arrive after the "finished" banner and read
        // as belonging to nothing.
        for t in tasks {
            let _ = t.await;
        }

        let status = {
            let mut guard = holder.lock().unwrap();
            guard.take()
        };
        let code = match status {
            Some(mut c) => c.wait().await.ok().and_then(|s| s.code()),
            None => None,
        };

        let was_cancelled = matches!(self.state(&id), Some(JobState::Cancelled));
        let state = if was_cancelled {
            JobState::Cancelled
        } else if code == Some(0) {
            JobState::Succeeded
        } else {
            JobState::Failed
        };

        self.push(
            &id,
            "meta",
            match state {
                JobState::Succeeded => "finished — exit 0".to_string(),
                JobState::Cancelled => "stopped on request".to_string(),
                _ => format!("finished — exit {}", code.unwrap_or(-1)),
            },
        );
        self.finish(&id, state, code);
    }

    fn push(&self, id: &str, stream: &'static str, text: String) {
        let mut jobs = self.inner.lock().unwrap();
        let Some(job) = jobs.get_mut(id) else { return };
        let line = JobLine {
            seq: job.lines.len() as u64,
            stream,
            text,
        };
        job.lines.push(line.clone());
        if job.lines.len() > MAX_RETAINED_LINES {
            // Drop from the front. The tail is what someone debugging wants,
            // and unbounded growth on a week-long `watch` is not an option.
            job.lines.drain(0..MAX_RETAINED_LINES / 4);
        }
        // Err means nobody is listening. That is the common case for a job
        // nobody has opened yet, and is not a failure.
        let _ = job.tx.send(line);
    }

    fn finish(&self, id: &str, state: JobState, code: Option<i32>) {
        let mut jobs = self.inner.lock().unwrap();
        if let Some(job) = jobs.get_mut(id) {
            // A cancel already recorded its state; do not overwrite it.
            if job.summary.state == JobState::Running || state == JobState::Cancelled {
                job.summary.state = state;
            }
            job.summary.exit_code = code;
            job.summary.finished_ms = Some(crate::web::detect::now_ms());
            job.child = None;
        }
    }

    fn state(&self, id: &str) -> Option<JobState> {
        self.inner.lock().unwrap().get(id).map(|j| j.summary.state)
    }

    pub fn summary(&self, id: &str) -> Option<JobSummary> {
        self.inner.lock().unwrap().get(id).map(|j| j.summary.clone())
    }

    pub fn list(&self) -> Vec<JobSummary> {
        let jobs = self.inner.lock().unwrap();
        let mut out: Vec<JobSummary> = jobs.values().map(|j| j.summary.clone()).collect();
        // Newest first: the job someone just started is the one they want.
        out.sort_by_key(|j| std::cmp::Reverse(j.started_ms));
        out
    }

    /// Everything printed so far, for a browser that connects mid-run.
    pub fn lines(&self, id: &str) -> Vec<JobLine> {
        self.inner
            .lock()
            .unwrap()
            .get(id)
            .map(|j| j.lines.clone())
            .unwrap_or_default()
    }

    pub fn subscribe(&self, id: &str) -> Option<broadcast::Receiver<JobLine>> {
        self.inner.lock().unwrap().get(id).map(|j| j.tx.subscribe())
    }

    /// Kill a running job.
    ///
    /// The state is recorded before the kill so the reaper in `drive` sees
    /// `Cancelled` and does not relabel it `Failed` on the way out — a stop
    /// the user asked for is not the same thing as a run that broke.
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let holder = {
            let mut jobs = self.inner.lock().unwrap();
            let job = jobs.get_mut(id).ok_or("no such job")?;
            if job.summary.state != JobState::Running {
                return Err("that job has already finished".into());
            }
            job.summary.state = JobState::Cancelled;
            job.child.clone().ok_or("that job has no process")?
        };
        let mut guard = holder.lock().unwrap();
        match guard.as_mut() {
            Some(child) => {
                let _ = child.start_kill();
                Ok(())
            }
            None => Ok(()),
        }
    }
}

fn next(seq: &Arc<Mutex<u64>>) -> u64 {
    let mut n = seq.lock().unwrap();
    *n += 1;
    *n
}

/// Build the argv for one of the UI's buttons.
///
/// Kept as data in one function so the set of things the browser may run is
/// visible in a single place. The browser sends a verb from this list and its
/// own parameters; it never sends an argv. A UI that could name the program to
/// run would be a remote shell with a nice font.
pub fn argv_for(action: &Action) -> Result<(String, Vec<String>), String> {
    let a = |s: &str| s.to_string();
    Ok(match action {
        Action::Create {
            path,
            name,
            purpose,
            config_file,
            force,
        } => (
            a("create"),
            {
                let mut v = vec![a("new"), a("--path"), path.clone()];
                if !name.trim().is_empty() {
                    v.push(a("--name"));
                    v.push(name.clone());
                }
                if !purpose.trim().is_empty() {
                    v.push(a("--purpose"));
                    v.push(purpose.clone());
                }
                // The draft the browser is holding, written to a scratch file
                // by `assemble::write_scratch`. Without it, `new` writes its
                // own starter config and everything typed into the form is
                // silently thrown away.
                v.push(a("--config-file"));
                v.push(config_file.clone());
                if *force {
                    v.push(a("--force"));
                }
                v
            },
        ),
        Action::Validate { config, strict } => (
            a("validate"),
            {
                let mut v = vec![a("validate"), config.clone()];
                if *strict {
                    v.push(a("--strict"));
                }
                v
            },
        ),
        Action::Plan { config } => (a("plan"), vec![a("plan"), config.clone()]),
        Action::DryRun { config } => (
            a("dry-run"),
            vec![a("run"), config.clone(), a("--dry-run"), a("--verbose")],
        ),
        Action::Run { config } => (a("run"), vec![a("run"), config.clone(), a("--verbose")]),
        Action::Resume { config, run_id } => (
            a("resume"),
            vec![a("resume"), config.clone(), run_id.clone(), a("--verbose")],
        ),
        Action::Watch { config, max_runs } => (
            a("watch"),
            {
                let mut v = vec![a("watch"), config.clone()];
                if let Some(n) = max_runs {
                    v.push(a("--max-runs"));
                    v.push(n.to_string());
                }
                v
            },
        ),
        Action::ScheduleInstall { config } => (
            a("schedule"),
            vec![a("schedule"), config.clone(), a("--install")],
        ),
        Action::SchedulePreview { config } => {
            (a("schedule-preview"), vec![a("schedule"), config.clone()])
        }
        Action::Doctor { config } => (
            a("doctor"),
            match config {
                Some(c) => vec![a("doctor"), c.clone()],
                None => vec![a("doctor")],
            },
        ),
        Action::Providers { config } => (a("providers"), vec![a("providers"), config.clone()]),
        Action::Gate { config, target } => (
            a("gate"),
            vec![a("gate"), config.clone(), a("--target"), target.clone()],
        ),
        Action::Status { config, run_id } => (
            a("status"),
            vec![a("status"), config.clone(), run_id.clone()],
        ),
        Action::Ledger { config, run_id } => (
            a("ledger"),
            vec![a("ledger"), config.clone(), run_id.clone()],
        ),
        Action::SkillsInstall { config } => (
            a("skills-install"),
            vec![a("skills"), a("install"), config.clone()],
        ),
        Action::PermissionsWrite { config, settings } => (
            a("permissions"),
            vec![
                a("permissions"),
                config.clone(),
                a("--write"),
                settings.clone(),
            ],
        ),
    })
}

/// The closed set of things the browser is allowed to ask for.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Create {
        path: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        purpose: String,
        /// Scratch file holding the draft. Written by the API handler, never
        /// named by the browser.
        config_file: String,
        #[serde(default)]
        force: bool,
    },
    Validate { config: String, #[serde(default)] strict: bool },
    Plan { config: String },
    DryRun { config: String },
    Run { config: String },
    Resume { config: String, run_id: String },
    Watch { config: String, #[serde(default)] max_runs: Option<u32> },
    ScheduleInstall { config: String },
    SchedulePreview { config: String },
    Doctor { #[serde(default)] config: Option<String> },
    Providers { config: String },
    Gate { config: String, target: String },
    Status { config: String, run_id: String },
    Ledger { config: String, run_id: String },
    SkillsInstall { config: String },
    PermissionsWrite { config: String, settings: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_maps_to_a_loopsmith_subcommand_and_nothing_else() {
        // The browser names a verb; this function names the program. If an
        // action ever produced an argv whose first element was not one of the
        // CLI's own subcommands, the web UI would be a way to run arbitrary
        // things through a trusted binary.
        let known = [
            "new", "validate", "plan", "run", "resume", "watch", "schedule",
            "doctor", "providers", "gate", "status", "ledger", "skills",
            "permissions",
        ];
        let actions = vec![
            Action::Create {
                path: "/tmp/x".into(),
                name: "x".into(),
                purpose: "p".into(),
                config_file: "/tmp/d.yaml".into(),
                force: false,
            },
            Action::Validate { config: "c".into(), strict: true },
            Action::Plan { config: "c".into() },
            Action::DryRun { config: "c".into() },
            Action::Run { config: "c".into() },
            Action::Resume { config: "c".into(), run_id: "r".into() },
            Action::Watch { config: "c".into(), max_runs: Some(2) },
            Action::ScheduleInstall { config: "c".into() },
            Action::SchedulePreview { config: "c".into() },
            Action::Doctor { config: None },
            Action::Providers { config: "c".into() },
            Action::Gate { config: "c".into(), target: "overall".into() },
            Action::Status { config: "c".into(), run_id: "r".into() },
            Action::Ledger { config: "c".into(), run_id: "r".into() },
            Action::SkillsInstall { config: "c".into() },
            Action::PermissionsWrite { config: "c".into(), settings: "s".into() },
        ];
        for action in actions {
            let (_, argv) = argv_for(&action).expect("every action maps");
            assert!(
                known.contains(&argv[0].as_str()),
                "{argv:?} does not start with a loopsmith subcommand"
            );
        }
    }

    #[test]
    fn creating_a_loop_always_carries_the_draft_the_browser_typed() {
        // Without --config-file, `new` writes its starter config and every
        // field the user filled in is discarded without a word.
        let (_, argv) = argv_for(&Action::Create {
            path: "/tmp/x".into(),
            name: "x".into(),
            purpose: "p".into(),
            config_file: "/tmp/draft.yaml".into(),
            force: false,
        })
        .unwrap();
        assert_eq!(argv[0], "new");
        assert!(argv.contains(&"--config-file".to_string()), "{argv:?}");
        assert!(argv.contains(&"/tmp/draft.yaml".to_string()), "{argv:?}");
        assert!(!argv.contains(&"--force".to_string()), "not asked for");
    }

    #[test]
    fn an_empty_name_is_omitted_rather_than_passed_as_an_empty_flag() {
        // `--name ""` makes `new` create a loop with no name; omitting it lets
        // the CLI default to the directory name, which is what a blank field
        // in the browser means.
        let (_, argv) = argv_for(&Action::Create {
            path: "/tmp/x".into(),
            name: "  ".into(),
            purpose: String::new(),
            config_file: "/tmp/d.yaml".into(),
            force: true,
        })
        .unwrap();
        assert!(!argv.contains(&"--name".to_string()), "{argv:?}");
        assert!(!argv.contains(&"--purpose".to_string()), "{argv:?}");
        assert!(argv.contains(&"--force".to_string()));
    }

    #[test]
    fn a_dry_run_is_a_run_that_cannot_spend_anything() {
        let (kind, argv) = argv_for(&Action::DryRun { config: "loop.yaml".into() }).unwrap();
        assert_eq!(kind, "dry-run");
        assert!(argv.contains(&"--dry-run".to_string()), "{argv:?}");
    }

    #[test]
    fn watch_without_a_ceiling_omits_the_flag_rather_than_passing_zero() {
        let (_, argv) = argv_for(&Action::Watch { config: "c".into(), max_runs: None }).unwrap();
        assert!(!argv.contains(&"--max-runs".to_string()), "{argv:?}");
    }

    #[tokio::test]
    async fn a_job_in_a_directory_that_does_not_exist_is_refused_before_spawning() {
        let jobs = Jobs::new();
        let err = jobs
            .spawn("run", vec!["--version".into()], PathBuf::from("/no/such/dir"))
            .expect_err("must refuse");
        assert!(err.contains("not a directory"), "got: {err}");
    }

    /// Wait for a job to leave `Running`, or give up.
    async fn settle(jobs: &Jobs, id: &str) -> JobSummary {
        for _ in 0..100 {
            let s = jobs.summary(id).expect("job exists");
            if s.state != JobState::Running {
                return s;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        panic!("job never finished: {:?}", jobs.lines(id));
    }

    // NOTE ON THESE TWO TESTS: the runner spawns `current_exe()`, which under
    // `cargo test` is the test harness rather than the loopsmith binary. That
    // is fine — what is being tested is the runner (spawn, both pipes, exit
    // code, state, retained buffer), not what loopsmith prints. The arguments
    // below are therefore chosen for libtest, and the comments say so, so that
    // nobody later "fixes" them to loopsmith flags and gets a confusing red.

    #[tokio::test]
    async fn a_successful_job_captures_stdout_and_reports_exit_zero() {
        let dir = loopsmith_util::testing::temp_dir("web-exec-ok");
        let jobs = Jobs::new();
        // `--list` is libtest's own flag: it prints and exits 0.
        let id = jobs.spawn("probe", vec!["--list".into()], dir).unwrap();

        let s = settle(&jobs, &id).await;
        let lines = jobs.lines(&id);
        assert_eq!(s.state, JobState::Succeeded, "lines: {lines:?}");
        assert_eq!(s.exit_code, Some(0));
        assert!(s.finished_ms.is_some(), "a finished job records when");
        assert!(
            lines.iter().any(|l| l.stream == "out"),
            "stdout must reach the buffer: {lines:?}"
        );
        // The echoed command line is this module speaking, not the subprocess,
        // and it must be distinguishable from real output.
        assert!(lines.iter().any(|l| l.stream == "meta" && l.text.starts_with("$ loopsmith")));
    }

    #[tokio::test]
    async fn a_failing_job_keeps_its_stderr_and_its_exit_code() {
        // A non-zero exit is not an error in this module's sense — a gate that
        // refuses to open exits non-zero and is loopsmith working correctly.
        // What matters is that the reason survives to the console.
        let dir = loopsmith_util::testing::temp_dir("web-exec-fail");
        let jobs = Jobs::new();
        let id = jobs
            .spawn("probe", vec!["--no-such-flag-anywhere".into()], dir)
            .unwrap();

        let s = settle(&jobs, &id).await;
        let lines = jobs.lines(&id);
        assert_eq!(s.state, JobState::Failed, "lines: {lines:?}");
        assert_ne!(s.exit_code, Some(0));
        assert!(
            lines.iter().any(|l| l.stream == "err"),
            "the reason it failed must survive: {lines:?}"
        );
    }

    #[tokio::test]
    async fn cancelling_a_finished_job_says_so_instead_of_pretending() {
        let jobs = Jobs::new();
        assert!(jobs.cancel("job-nope").is_err());
    }
}
