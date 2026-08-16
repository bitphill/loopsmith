//! Stress tests that reach the network or spend money.
//!
//! Every test here returns without asserting unless its environment variable is
//! set, so `cargo test --workspace` never clones a repository, never calls a
//! model, and never costs anything. Run them deliberately:
//!
//! ```sh
//! LOOPSMITH_STRESS_NETWORK=1  cargo test -p loopsmith-cli --test opt_in
//! LOOPSMITH_STRESS_PROVIDER=1 cargo test -p loopsmith-cli --test opt_in -- --nocapture
//! ```
//!
//! `LOOPSMITH_STRESS_PROVIDER` invokes whatever model the config names. Keep it
//! pointed at the cheapest one you have; the point is to prove the plumbing
//! reaches a real provider, not to get a good answer.

mod harness;

use harness::{Fixture, Stubs};
use loopsmith_memory::{LedgerKind, Store};

/// Skip unless the named variable is set to something truthy.
macro_rules! gated {
    ($var:literal) => {
        match std::env::var($var).ok().as_deref() {
            Some("1") | Some("true") => {}
            _ => {
                eprintln!("skipping: set {}=1 to run this", $var);
                return;
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Section J: the github clone
// ---------------------------------------------------------------------------

/// `install_default` has never actually cloned anything. It shells out to
/// `git clone --depth 1` into the quarantine directory, and the network path,
/// `is_safe_repo_url`, and the post-clone `init_command` were all untried.
#[test]
fn a_declared_github_sub_agent_is_cloned_into_quarantine() {
    gated!("LOOPSMITH_STRESS_NETWORK");

    let f = Fixture::from_yaml(
        r#"
name: clone-loop
goals:
  - name: g1
    description: a goal description long enough to satisfy the validator
pre_execution:
  - step: done by hand
    done: true
validations:
  - target: g1
    name: v1
    mode: objective
    statement: always true
    detector: { type: script, command: "true" }
default_skills:
  - name: agent-reach
    source: github
    url: https://github.com/Panniantong/agent-reach
graph:
  nodes:
    - id: build
      role: builder
      instruction: produce the thing the goal describes
      goals: [g1]
providers:
  providers:
    - id: p
      kind: byok
      command: echo
      args: ["ok"]
  cascade:
    standard: [p]
"#,
        "clone-github",
    );

    let out = f.run(&["skills", "install", "loop.yaml"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");

    let cloned = f.dir.join("generated-skills/agent-reach");
    assert!(cloned.is_dir(), "the repository must land in quarantine: {text}");
    assert!(
        !cloned.join(".git/refs/remotes").exists() || cloned.join(".git").exists(),
        "a --depth 1 clone still has a .git directory"
    );
    f.cleanup();
}

/// A `github` sub-agent with a URL the loop will not accept must be refused
/// before git is invoked at all.
///
/// Not gated: refusing takes no network, and this is the half of the clone path
/// that matters most.
#[test]
fn an_unsafe_repo_url_is_refused_before_git_is_reached() {
    let f = Fixture::from_yaml(
        r#"
name: unsafe-clone
goals:
  - name: g1
    description: a goal description long enough to satisfy the validator
validations:
  - target: g1
    name: v1
    mode: objective
    statement: always true
    detector: { type: script, command: "true" }
default_skills:
  - name: sneaky
    source: github
    url: file:///etc
"#,
        "clone-unsafe",
    );

    let out = f.run(&["skills", "install", "loop.yaml"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("only https is accepted") || text.contains("not an https git URL"),
        "the refusal must name the reason: {text}"
    );
    assert!(
        !f.dir.join("generated-skills/sneaky").exists(),
        "nothing must be fetched for a refused URL"
    );
    f.cleanup();
}

/// A post-clone `init_command` is argv, never a shell line. `init_argv` is
/// unit-tested and had never been executed; no example sets one.
#[test]
fn a_post_clone_init_command_runs_inside_the_installed_directory() {
    gated!("LOOPSMITH_STRESS_NETWORK");

    let f = Fixture::from_yaml(
        r#"
name: init-loop
goals:
  - name: g1
    description: a goal description long enough to satisfy the validator
validations:
  - target: g1
    name: v1
    mode: objective
    statement: always true
    detector: { type: script, command: "true" }
default_skills:
  - name: agent-reach
    source: github
    url: https://github.com/Panniantong/agent-reach
    init_command: touch .init-ran
"#,
        "clone-init",
    );

    let out = f.run(&["skills", "install", "loop.yaml"]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    assert!(
        f.dir.join("generated-skills/agent-reach/.init-ran").is_file(),
        "the init command must run inside the skill it set up: {text}"
    );
    f.cleanup();
}

// ---------------------------------------------------------------------------
// A real provider, once
// ---------------------------------------------------------------------------

/// One real-provider execution of the simplest example.
///
/// Everything else in the stress suite swaps the providers for `printf`, which
/// proves the plumbing but never proves that the plumbing reaches a model. This
/// does, once, deliberately, and only when asked.
#[test]
fn the_simplest_example_runs_against_a_real_provider() {
    gated!("LOOPSMITH_STRESS_PROVIDER");

    // `Fixture::example` swaps the providers out, so this reads the example
    // directly and keeps them.
    let text = std::fs::read_to_string(harness::examples_dir().join("research-loop.yaml"))
        .expect("the example is readable");
    let mut cfg =
        loopsmith_core::parse_str(&text, "opt-in").expect("the example parses");
    for step in &mut cfg.pre_execution {
        step.done = true;
    }
    cfg.stop_gates.max_iterations = 1;
    cfg.stop_gates.no_progress_iterations = 0;
    cfg.stop_gates.max_cost_usd = Some(1.0);

    let dir = loopsmith_util::testing::temp_dir("real-provider");
    std::fs::write(
        dir.join("loop.yaml"),
        serde_yaml::to_string(&cfg).expect("serialises"),
    )
    .unwrap();

    let out = std::process::Command::new(harness::LOOPSMITH)
        .args(["run", "loop.yaml", "--run-id", "real", "--no-acquire"])
        .current_dir(&dir)
        .output()
        .expect("the binary runs");
    println!("{}", String::from_utf8_lossy(&out.stdout));
    eprintln!("{}", String::from_utf8_lossy(&out.stderr));

    let store = loopsmith_memory::open(dir.join("state")).expect("the store opens");
    let episodes = store.episodes("real").unwrap();
    assert!(
        !episodes.is_empty(),
        "a real-provider run must have produced at least one episode"
    );
    let served: Vec<&str> = episodes
        .iter()
        .map(|e| e.provider_id.as_str())
        .filter(|p| !p.is_empty())
        .collect();
    assert!(
        !served.is_empty(),
        "no provider served anything; check `loopsmith providers loop.yaml`"
    );
    println!("served by: {served:?}");
    assert!(
        store
            .ledger("real")
            .unwrap()
            .iter()
            .any(|e| e.kind == LedgerKind::GateEvaluated),
        "the gate must have ruled on whatever the model produced"
    );
    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}

/// The randomness agent against a real cheap model, rather than a `printf` that
/// always answers correctly. What is being tested is whether a model actually
/// keeps to the four-item menu.
#[test]
fn the_randomness_agent_keeps_to_the_menu_with_a_real_model() {
    gated!("LOOPSMITH_STRESS_PROVIDER");

    let text = std::fs::read_to_string(harness::examples_dir().join("research-loop.yaml"))
        .expect("the example is readable");
    let mut cfg = loopsmith_core::parse_str(&text, "opt-in").expect("the example parses");
    for step in &mut cfg.pre_execution {
        step.done = true;
    }
    cfg.stop_gates.max_iterations = 3;
    cfg.stop_gates.no_progress_iterations = 2;
    cfg.stop_gates.no_progress_iterations_randomness = Some(1);
    cfg.stop_gates.max_cost_usd = Some(1.0);

    let dir = loopsmith_util::testing::temp_dir("real-perturb");
    std::fs::write(
        dir.join("loop.yaml"),
        serde_yaml::to_string(&cfg).expect("serialises"),
    )
    .unwrap();

    let out = std::process::Command::new(harness::LOOPSMITH)
        .args(["run", "loop.yaml", "--run-id", "rp", "--no-acquire"])
        .current_dir(&dir)
        .output()
        .expect("the binary runs");
    println!("{}", String::from_utf8_lossy(&out.stdout));

    let store = loopsmith_memory::open(dir.join("state")).expect("the store opens");
    let nudges: Vec<String> = store
        .ledger("rp")
        .unwrap()
        .iter()
        .filter(|e| e.detail.contains("no change for"))
        .map(|e| e.detail.clone())
        .collect();
    assert!(
        !nudges.is_empty(),
        "the run did not stall, so the agent was never asked"
    );
    for n in &nudges {
        println!("{n}");
        let on_menu = ["reorder:", "escalate:", "explore:", "reframe:"]
            .iter()
            .any(|m| n.contains(m));
        assert!(on_menu, "the choice must be one of the four: {n}");
    }
    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}

/// A sanity check that the harness itself still produces a runnable fixture,
/// so a failure above is about the network or the provider rather than about
/// the scaffolding around it.
#[test]
fn the_harness_still_builds_a_runnable_fixture() {
    let f = Fixture::example("research-loop", "optin-sanity")
        .stub_scripts(Stubs::Pass)
        .satisfy_files()
        .satisfy_metrics();
    let out = f.run_loop("sanity", &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    f.cleanup();
}
