//! Fixture builder for the stress harness.
//!
//! The shipped examples cannot be run as they stand, for two deliberate
//! reasons: every one refuses until its `pre_execution` steps are marked done,
//! and none of the detector scripts they name exist. Both properties are worth
//! keeping, so everything here rewrites a *copy* in a scratch directory and
//! never touches `config/examples/`.
//!
//! What a fixture supplies:
//!
//! - the example's config with `pre_execution` marked done
//! - deterministic providers — a command that emits a fixed judge block rather
//!   than a model that costs money and disagrees with itself
//! - a generated `scripts/` directory whose stubs exit with `$STUB_EXIT`
//! - optionally a git repository, so worktree isolation is real rather than
//!   silently degraded
//!
//! Assertions belong on the artifacts a run leaves behind — the ledger, the
//! run log, the summaries, the export — not on stdout. [`Fixture::store`] and
//! [`Fixture::log_text`] are how a scenario reaches them.

#![allow(dead_code)]

use loopsmith_core::{Detector, LoopConfig, ProviderKind, Role};
use loopsmith_memory::SledStore;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The binary under test. Cargo builds it for this integration test and hands
/// the path over in an environment variable, so there is no `cargo run` here
/// and no chance of driving a stale build.
pub const LOOPSMITH: &str = env!("CARGO_BIN_EXE_loopsmith");

/// The repository root, derived from this crate's own manifest directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate sits three levels below the repo root")
        .to_path_buf()
}

pub fn examples_dir() -> PathBuf {
    repo_root().join("config/examples")
}

/// Every example that ships, by name.
pub fn all_examples() -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(examples_dir())
        .expect("config/examples is readable")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(String::from))
        .collect();
    out.sort();
    out
}

/// What the generated detector stubs should do.
///
/// This is the axis that reaches the interesting states. Passing stubs
/// exercise the success path and the export; failing ones exercise
/// `no_progress_iterations`, the randomness gate, and `max_revisions_per_node`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stubs {
    Pass,
    Fail,
    /// Fail until the given iteration, then pass. Implemented with a counter
    /// file rather than an environment variable, since the loop cannot change
    /// its own environment between iterations.
    PassFrom(u32),
}

/// Default contents for a produced artifact.
///
/// Carries a URL and a `post_id:` line because those are what the shipped
/// examples' `regex_match` detectors look for. Without them the file exists,
/// the `file_exists` check passes, and the regex on the same file fails —
/// which is a confusing way for a stress run to go red.
const ARTIFACT_BODY: &str = "\
written by the stress harness
source: https://example.invalid/reference
post_id: 1
";

pub struct Fixture {
    pub dir: PathBuf,
    pub config: PathBuf,
    /// The rewritten config, so a scenario can read what it is running against
    /// without parsing the file again.
    pub cfg: LoopConfig,
}

impl Fixture {
    /// Build a runnable copy of a shipped example.
    ///
    /// `tag` only has to be unique across concurrently running tests: temp
    /// directories are named from it, and two tests sharing one would collide
    /// on the sled lock in a way that reads like a backend bug.
    pub fn example(name: &str, tag: &str) -> Self {
        let text = std::fs::read_to_string(examples_dir().join(format!("{name}.yaml")))
            .unwrap_or_else(|e| panic!("example `{name}` is readable: {e}"));
        Self::from_yaml(&text, tag)
    }

    /// Build a fixture from config text directly, for scenarios that need a
    /// shape no example has.
    pub fn from_yaml(text: &str, tag: &str) -> Self {
        let dir = loopsmith_util::testing::temp_dir(tag);
        let mut cfg = loopsmith_core::parse_str(text, "fixture")
            .unwrap_or_else(|e| panic!("fixture config parses: {e}"));

        unblock(&mut cfg);
        deterministic_providers(&mut cfg);

        let config = dir.join("loop.yaml");
        let out = Self { dir, config, cfg };
        out.write_config();
        out
    }

    /// Re-serialise `cfg` over the config file. Call this after mutating
    /// `cfg` in place; nothing else notices the change.
    pub fn write_config(&self) {
        let text = serde_yaml::to_string(&self.cfg).expect("the config serialises");
        std::fs::write(&self.config, text).expect("the config is writable");
    }

    /// Generate a stub for every `scripts/…` detector the config names.
    ///
    /// The examples reference 29 distinct script detectors between them and the
    /// repository ships none of them. A missing script becomes a detector
    /// error, which the gate converts to a failed check — correct, and useless
    /// for exercising anything past the first gate.
    /// A detector runs with **no shell** — `command` is argv[0] — so a stub has
    /// to be something the operating system can execute on its own. On unix that
    /// is a `#!` script with the executable bit. Windows has no shebang handling,
    /// so the same file is unrunnable there and every stubbed detector fails to
    /// spawn; the stub is written as a `.cmd` instead and the config's detector
    /// command is repointed at it.
    ///
    /// Rewriting the command rather than skipping these tests keeps the iteration
    /// loop covered on Windows, and the rewrite is honest: it is exactly what a
    /// user has to do there, which is why `compat.sh` says so.
    pub fn stub_scripts(mut self, mode: Stubs) -> Self {
        let scripts = self.dir.join("scripts");
        std::fs::create_dir_all(&scripts).expect("scripts/ is creatable");

        let windows = cfg!(windows);
        let body = match (windows, mode) {
            (false, Stubs::Pass) => "#!/bin/sh\nexit ${STUB_EXIT:-0}\n".to_string(),
            (false, Stubs::Fail) => "#!/bin/sh\nexit ${STUB_EXIT:-1}\n".to_string(),
            // The gate re-runs every detector each iteration, so a counter file
            // in the loop root is enough to make "fails, then starts passing"
            // reproducible without touching the environment mid-run.
            (false, Stubs::PassFrom(n)) => format!(
                "#!/bin/sh\nc=$(cat .stub-count 2>/dev/null || echo 0)\n\
                 c=$((c+1))\necho $c > .stub-count\n\
                 [ \"$c\" -ge {n} ] && exit 0\nexit 1\n"
            ),
            // `if not defined` so an unset STUB_EXIT behaves like `${STUB_EXIT:-0}`.
            (true, Stubs::Pass) => crlf(
                "@echo off\r\nif not defined STUB_EXIT set \"STUB_EXIT=0\"\r\n\
                 exit /b %STUB_EXIT%\r\n",
            ),
            (true, Stubs::Fail) => crlf(
                "@echo off\r\nif not defined STUB_EXIT set \"STUB_EXIT=1\"\r\n\
                 exit /b %STUB_EXIT%\r\n",
            ),
            (true, Stubs::PassFrom(n)) => crlf(&format!(
                "@echo off\r\nsetlocal enabledelayedexpansion\r\n\
                 set \"C=0\"\r\n\
                 if exist .stub-count set /p C=<.stub-count\r\n\
                 set /a C=!C!+1\r\n\
                 echo !C!>.stub-count\r\n\
                 if !C! GEQ {n} (set \"CODE=0\") else (set \"CODE=1\")\r\n\
                 endlocal & exit /b %CODE%\r\n"
            )),
        };

        // Collected before the loop: repointing a command while iterating over
        // the set derived from those same commands would read half-rewritten.
        let detectors: Vec<String> = self.script_detectors().into_iter().collect();
        for name in &detectors {
            let target = if windows {
                // `scripts/check-x.sh` -> `scripts/check-x.cmd`
                format!("{}.cmd", name.trim_end_matches(".sh"))
            } else {
                name.clone()
            };
            let path = self.dir.join(&target);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("stub parent is creatable");
            }
            std::fs::write(&path, &body).expect("stub is writable");
            make_executable(&path);
        }

        if windows {
            for v in &mut self.cfg.validations {
                if let Detector::Script { command, .. } = &mut v.detector {
                    if command.contains('/') && command.ends_with(".sh") {
                        *command = format!("{}.cmd", command.trim_end_matches(".sh"));
                    }
                }
            }
            self.write_config();
        }
        self
    }

    /// Every distinct `scripts/…` path the config's detectors name.
    pub fn script_detectors(&self) -> BTreeSet<String> {
        self.cfg
            .validations
            .iter()
            .filter_map(|v| match &v.detector {
                Detector::Script { command, .. } => Some(command.clone()),
                _ => None,
            })
            .filter(|c| c.contains('/'))
            .collect()
    }

    /// Create every path a `file_exists` detector points at, so the objective
    /// half of the gate can be satisfied without a real builder.
    ///
    /// The contents are not arbitrary. Those same files are what `regex_match`
    /// detectors read — evidence collection registers them under their stem —
    /// so the body carries the tokens the shipped examples look for. A harness
    /// cannot synthesise a string for an arbitrary pattern; use
    /// [`Fixture::write_artifacts`] when a scenario needs something specific.
    pub fn satisfy_files(self) -> Self {
        self.write_artifacts(ARTIFACT_BODY)
    }

    /// Write the same body into every file a `file_exists` detector names.
    pub fn write_artifacts(self, body: &str) -> Self {
        for (path, _) in self.file_detectors() {
            let p = self.dir.join(&path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).expect("artifact parent is creatable");
            }
            std::fs::write(&p, body).expect("artifact is writable");
        }
        self
    }

    /// Every `(path, non_empty)` a `file_exists` detector names.
    pub fn file_detectors(&self) -> Vec<(String, bool)> {
        self.cfg
            .validations
            .iter()
            .filter_map(|v| match &v.detector {
                Detector::FileExists { path, non_empty } => Some((path.clone(), *non_empty)),
                _ => None,
            })
            .collect()
    }

    /// Write `metrics.json`, which is where threshold detectors read from.
    ///
    /// With no argument this satisfies every threshold in the config; the loop
    /// is asked to prove its plumbing, not its arithmetic.
    pub fn satisfy_metrics(self) -> Self {
        let mut map = serde_json::Map::new();
        for v in &self.cfg.validations {
            if let Detector::Threshold { metric, op, value } = &v.detector {
                map.insert(metric.clone(), satisfying_value(*op, *value).into());
            }
        }
        std::fs::write(
            self.dir.join("metrics.json"),
            serde_json::to_string_pretty(&map).expect("metrics serialise"),
        )
        .expect("metrics.json is writable");
        self
    }

    /// Write specific metric values, for scenarios that want a threshold to
    /// fail on purpose.
    pub fn metrics(self, values: &[(&str, f64)]) -> Self {
        let map: serde_json::Map<String, serde_json::Value> = values
            .iter()
            .map(|(k, v)| ((*k).to_string(), serde_json::json!(v)))
            .collect();
        std::fs::write(
            self.dir.join("metrics.json"),
            serde_json::to_string_pretty(&map).expect("metrics serialise"),
        )
        .expect("metrics.json is writable");
        self
    }

    /// Make the loop directory a git repository, so `isolated: true` produces a
    /// real worktree instead of degrading to the shared directory.
    ///
    /// Worth doing deliberately in both directions: a scratch loop is *not* a
    /// repository unless someone makes it one, and the degradation is silent by
    /// design.
    pub fn git_init(self) -> Self {
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "harness@example.invalid"],
            vec!["config", "user.name", "stress harness"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            let out = Command::new("git")
                .args(&args)
                .current_dir(&self.dir)
                .output()
                .expect("git runs");
            assert!(out.status.success(), "git {args:?} failed");
        }
        // A repository with no commit has no HEAD, and `git worktree add`
        // refuses to branch from nothing.
        std::fs::write(self.dir.join(".gitignore"), "state/\nlogs/\n").expect("gitignore writes");
        for args in [vec!["add", "-A"], vec!["commit", "-qm", "fixture"]] {
            Command::new("git")
                .args(&args)
                .current_dir(&self.dir)
                .output()
                .expect("git runs");
        }
        self
    }

    /// Run the binary against this fixture. `args` follow the subcommand's own
    /// grammar; the config path is supplied here.
    pub fn run(&self, args: &[&str]) -> Output {
        self.run_with_env(args, &[])
    }

    pub fn run_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
        let mut cmd = Command::new(LOOPSMITH);
        cmd.args(args).current_dir(&self.dir);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.output().expect("the binary runs")
    }

    /// A `run` with a fixed run id, so the assertions know what to look up.
    pub fn run_loop(&self, run_id: &str, env: &[(&str, &str)]) -> Output {
        self.run_with_env(
            &["run", "loop.yaml", "--run-id", run_id, "--no-acquire"],
            env,
        )
    }

    /// Open the ledger the run wrote. Only valid once the binary has exited —
    /// sled holds an exclusive lock while the run is in flight.
    pub fn store(&self) -> SledStore {
        loopsmith_memory::open(self.dir.join("state")).expect("the store opens")
    }

    /// The plain-text run log, which must always hold exactly as many lines as
    /// the ledger holds entries.
    pub fn log_text(&self, run_id: &str) -> String {
        let path = self.dir.join("logs").join(format!("{run_id}.log"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("run log {} is readable: {e}", path.display()))
    }

    /// Where a gate-certified success would be exported to.
    pub fn export_dir(&self) -> PathBuf {
        self.dir.join(format!("{}-success", self.cfg.name))
    }

    pub fn cleanup(self) {
        // Worktrees hold their own administrative files; removing the loop
        // directory outright is enough for a scratch fixture.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Mark every `pre_execution` step done.
///
/// This is the single thing standing between an example and a run, and it is
/// the teaching mechanism rather than a bug — which is why it happens on a
/// copy.
fn unblock(cfg: &mut LoopConfig) {
    for step in &mut cfg.pre_execution {
        step.done = true;
    }
}

/// Replace every provider with a command that costs nothing and says the same
/// thing every time.
///
/// Provider *ids* are preserved, because `enforce_judge_independence` compares
/// them: a judge must sit on a different id from the builder it reviewed, or
/// the gate refuses the judgment outright rather than discounting it. Rewriting
/// the ids would quietly turn that check off.
///
/// Every provider emits the judge block, not only the ones a judge sits on.
/// Judge output is only ever harvested from nodes whose role is `judge`, so a
/// builder emitting the same text is inert — and this avoids having to
/// re-derive the cascade to work out which provider a judge would land on.
fn deterministic_providers(cfg: &mut LoopConfig) {
    let payload = judge_payload(cfg);
    for p in &mut cfg.providers.providers {
        p.kind = ProviderKind::Byok;
        p.command = "printf".into();
        p.args = vec!["%s".into(), payload.clone()];
        p.model = None;
        // The examples name environment variable keys so the runtime can check
        // they are present. A fixture must not depend on the machine having
        // them, and loopsmith never reads their values in any case.
        p.requires_env.clear();
        p.prompt_on_stdin = false;
        p.usage_regex = None;
    }
}

/// A judge block covering every `judge` detector in the config.
///
/// `SCORE: 10` clears any `min_score`, and the evidence line is mandatory: a
/// PASS with no evidence is demoted to FAIL by the parser, which would make
/// every subjective validation permanently unsatisfiable.
fn judge_payload(cfg: &LoopConfig) -> String {
    let mut out = String::new();
    for v in &cfg.validations {
        if let Detector::Judge { standard, .. } = &v.detector {
            out.push_str(&format!(
                "VERDICT: {} PASS\nSTANDARD: {}\nEVIDENCE: asserted deterministically by the stress harness\nSCORE: 10\n",
                v.name, standard
            ));
        }
    }
    if out.is_empty() {
        out.push_str("no judge detectors in this config\n");
    }
    out
}

/// A judge block that fails every check, for the judge-refuses axis.
pub fn failing_judge_payload(cfg: &LoopConfig) -> String {
    let mut out = String::new();
    for v in &cfg.validations {
        if let Detector::Judge { .. } = &v.detector {
            out.push_str(&format!(
                "VERDICT: {} FAIL\nEVIDENCE: the stress harness refused this deliberately\nSCORE: 1\n",
                v.name
            ));
        }
    }
    out
}

/// Point one provider id at a different payload, so a judge can be made to
/// disagree while the builders carry on.
pub fn set_provider_output(cfg: &mut LoopConfig, id: &str, payload: &str) {
    if let Some(p) = cfg.providers.providers.iter_mut().find(|p| p.id == id) {
        p.args = vec!["%s".into(), payload.to_string()];
    }
}

/// Which provider id a judge node actually sits on, following the cascade when
/// the node did not name one.
pub fn judge_provider_ids(cfg: &LoopConfig) -> BTreeSet<String> {
    cfg.graph
        .nodes
        .iter()
        .filter(|n| n.role == Role::Judge)
        .filter_map(|n| match &n.provider {
            Some(id) => Some(id.clone()),
            None => cfg.cascade_for(n.tier).first().map(|p| p.id.clone()),
        })
        .collect()
}

fn satisfying_value(op: loopsmith_core::CompareOp, want: f64) -> f64 {
    use loopsmith_core::CompareOp::*;
    match op {
        Gt => want + 1.0,
        Gte | Eq => want,
        Lt => want - 1.0,
        Lte => want,
    }
}

#[cfg(unix)]
/// `cmd.exe` needs CRLF in a batch file: with LF only, the trailing newline joins
/// the last token on the line and `exit /b 0` becomes an unknown command.
fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).expect("stub exists").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("stub is chmod-able");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
