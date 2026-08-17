# The stress harness

Everything in the runtime is covered by unit tests in isolation. These cover
what those cannot: whether the pieces work **together**, under a real iteration
loop, driven through the real binary, against the configs users are handed.

```sh
cargo test -p loopsmith-cli --test stress
cargo test -p loopsmith-cli --test surface
cargo test -p loopsmith-cli --test compat
cargo test -p loopsmith-cli --test opt_in     # skips everything gated by default
```

| File | Covers |
|---|---|
| `harness/mod.rs` | The fixture builder — no tests of its own |
| `stress.rs` | The iteration loop: phases, isolation, stop gates, proposals, resume, export |
| `surface.rs` | Subcommands that were implemented and never executed |
| `compat.rs` | What a generated loop assumes about the machine it lands on |
| `opt_in.rs` | Anything that reaches the network or spends money |

## Why a fixture is needed at all

The shipped examples cannot be run as they stand, and both reasons are
deliberate:

1. **Every example refuses.** `pre_execution` steps ship as `done: false`, so
   `validate` and `run` both stop. That is the teaching mechanism.
2. **No detector scripts exist.** The examples name 29 distinct `scripts/…`
   detectors between them and the repository ships none. A missing script
   becomes a detector error, which the gate converts to a failed check — correct
   behaviour, and useless for exercising anything past the first gate.

`Fixture` rewrites a **copy** in a scratch directory: it marks the steps done,
generates stubs whose exit code the scenario controls, swaps the providers for
commands that cost nothing and say the same thing every time, and optionally
`git init`s the directory so worktree isolation is real rather than silently
degraded. Nothing under `config/examples/` is ever touched.

## Assertions read artifacts, not stdout

Stdout is a report. The record is the sled ledger, `logs/<run-id>.log`,
`store.summaries()`, the checkpoint, and the presence or absence of
`<name>-success/`. `Fixture::store` and `Fixture::log_text` are how a scenario
reaches them.

One assertion recurs and is worth keeping: the run log and the ledger are
written through a single call, so they must always hold the same number of
events. A divergence means one of the two write paths grew a branch the other
did not.

## Feature × example: what has actually been executed

This was a plan. It is now a record. Every row below has run.

| Feature | Exercised by | Example |
|---|---|---|
| One iteration, end to end | `every_example_completes_an_iteration_and_leaves_a_consistent_record` | all 13 |
| Nothing satisfiable, no crash | `every_example_survives_a_run_where_nothing_passes` | all 13 |
| Regex detectors, both directions | `a_regex_detector_reads_the_file_the_loop_produced` | `blogger-loop` |
| Phase DAG, long linear chain | `phases_open_one_at_a_time_across_a_real_run` | `traffic-loop` |
| A phase that never closes | `a_phase_that_never_satisfies_its_goals_never_opens_the_next_one` | `traffic-loop` |
| Phases + perturbation | `perturbation_and_phases_do_not_dispatch_a_shut_phase` | `traffic-loop` |
| Phases + export | `a_phased_loop_that_succeeds_still_exports` | `traffic-loop` |
| Isolation degrading outside a repo | `isolation_degrades_to_shared_outside_a_repository` | `refactor-loop` |
| Isolation real inside a repo | `isolation_is_real_inside_a_repository` | `refactor-loop` |
| Isolated output reaching the gate | `an_isolated_builders_output_reaches_the_gate` | hand-written |
| One isolated node reading another's output | `an_isolated_node_can_read_what_its_isolated_upstream_produced` | hand-written |
| `no_progress_iterations` | `a_run_with_no_moving_verdicts_halts_on_no_progress` | `research-loop` |
| Randomness gate, seeded fallback | `a_stalled_run_is_perturbed_before_it_is_abandoned` | `blogger-loop` |
| Randomness gate, agent path | `the_randomness_agent_chooses_when_a_cheap_provider_answers` | hand-written |
| Randomness gate, off-menu answer | `an_answer_off_the_menu_falls_back_rather_than_being_guessed_at` | hand-written |
| `max_revisions_per_node` in a real graph | `a_stuck_node_stops_being_dispatched_while_the_run_continues` | `landing-page-loop` |
| Success export | `a_certified_success_exports_a_reusable_package` | `research-loop` |
| Resume keeps the revision budget spent | `a_resume_does_not_hand_a_stuck_node_its_revision_budget_back` | hand-written |
| Resume keeps the no-progress count | `a_resume_does_not_reset_the_no_progress_counter` | hand-written |
| `ProposalKind::ReshapeGraph` | `a_node_that_exhausts_its_revisions_asks_for_the_graph_to_be_reshaped` | hand-written |
| `ProposalKind::ChangeCriteria` | `a_detector_that_cannot_run_asks_for_the_criteria_to_change` | hand-written |
| `ProposalKind::TrySkill` | `unexplored_candidates_are_proposed_rather_than_spent_on_unasked` | hand-written |
| `context.summary_provider` | `a_summary_provider_adds_prose_that_cannot_decide_anything` | hand-written |
| `SkillTrial.tokens` | `a_skill_trial_records_what_the_node_that_used_it_cost` | hand-written |
| `new --config-stdin`, YAML | `a_config_arrives_whole_on_stdin` | — |
| `new --config-stdin --markdown` | `a_markdown_config_arrives_whole_on_stdin_too` | — |
| `convert --to-yaml` (md → yaml) | `a_config_survives_the_trip_out_to_markdown_and_back` | `research-loop` |
| `convert --to-yaml` on YAML | `to_yaml_on_yaml_re_emits_rather_than_refusing` | — |
| `skills install` (section J) | `skills_install_reports_on_every_declared_agent` | — |
| `watch --check` | `watch_check_reports_the_triggers_and_exits` | `account-watch-loop` |
| `watch` as a resident process | `watch_runs_the_loop_when_a_trigger_fires_and_stops_at_max_runs` | — |
| `schedule --install` | `schedule_install_writes_a_launch_agent_where_it_is_told` | `account-watch-loop` |
| `schedule` without `--install` | `schedule_without_install_only_prints` | `account-watch-loop` |
| `watch` refusing a manual-only loop | `watch_refuses_a_loop_with_no_trigger` | — |
| `plan`, `providers`, `permissions`, `gate` | `the_reporting_commands_all_work_against_a_shipped_example` | `refactor-loop` |
| `status`, `ledger`, `proposals` on an unknown run | `the_run_reports_handle_an_unknown_run_id` | — |
| `prune` | `prune_is_safe_with_and_without_worktrees` | — |
| Section J github clone | `a_declared_github_sub_agent_is_cloned_into_quarantine` | opt-in |
| Section J `init_command` | `a_post_clone_init_command_runs_inside_the_installed_directory` | opt-in |
| Unsafe clone URL refused | `an_unsafe_repo_url_is_refused_before_git_is_reached` | — |
| A real provider, once | `the_simplest_example_runs_against_a_real_provider` | opt-in |
| A real model on the perturbation menu | `the_randomness_agent_keeps_to_the_menu_with_a_real_model` | opt-in |
| `scripts/compat.sh` ships with a new loop | `a_new_loop_ships_the_compatibility_helpers` | — |
| Every compat helper, under POSIX `sh` | `the_compatibility_helpers_work_under_posix_sh` | — |
| Missing tooling exits 2, not 1 | `need_bash_exits_two_so_a_missing_tool_is_not_read_as_a_failed_check` | — |
| A missing command is named | `require_names_the_command_that_is_missing` | — |
| Generated scripts are POSIX | `the_generated_scripts_parse_under_posix_sh` | — |
| `resume.sh` with no run id | `resume_without_a_run_id_explains_itself` | — |
| A moved binary falls back to `PATH` | `a_generated_script_falls_back_to_path_when_the_pinned_binary_has_moved` | — |
| The export script pins nothing | `the_export_script_is_posix_and_does_not_pin_a_binary` | — |
| `doctor` reports the platform | `doctor_reports_the_platform_and_stays_advisory` | — |
| `doctor` finds unrunnable detectors | `doctor_reports_detectors_the_machine_cannot_run` | `traffic-loop` |
| `doctor` finds a non-executable detector | `doctor_reports_a_detector_that_is_not_executable` | `traffic-loop` |

### Not covered, and why

- **Actually loading a launch agent.** `--install` writes the plist and stops;
  `launchctl load -w` stays the user's call, so no test runs it. The write itself
  is covered because `LOOPSMITH_LAUNCH_AGENTS_DIR` redirects the destination.
- **A GNU userland.** Everything portability-related is asserted on whichever
  machine the suite runs on, and this one is BSD. `compat.rs` exercises the
  helpers rather than mocking the userland, so a GNU host proves the other half
  by running the same tests.
- **Windows.** The generated scripts are POSIX `sh` and the worktree, scheduler,
  and detector paths all assume a Unix. The badge claims Windows; nothing here
  tests it.

## Opt-in tests

```sh
LOOPSMITH_STRESS_NETWORK=1  cargo test -p loopsmith-cli --test opt_in
LOOPSMITH_STRESS_PROVIDER=1 cargo test -p loopsmith-cli --test opt_in -- --nocapture
```

Without the variable each gated test prints why it is skipping and returns, so
`cargo test --workspace` never clones a repository, never calls a model, and
never costs anything.

`LOOPSMITH_STRESS_PROVIDER` invokes whatever model the example names. Point it
at the cheapest one you have — the aim is to prove the plumbing reaches a real
provider, not to get a good answer.

## Traps worth not rediscovering

- **Give every fixture a distinct tag.** Temp directories are named from it, and
  two concurrent tests sharing one collide on the sled lock in a way that reads
  like a backend bug.
- **The store must be dropped before the directory is removed**, and cannot be
  opened while the binary under test is running — sled holds an exclusive lock.
- **A detector script runs with no shell and no timeout.** `command` is argv[0]
  and `args` are literal, so a stub needs a shebang and must not hang.
- **Providers keep their ids when they are swapped for stubs.**
  `enforce_judge_independence` compares them, so rewriting the ids would quietly
  switch the check off.
- **A judge PASS with no evidence is demoted to FAIL.** Any stub judge payload
  needs an `EVIDENCE:` line or every subjective validation becomes permanently
  unsatisfiable.
- **`satisfy_files` writes a body, not a placeholder.** The files a
  `file_exists` detector names are the same files a `regex_match` detector
  reads, so the default body carries a URL and a `post_id:` line — the tokens
  the shipped examples look for. `write_artifacts` takes a specific body when a
  scenario needs one.
