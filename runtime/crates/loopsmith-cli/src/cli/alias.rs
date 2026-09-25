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

    /// Every button in the web UI spells its command the 1.0 way.
    ///
    /// The browser builds no argv of its own — `loopsmith_web::exec::argv_for`
    /// does, and that function lives on the other side of a crate boundary
    /// from the grammar it has to agree with. A 0.3 spelling there would still
    /// *work*, which is exactly the danger: it would work by coming through
    /// this table, and the notice would land in the console of somebody who
    /// pressed a button and typed nothing at all.
    ///
    /// So this asserts the pair: clap accepts it, and the rewrite has nothing
    /// to say about it.
    #[cfg(feature = "web")]
    #[test]
    fn the_browser_never_asks_for_a_spelling_that_moved() {
        use clap::Parser;
        for action in loopsmith_web::exec::all_actions() {
            let (kind, argv) = loopsmith_web::exec::argv_for(&action).expect("every action maps");
            let full: Vec<String> = std::iter::once("loopsmith".to_string())
                .chain(argv.iter().cloned())
                .collect();

            let (_, moved) = rewrite(full.clone());
            assert!(
                moved.is_none(),
                "the `{kind}` button spells its command the 0.3 way: {argv:?}"
            );
            super::super::Cli::try_parse_from(&full)
                .unwrap_or_else(|e| panic!("the `{kind}` button builds {argv:?}, which clap refuses:\n{e}"));
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

    /// Documents that live above this crate, and are read rather than run.
    ///
    /// The repository root is three directories above the crate: this is a
    /// workspace under `runtime/`, and the docs are beside it.
    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("the crate is three deep in the repository")
            .to_path_buf()
    }

    /// Files whose whole job is to record what the old spelling was.
    ///
    /// `CHANGELOG.md` is history and must not be rewritten; the migration page
    /// is the mapping table itself. Everything else is telling a reader what
    /// to type today.
    const KEEPS_THE_OLD_SPELLING: &[&str] =
        &["CHANGELOG.md", "wiki/Migration-0-3-To-1-0.md"];

    fn documents(dir: &std::path::Path, into: &mut Vec<std::path::PathBuf>) {
        const SKIP: &[&str] = &[
            ".git", ".gitnexus", "target", "node_modules", "dist", "docs", "proposals",
            ".claude", "build",
        ];
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !SKIP.contains(&name.as_ref()) {
                    documents(&path, into);
                }
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("md" | "sh" | "bat" | "cmd" | "ps1" | "rb" | "html")
            ) {
                into.push(path);
            }
        }
    }

    /// No document tells anyone to type a spelling that moved.
    ///
    /// The same question the web UI is held to by
    /// `the_browser_never_asks_for_a_spelling_that_moved`, asked of the other
    /// surface a user copies commands from. It is asked through [`rewrite`]
    /// rather than against a list of strings, so a verb that moves later is
    /// caught in the documents on the same commit that moves it.
    ///
    /// A deprecation notice is not an error, which is exactly why this is
    /// worth a test: every README could tell every reader to type the 0.3
    /// spelling for a year and nothing would fail.
    #[test]
    fn no_document_tells_anyone_to_type_a_spelling_that_moved() {
        let root = repo_root();
        let mut docs = Vec::new();
        documents(&root, &mut docs);
        assert!(docs.len() > 20, "found only {} documents to check", docs.len());

        let mut wrong: Vec<String> = Vec::new();
        let mut seen = 0usize;
        for doc in docs {
            let rel = doc.strip_prefix(&root).unwrap_or(&doc).to_string_lossy().replace('\\', "/");
            if KEEPS_THE_OLD_SPELLING.contains(&rel.as_ref()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&doc) else {
                continue;
            };
            for (n, line) in text.lines().enumerate() {
                for command in commands_in(line) {
                    seen += 1;
                    let argv: Vec<String> = std::iter::once("loopsmith".to_string())
                        .chain(command.iter().map(|s| s.to_string()))
                        .collect();
                    if let (_, Some(moved)) = rewrite(argv) {
                        wrong.push(format!(
                            "{}:{}: `loopsmith {}` is now `loopsmith {}`",
                            rel,
                            n + 1,
                            moved.was,
                            moved.now
                        ));
                    }
                }
            }
        }
        // A scanner that finds nothing passes, and would go on passing after
        // someone changed the extension list or the walk.
        assert!(seen > 100, "only {seen} invocations found; the scan is not reading the documents");
        assert!(wrong.is_empty(), "documents still teaching 0.3:\n{}", wrong.join("\n"));
    }

    #[test]
    fn an_invocation_is_read_out_of_prose_and_a_crate_name_is_not() {
        assert_eq!(commands_in("run `loopsmith loop validate loop.yaml` first"), [["loop", "validate"]]);
        assert_eq!(commands_in("    loopsmith run start ./loop.yaml"), [["run", "start"]]);
        assert_eq!(commands_in("`loopsmith run <config>` is the same command"), [["run"]]);
        assert_eq!(commands_in("loopsmith.exe doctor"), [["doctor"]]);
        // Two on one line, which a table row usually is.
        assert_eq!(
            commands_in("| `loopsmith loop new` | then `loopsmith doctor` |"),
            [vec!["loop", "new"], vec!["doctor"]]
        );
        // Not invocations.
        assert!(commands_in("`loopsmith-core` owns the model").is_empty());
        assert!(commands_in("target/release/loopsmith plan x").is_empty());
        assert!(commands_in("the loopsmith").is_empty());
    }

    /// Every `loopsmith …` invocation on one line, as argv without the program.
    ///
    /// Prose is not a shell. The command ends at the first character that ends
    /// one in running text — a closing backtick, a quote, a pipe, a `<`
    /// opening a placeholder — and only the two tokens after `loopsmith` are
    /// kept, which is all [`rewrite`] reads.
    ///
    /// Stopping at `<` means `loopsmith run <config>` is read as the bare noun
    /// and passes. That is deliberate: it is the one 0.3 spelling the grammar
    /// still documents as current, because it is in every generated launcher.
    /// A concrete path — `loopsmith run loop.yaml` — is still caught.
    fn commands_in(line: &str) -> Vec<Vec<&str>> {
        let mut found = Vec::new();
        let mut rest = line;
        while let Some(at) = rest.find("loopsmith") {
            let before = rest[..at].chars().next_back();
            let after = &rest[at + "loopsmith".len()..];
            rest = after;
            // `loopsmith-core`, `my-loopsmith`, `loopsmithery`.
            if before.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/') {
                continue;
            }
            let after = after.strip_prefix(".exe").unwrap_or(after);
            if !after.starts_with(' ') {
                continue;
            }
            let end = after.find(|c: char| "`'\"|;&<>\n".contains(c)).unwrap_or(after.len());
            let argv: Vec<&str> = after[..end].split_whitespace().take(2).collect();
            if !argv.is_empty() {
                found.push(argv);
            }
        }
        found
    }

    /// The reference in `HOW-TO-USE.md` is the one the browser shows.
    ///
    /// `help.rs` says in as many words that the browser, the YAML, the schema
    /// and `HOW-TO-USE.md` describe the same sections in the same sequence.
    /// Until 1.0 that sentence was false — the document was still lettered
    /// `A` to `J` while the model had four bundles — and nothing said so. This
    /// is what says so.
    #[cfg(feature = "web")]
    #[test]
    fn the_written_reference_walks_the_sections_in_the_models_own_order() {
        let doc = repo_root().join("HOW-TO-USE.md");
        let text = std::fs::read_to_string(&doc).expect("HOW-TO-USE.md is beside the workspace");

        // Each section of the reference is headed by the dotted path it
        // documents, in backticks, which is also what the validator names in
        // an issue and what the form puts on a card.
        let headed: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("### `"))
            .filter_map(|l| l.split('`').next())
            .collect();

        let expected: Vec<&str> = loopsmith_web::help::SECTIONS.iter().map(|s| s.key).collect();
        let covered: Vec<&str> = headed
            .iter()
            .copied()
            .filter(|k| expected.contains(k))
            .collect();
        assert_eq!(
            covered, expected,
            "HOW-TO-USE.md documents the sections in a different order, or is \
             missing one the model has"
        );

    }

    /// No document still describes the lettered model.
    ///
    /// `A` through `J` were the 0.3 section names, and they are gone from the
    /// model. A document that still teaches them sends a reader looking for a
    /// key that no longer exists — and unlike a moved command, there is no
    /// deprecation notice to catch it, because there is nothing left to run.
    ///
    /// "Formerly section A" is a different sentence and lives in the Rust doc
    /// comments and the generated schema, neither of which this reads: that
    /// one is for a 0.3 author looking for where their key went.
    #[test]
    fn no_document_still_describes_the_lettered_model() {
        let root = repo_root();
        let mut docs = Vec::new();
        documents(&root, &mut docs);

        let mut wrong: Vec<String> = Vec::new();
        for doc in docs {
            let rel = doc.strip_prefix(&root).unwrap_or(&doc).to_string_lossy().replace('\\', "/");
            if KEEPS_THE_OLD_SPELLING.contains(&rel.as_ref()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&doc) else {
                continue;
            };
            for (n, line) in text.lines().enumerate() {
                let lettered = ["A–J", "A-J", "A–H", "A-H"]
                    .iter()
                    .any(|range| line.contains(range))
                    || line.split_whitespace().collect::<Vec<_>>().windows(2).any(|w| {
                        w[0].eq_ignore_ascii_case("section")
                            && w[1].len() == 1
                            && w[1].chars().all(|c| c.is_ascii_uppercase() && c <= 'J')
                    });
                if lettered {
                    wrong.push(format!("{}:{}: {}", rel, n + 1, line.trim()));
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "documents still describing the lettered model:\n{}",
            wrong.join("\n")
        );
    }
}
