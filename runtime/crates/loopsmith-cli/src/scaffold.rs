//! `loopsmith new --path <dir>` — materialise a purpose-specific loop.
//!
//! The `--path` argument is mandatory by design. A loop is a durable thing
//! with state, a ledger, and a schedule; leaving its home directory implicit
//! is how you end up with three half-finished loops writing into each other's
//! sled trees.

use loopsmith_core::{
    AcquisitionSource, Concurrency, ConstraintSet, Constraints, Detector, Goal, GraphSpec,
    InfoItem, LoopConfig, Mode, NodeSpec, ProviderRouting, Role, SkillPolicy, StopGates,
    SuccessScenario, Tier, Trigger, Validation, WorkItem, OVERALL,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct NewLoopArgs {
    pub path: PathBuf,
    pub name: String,
    pub purpose: String,
    pub force: bool,
}

/// Files written into the new loop directory.
#[derive(Debug)]
pub struct Scaffold {
    pub written: Vec<PathBuf>,
}

pub fn starter_config(name: &str, purpose: &str) -> LoopConfig {
    let mut cascade: BTreeMap<String, Vec<String>> = BTreeMap::new();
    cascade.insert(
        "cheap".into(),
        vec!["ollama".into(), "claude".into()],
    );
    cascade.insert("standard".into(), vec!["claude".into(), "gemini".into()]);
    cascade.insert("strong".into(), vec!["openai".into(), "claude".into()]);

    LoopConfig {
        name: name.to_string(),
        version: "0.1.0".into(),
        description: purpose.to_string(),

        information: vec![InfoItem {
            key: "purpose".into(),
            value: purpose.to_string(),
            note: Some("Replace with the durable facts every node should know.".into()),
        }],

        pre_execution: vec![
            WorkItem {
                step: "Run this task manually end to end at least once".into(),
                done: false,
                evidence: Some("Link the transcript or notes here.".into()),
            },
            WorkItem {
                step: "Write down what 'done' means in checkable terms".into(),
                done: false,
                evidence: None,
            },
        ],

        goals: vec![Goal {
            name: "primary".into(),
            description: format!("Deliver the {purpose} outcome to the stated bar."),
            depends_on: vec![],
            priority: Some(1),
        }],

        validations: vec![
            Validation {
                target: "primary".into(),
                name: "artifact-exists".into(),
                mode: Mode::Objective,
                statement: "The deliverable exists and is non-empty.".into(),
                detector: Detector::FileExists {
                    path: "out/result.md".into(),
                    non_empty: true,
                },
                blocking: true,
            },
            Validation {
                target: OVERALL.into(),
                name: "checks-pass".into(),
                mode: Mode::Objective,
                statement: "The project's own check command exits clean.".into(),
                detector: Detector::Script {
                    command: "true".into(),
                    args: vec![],
                    expect_exit: Some(0),
                },
                blocking: true,
            },
        ],

        success: vec![SuccessScenario {
            target: OVERALL.into(),
            name: "all-blocking-pass".into(),
            mode: Mode::Percentage,
            statement: "Every blocking validation passes.".into(),
            threshold: Some(1.0),
        }],

        stop_gates: StopGates {
            max_iterations: 8,
            max_revisions_per_node: 3,
            max_wall_clock_seconds: Some(3600),
            max_tokens: Some(2_000_000),
            max_cost_usd: Some(5.0),
            no_progress_iterations: 3,
            stop_on_overall_success: true,
        },

        schedules: vec![Trigger::Manual],

        constraints: Constraints {
            global: ConstraintSet {
                rules: ConstraintSet::frozen_git_rules(),
                forbidden_paths: vec![".git/".into(), "node_modules/".into()],
                forbidden_commands: vec!["rm -rf".into(), "git push".into()],
                max_tokens: None,
                max_seconds: Some(900),
                human_checkpoint: vec![
                    "publishing anything".into(),
                    "sending a message".into(),
                    "deleting data".into(),
                ],
            },
            per_node: BTreeMap::new(),
        },

        graph: GraphSpec {
            nodes: vec![
                NodeSpec {
                    id: "build".into(),
                    role: Role::Builder,
                    instruction: "Produce the deliverable described in the goal. State any assumption you had to make.".into(),
                    depends_on: vec![],
                    goals: vec!["primary".into()],
                    tier: Tier::Standard,
                    provider: None,
                    skills: vec![],
                    weight: 3.0,
                    isolated: true,
                },
                NodeSpec {
                    id: "judge".into(),
                    role: Role::Judge,
                    instruction: "Check the builder's output against the named standard and the original brief. Report per-check pass or fail with evidence.".into(),
                    depends_on: vec!["build".into()],
                    goals: vec!["primary".into()],
                    tier: Tier::Strong,
                    provider: None,
                    skills: vec![],
                    weight: 1.0,
                    isolated: false,
                },
            ],
            concurrency: Concurrency::Auto {
                cap: 16,
                min_marginal_gain: 0.05,
            },
        },

        providers: ProviderRouting {
            providers: loopsmith_provider::starter_providers(),
            cascade,
            enforce_judge_independence: true,
        },

        skills: SkillPolicy {
            acquisition_order: vec![
                AcquisitionSource::Installed,
                AcquisitionSource::Marketplace,
                AcquisitionSource::Generate,
            ],
            quarantine_dir: "generated-skills".into(),
            min_marketplace_stars: 100,
            require_human_promotion: true,
            explore: false,
            explore_candidates: vec![],
            min_trials: 3,
        },
    }
}

const GITIGNORE: &str = "\
# loopsmith run state — regenerable, machine-local, and often large
state/
out/
*.log

# Quarantined sub-agents wait for human promotion; they are not source
generated-skills/
";

fn readme(name: &str, purpose: &str) -> String {
    format!(
        "# {name}\n\n\
A loopsmith loop.\n\n\
**Purpose:** {purpose}\n\n\
## Run it\n\n\
```bash\n\
loopsmith validate loop.yaml     # A-H model must be complete\n\
loopsmith plan     loop.yaml     # waves, critical path, predicted speedup\n\
loopsmith run      loop.yaml     # hands-off after the permission grant\n\
```\n\n\
## Before the first run\n\n\
`pre_execution` in `loop.yaml` is deliberately unfinished. Run the task by \
hand once, record what you learned, and set each step to `done: true`. \
Validation fails until you do, because automating a process you cannot \
describe produces fast, confident garbage.\n\n\
## Layout\n\n\
| Path | What it is |\n\
|---|---|\n\
| `loop.yaml` | The A-H config: goals, validations, success, stop gates, schedules, constraints |\n\
| `state/` | sled memory: episodes, goal state, ledger, checkpoints |\n\
| `out/` | Deliverables the nodes produce |\n\
| `proposals/` | Changes the loop wants to make to its own goals — review these |\n\
| `generated-skills/` | Auto-created sub-agents awaiting promotion |\n"
    )
}

pub fn scaffold(args: &NewLoopArgs) -> std::io::Result<Scaffold> {
    let root = &args.path;
    if root.exists() && !args.force {
        let non_empty = std::fs::read_dir(root)?.next().is_some();
        if non_empty {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "{} already exists and is not empty; pass --force to write into it anyway",
                    root.display()
                ),
            ));
        }
    }

    let mut written = Vec::new();
    for sub in ["state", "out", "proposals", "generated-skills"] {
        std::fs::create_dir_all(root.join(sub))?;
    }

    let cfg = starter_config(&args.name, &args.purpose);
    let yaml = serde_yaml::to_string(&cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    write_file(root.join("loop.yaml"), &yaml, &mut written)?;
    write_file(root.join(".gitignore"), GITIGNORE, &mut written)?;
    write_file(
        root.join("README.md"),
        &readme(&args.name, &args.purpose),
        &mut written,
    )?;
    write_file(
        root.join("proposals/.gitkeep"),
        "",
        &mut written,
    )?;

    Ok(Scaffold { written })
}

fn write_file(path: PathBuf, contents: &str, written: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, contents)?;
    written.push(path);
    Ok(())
}

/// Derive a loop name from a path when the caller does not supply one.
pub fn name_from_path(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("loop")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn tmp(tag: &str) -> PathBuf {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::env::temp_dir().join(format!(
            "loopsmith-scaffold-{tag}-{}-{n}",
            std::process::id()
        ))
    }

    #[test]
    fn the_starter_config_validates_once_pre_execution_is_marked_done() {
        let mut cfg = starter_config("demo", "a demo loop");
        // As shipped it must NOT validate: the manual run has not happened.
        assert!(loopsmith_core::validate(&cfg).has_errors());
        for w in &mut cfg.pre_execution {
            w.done = true;
        }
        let r = loopsmith_core::validate(&cfg);
        assert!(!r.has_errors(), "unexpected errors:\n{}", r.render());
    }

    #[test]
    fn scaffold_writes_the_expected_files() {
        let root = tmp("writes");
        let s = scaffold(&NewLoopArgs {
            path: root.clone(),
            name: "demo".into(),
            purpose: "a demo loop".into(),
            force: false,
        })
        .unwrap();
        assert!(root.join("loop.yaml").exists());
        assert!(root.join(".gitignore").exists());
        assert!(root.join("README.md").exists());
        assert!(root.join("state").is_dir());
        assert!(root.join("proposals").is_dir());
        assert!(s.written.len() >= 4);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_written_config_round_trips_through_the_parser() {
        let root = tmp("roundtrip");
        scaffold(&NewLoopArgs {
            path: root.clone(),
            name: "demo".into(),
            purpose: "a demo loop".into(),
            force: false,
        })
        .unwrap();
        let cfg = loopsmith_core::load(root.join("loop.yaml")).expect("reloads");
        assert_eq!(cfg.name, "demo");
        assert_eq!(cfg.graph.nodes.len(), 2);
        assert!(cfg.providers.enforce_judge_independence);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn scaffolding_into_a_non_empty_directory_is_refused_without_force() {
        let root = tmp("occupied");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("something.txt"), b"hi").unwrap();
        let err = scaffold(&NewLoopArgs {
            path: root.clone(),
            name: "demo".into(),
            purpose: "p".into(),
            force: false,
        })
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn force_overrides_the_non_empty_check() {
        let root = tmp("forced");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("something.txt"), b"hi").unwrap();
        assert!(scaffold(&NewLoopArgs {
            path: root.clone(),
            name: "demo".into(),
            purpose: "p".into(),
            force: true,
        })
        .is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn name_falls_back_to_the_directory_name() {
        assert_eq!(name_from_path(Path::new("/tmp/my-loop")), "my-loop");
    }
}
