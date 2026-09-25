//! The 0.3 command spellings, kept alive in one table.
//!
//! 1.0 groups the commands under four nouns — `loop`, `run`, `memory`,
//! `skills` — because twenty-two flat verbs is a list nobody reads. But every
//! one of those verbs is in somebody's shell history, somebody's Makefile, and
//! somebody's crontab, and two of them are in every launcher this binary has
//! ever generated. So the old spellings keep working.
//!
//! They keep working *here*, before clap sees the arguments, rather than as a
//! second set of hidden subcommands. One table, one notice, and at 2.0 this
//! file is deleted whole rather than unpicked from the grammar (q56 b).
//!
//! The rewrite only ever looks at the first argument, which is always a
//! subcommand or a global flag — never a path — so a loop config named
//! `validate` is in no danger.

/// Which noun each 0.3 verb moved under.
const MOVED: &[(&str, &str)] = &[
    // `loop`: everything that makes or checks a config file.
    ("new", "loop"),
    ("guided", "loop"),
    ("validate", "loop"),
    ("plan", "loop"),
    ("convert", "loop"),
    ("migrate", "loop"),
    ("permissions", "loop"),
    // `run`: everything that concerns an actual run of one.
    ("resume", "run"),
    ("status", "run"),
    ("ledger", "run"),
    ("gate", "run"),
    ("watch", "run"),
    ("schedule", "run"),
    ("prune", "run"),
    ("proposals", "run"),
];

/// The verbs under `run`, which is the one noun that shares its name with a
/// 0.3 verb. `loopsmith run <config>` has to stay the run-it command, so the
/// token after `run` is what decides which of the two was meant.
const RUN_VERBS: &[&str] = &[
    "start",
    "resume",
    "status",
    "ledger",
    "gate",
    "watch",
    "schedule",
    "prune",
    "proposals",
    "help",
];

/// What a rewrite did, for the one-line notice.
pub struct Moved {
    pub was: String,
    pub now: String,
}

impl Moved {
    pub fn notice(&self) -> String {
        format!(
            "note: `loopsmith {}` is now `loopsmith {}`. The old spelling still \
             works and goes away in 2.0.",
            self.was, self.now
        )
    }
}

/// Rewrite a 0.3 invocation into its 1.0 spelling.
///
/// `args` is the whole `argv`, program name included, exactly as clap expects
/// it back. Anything unrecognised is returned untouched, so a bad command
/// still reaches clap and gets clap's error rather than one of ours.
pub fn rewrite<I, S>(args: I) -> (Vec<String>, Option<Moved>)
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args: Vec<String> = args.into_iter().map(Into::into).collect();
    let Some(first) = args.get(1).cloned() else {
        return (args, None);
    };

    // `run` is the collision: a noun in 1.0, a verb in 0.3.
    //
    // The token straight after it settles the common case: one of `run`'s own
    // verbs means the noun. Otherwise what decides is whether there is a
    // config path in there at all, because 0.3's `run` took flags before its
    // positional — `loopsmith run --dry-run loop.yaml` is in as many scripts
    // as the plain form. So any later non-flag token means the 0.3 verb, and
    // `start` goes in immediately after `run` where the verb belongs. Nothing
    // but flags means neither, and clap prints the noun's help.
    //
    // Reading the first *non-flag* token instead would misfire on a flag's
    // value: `--run-id start` would look like the `start` verb.
    if first == "run" {
        let verb_next = args.get(2).is_some_and(|n| RUN_VERBS.contains(&n.as_str()));
        let has_config = args[2..].iter().any(|a| !a.starts_with('-'));
        if verb_next || !has_config {
            return (args, None);
        }
        args.insert(2, "start".into());
        return (
            args,
            Some(Moved {
                was: "run <config>".into(),
                now: "run start <config>".into(),
            }),
        );
    }

    let Some((_, noun)) = MOVED.iter().find(|(verb, _)| *verb == first) else {
        return (args, None);
    };
    args.insert(1, (*noun).into());
    (
        args,
        Some(Moved {
            was: first.clone(),
            now: format!("{noun} {first}"),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewritten(args: &[&str]) -> (Vec<String>, Option<String>) {
        let (out, moved) = rewrite(args.iter().map(|s| s.to_string()));
        (out, moved.map(|m| m.now))
    }

    #[test]
    fn a_moved_verb_gains_its_noun() {
        let (args, moved) = rewritten(&["loopsmith", "validate", "loop.yaml", "--strict"]);
        assert_eq!(args, ["loopsmith", "loop", "validate", "loop.yaml", "--strict"]);
        assert_eq!(moved.as_deref(), Some("loop validate"));
    }

    #[test]
    fn every_generated_launcher_keeps_working() {
        // These two lines are in every loop directory this binary has ever
        // scaffolded, and in the crontab lines `schedule --install` writes.
        let (args, _) = rewritten(&["loopsmith", "run", "loop.yaml", "--verbose"]);
        assert_eq!(args, ["loopsmith", "run", "start", "loop.yaml", "--verbose"]);
        let (args, _) = rewritten(&["loopsmith", "resume", "loop.yaml", "r-1"]);
        assert_eq!(args, ["loopsmith", "run", "resume", "loop.yaml", "r-1"]);
    }

    #[test]
    fn a_03_run_keeps_working_with_its_flags_in_front() {
        // 0.3's `run` took `--dry-run`, `--verbose`, `--run-id` and
        // `--no-acquire` before its positional, and clap was happy to read
        // them in either order. Both orders are in scripts.
        let (args, moved) = rewritten(&["loopsmith", "run", "--dry-run", "loop.yaml"]);
        assert_eq!(args, ["loopsmith", "run", "start", "--dry-run", "loop.yaml"]);
        assert!(moved.is_some());

        let (args, _) = rewritten(&["loopsmith", "run", "-v", "loop.yaml"]);
        assert_eq!(args, ["loopsmith", "run", "start", "-v", "loop.yaml"]);
    }

    #[test]
    fn a_flag_value_that_reads_like_a_verb_is_still_a_value() {
        // `--run-id start` is the reason this looks at the token straight
        // after `run` rather than at the first non-flag token anywhere.
        let (args, _) = rewritten(&["loopsmith", "run", "--run-id", "start", "loop.yaml"]);
        assert_eq!(
            args,
            ["loopsmith", "run", "start", "--run-id", "start", "loop.yaml"]
        );
    }

    #[test]
    fn the_run_noun_is_left_alone() {
        for tail in [
            vec!["start", "loop.yaml"],
            vec!["status", "loop.yaml", "r-1"],
            vec!["--help"],
            vec![],
        ] {
            let mut argv = vec!["loopsmith", "run"];
            argv.extend(tail.iter().copied());
            let (args, moved) = rewritten(&argv);
            assert_eq!(args.len(), argv.len(), "{argv:?} was rewritten");
            assert!(moved.is_none(), "{argv:?} reported a move");
        }
    }

    #[test]
    fn a_noun_that_did_not_move_is_untouched() {
        for verb in ["memory", "skills", "doctor", "providers", "web", "mcp", "loop"] {
            let (args, moved) = rewritten(&["loopsmith", verb]);
            assert_eq!(args, ["loopsmith", verb]);
            assert!(moved.is_none(), "`{verb}` reported a move");
        }
    }

    #[test]
    fn a_global_flag_or_an_unknown_word_reaches_clap_unchanged() {
        for first in ["--web", "--guided", "--help", "nonsense"] {
            let (args, moved) = rewritten(&["loopsmith", first]);
            assert_eq!(args, ["loopsmith", first]);
            assert!(moved.is_none());
        }
        let (args, _) = rewritten(&["loopsmith"]);
        assert_eq!(args, ["loopsmith"]);
    }

    #[test]
    fn every_moved_verb_lands_on_a_noun_that_exists() {
        use clap::CommandFactory;
        let cli = super::super::Cli::command();
        for (verb, noun) in MOVED {
            let group = cli
                .get_subcommands()
                .find(|s| s.get_name() == *noun)
                .unwrap_or_else(|| panic!("`{noun}` is not a subcommand"));
            assert!(
                group.get_subcommands().any(|s| s.get_name() == *verb),
                "`{noun}` has no `{verb}`"
            );
        }
    }

    #[test]
    fn every_run_verb_is_a_real_one() {
        // The list decides whether `loopsmith run x` means the noun or the
        // 0.3 verb, so a verb missing from it would be read as a config path.
        use clap::CommandFactory;
        let cli = super::super::Cli::command();
        let run = cli
            .get_subcommands()
            .find(|s| s.get_name() == "run")
            .expect("`run` is a subcommand");
        let declared: Vec<&str> = run.get_subcommands().map(|s| s.get_name()).collect();
        for verb in RUN_VERBS {
            assert!(
                declared.contains(verb) || *verb == "help",
                "`run {verb}` is in the table but not in the grammar"
            );
        }
        for verb in declared {
            assert!(
                RUN_VERBS.contains(&verb),
                "`run {verb}` is in the grammar but not in the table, so \
                 `loopsmith run {verb}` would be read as a config path"
            );
        }
    }
}
