//! Reading a run's own log back, so the browser can show what is happening.
//!
//! A run in the browser is a subprocess, and the only thing crossing that
//! boundary is its output. That is on purpose — it is what stops the web UI
//! from drifting away from the CLI — but it means a console full of scrolling
//! text is all the browser has, and a console is a poor way to answer "is it
//! nearly done?".
//!
//! So the lines are read. With `--verbose` a run mirrors its ledger to stderr
//! through `loopsmith_run::logging::line`, one entry per line, in fixed
//! columns; this turns each one back into what it was. Nothing is invented
//! here and nothing is a second source of truth: an event exists only because
//! the ledger recorded one, so the run view and the ledger cannot disagree.
//!
//! The parse lives in Rust rather than in the browser for the ordinary reason:
//! the format is Rust's, and a regex in TypeScript would be a copy of it that
//! nothing checks. `the_run_log_is_a_format_the_web_can_read` is what holds
//! the two ends together: it asks the writer for a line of every kind the
//! ledger has and requires this to give back what went in.

use serde::Serialize;

/// One thing that happened during a run, as the ledger recorded it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    /// Which pass over the graph. `0` is everything before the first one.
    pub iteration: u32,
    /// The `LedgerKind` that produced it, verbatim: `NodeDispatched`,
    /// `GateEvaluated`, `StateChanged`, and the rest. Left as the Rust name
    /// rather than translated, so a kind added over there arrives here
    /// unannounced instead of being silently dropped by a stale mapping.
    pub kind: String,
    /// The node it concerns, where it concerns one.
    pub node: Option<String>,
    pub detail: String,
    /// For a `StateChanged`, the state the run moved *to*.
    pub state: Option<String>,
    /// For a `StateChanged`, the state it moved *from*.
    ///
    /// Only the first transition of a run says anything the trail does not
    /// already know — but that one matters: without it the trail opens on
    /// `validating` and reads as though something was missed.
    pub from: Option<String>,
    /// Why, where the entry gives a reason: the part after the colon in
    /// "running → blocked: no measurable change for 3 iterations".
    pub why: Option<String>,
    /// The gate's running score, from the sentence it closes an iteration
    /// with: `(passed, total)` out of "2/4 target(s) satisfied".
    ///
    /// This one reads a sentence the gate writes rather than the frame
    /// `logging::line` puts around it, so it is the weaker of the two
    /// contracts here — but it is the single number that says whether a run
    /// is getting anywhere, and it is buried in prose either way. Better
    /// buried in prose on this side of the wire, where a test can see it.
    pub satisfied: Option<(u32, u32)>,
}

/// Turn one line of a verbose run log back into the entry it came from.
///
/// `None` for anything that is not one — the closing summary, a provider's
/// own stderr, a panic. Those still reach the console; they are simply not
/// events.
pub fn parse(line: &str) -> Option<Event> {
    // `<timestamp>  it <n>  <Kind> [<node>] <detail>`, split on whitespace
    // runs rather than by column offset: the iteration counter is right
    // aligned in three characters and overflows on iteration 1000, and a run
    // that long is exactly the kind that is worth watching.
    if !line.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let (stamp, after) = line.split_once("  ")?;
    // A timestamp, not prose that happens to start with a digit.
    if stamp.len() != 20 || !stamp.ends_with('Z') {
        return None;
    }
    let after_it = after.trim_start().strip_prefix("it ")?;
    let (iteration, after_iteration) = {
        let t = after_it.trim_start();
        let end = t.find(' ')?;
        (t[..end].parse::<u32>().ok()?, &t[end..])
    };

    let t = after_iteration.trim_start();
    let (kind, after_kind) = match t.find(' ') {
        Some(end) => (&t[..end], t[end..].trim_start()),
        None => (t, ""),
    };
    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }

    let (node, detail) = match after_kind.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        Some((node, detail)) => (Some(node.to_string()), detail),
        None => (None, after_kind),
    };

    let moved = (kind == "StateChanged").then(|| transition(detail)).flatten();
    Some(Event {
        iteration,
        state: moved.as_ref().map(|m| m.1.clone()),
        from: moved.as_ref().and_then(|m| m.0.clone()),
        why: moved.and_then(|m| m.2),
        satisfied: satisfied(detail),
        kind: kind.to_string(),
        node,
        detail: detail.to_string(),
    })
}

/// A `StateChanged`'s detail, taken apart: from, to, and why.
///
/// It reads `running → blocked: no measurable change for 3 iterations`. The
/// arrow separates the two states and the colon separates the destination
/// from the reason. `None` where there is no arrow, which would mean the
/// engine changed the sentence without telling anyone.
#[allow(clippy::type_complexity)]
fn transition(detail: &str) -> Option<(Option<String>, String, Option<String>)> {
    let (before, after) = detail.split_once('→')?;
    let (to, why) = match after.split_once(':') {
        Some((to, why)) => (to.trim(), Some(why.trim().to_string())),
        None => (after.trim(), None),
    };
    if to.is_empty() {
        return None;
    }
    let from = before.trim();
    Some(((!from.is_empty()).then(|| from.to_string()), to.to_string(), why))
}

/// The gate's score out of "… 2/4 target(s) satisfied."
fn satisfied(detail: &str) -> Option<(u32, u32)> {
    let at = detail.find(" target(s) satisfied")?;
    let counts = detail[..at].rsplit(' ').next()?;
    let (passed, total) = counts.split_once('/')?;
    Some((passed.parse().ok()?, total.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_memory::{LedgerEntry, LedgerKind};

    /// Every kind of entry the engine writes survives the round trip.
    ///
    /// This is the seam. `loopsmith_run::logging::line` is the writer and
    /// [`parse`] is the reader, and nothing in the type system connects them
    /// — the connection is a subprocess's stderr. So the writer is asked for
    /// a line of each kind, and the reader has to give back what went in.
    ///
    /// The `match` has no wildcard arm on purpose: a kind added to the ledger
    /// stops this compiling, which is the moment to decide what the run view
    /// should do with it.
    #[test]
    fn the_run_log_is_a_format_the_web_can_read() {
        use LedgerKind::*;
        const ALL: [LedgerKind; 18] = [
            RunStarted,
            IterationStarted,
            NodeDispatched,
            NodeSucceeded,
            NodeFailed,
            GateEvaluated,
            GoalSatisfied,
            GoalRevoked,
            SkillAcquired,
            ProposalWritten,
            StopGateTriggered,
            RunFinished,
            StateChanged,
            Recovered,
            Escalated,
            AlertRaised,
            RuleEvaluated,
            Remembered,
        ];
        fn covered(k: LedgerKind) -> bool {
            match k {
                RunStarted | IterationStarted | NodeDispatched | NodeSucceeded | NodeFailed
                | GateEvaluated | GoalSatisfied | GoalRevoked | SkillAcquired | ProposalWritten
                | StopGateTriggered | RunFinished | StateChanged | Recovered | Escalated
                | AlertRaised | RuleEvaluated | Remembered => true,
            }
        }

        for kind in ALL {
            assert!(covered(kind));
            let entry = LedgerEntry {
                run_id: "run-1".into(),
                iteration: 7,
                kind,
                // A newline in a detail would split one entry over two lines,
                // and the reader counts lines.
                detail: "running \u{2192} blocked: two\nlines".into(),
                node_id: Some("node-a".into()),
                tokens: None,
                cost_usd: None,
                created_ms: 1_790_344_651_347,
            };
            let text = loopsmith_run::logging::line(&entry);
            assert!(!text.contains('\n'), "one entry became two lines: {text}");

            let back = parse(&text).unwrap_or_else(|| panic!("`{text}` did not parse"));
            assert_eq!(back.iteration, 7);
            assert_eq!(back.kind, format!("{kind:?}"));
            assert_eq!(back.node.as_deref(), Some("node-a"));
            assert_eq!(back.detail, "running \u{2192} blocked: two lines");
            // Only a transition is taken apart; every other kind keeps its
            // detail whole and says nothing it did not say.
            if matches!(kind, StateChanged) {
                assert_eq!(back.state.as_deref(), Some("blocked"));
                assert_eq!(back.from.as_deref(), Some("running"));
                assert_eq!(back.why.as_deref(), Some("two lines"));
            } else {
                assert_eq!(back.state, None);
                assert_eq!(back.from, None);
                assert_eq!(back.why, None);
            }
        }
    }

    #[test]
    fn a_dispatch_line_names_the_node_it_dispatched() {
        let e = parse(
            "2026-09-25T13:57:31Z  it   1  NodeDispatched     [survey] dry run: would dispatch `survey`",
        )
        .expect("a run log line");
        assert_eq!(e.iteration, 1);
        assert_eq!(e.kind, "NodeDispatched");
        assert_eq!(e.node.as_deref(), Some("survey"));
        assert!(e.detail.starts_with("dry run:"), "{}", e.detail);
        assert_eq!(e.state, None);
    }

    #[test]
    fn a_transition_carries_the_state_the_run_moved_to() {
        // The lifecycle strip draws this and nothing else, so the reason on
        // the end has to come off.
        let e = parse(
            "2026-09-25T13:57:32Z  it   4  StateChanged       running → blocked: no measurable change for 3 iterations",
        )
        .expect("a run log line");
        assert_eq!(e.state.as_deref(), Some("blocked"));
        assert_eq!(e.from.as_deref(), Some("running"));
        assert_eq!(
            e.why.as_deref(),
            Some("no measurable change for 3 iterations")
        );
        assert_eq!(e.node, None);
    }

    #[test]
    fn the_gates_running_score_is_read_out_of_the_sentence_it_is_buried_in() {
        // The one number that says whether a run is getting anywhere.
        let e = parse(
            "2026-09-25T13:57:31Z  it   2  GateEvaluated      0 node(s) ran (1 failed); 2/4 target(s) satisfied.",
        )
        .expect("a run log line");
        assert_eq!(e.satisfied, Some((2, 4)));
        // The count in the parenthesis must not be mistaken for it.
        assert_eq!(
            parse("2026-09-25T13:57:31Z  it   2  GateEvaluated      3 node(s) ran (1 failed)")
                .expect("a run log line")
                .satisfied,
            None
        );
    }

    #[test]
    fn a_transition_with_no_reason_still_names_its_destination() {
        let e = parse("2026-09-25T13:57:31Z  it   0  StateChanged       created → validating")
            .expect("a run log line");
        assert_eq!(e.state.as_deref(), Some("validating"));
        assert_eq!(e.from.as_deref(), Some("created"));
        assert_eq!(e.why, None);
    }

    #[test]
    fn an_entry_about_no_node_in_particular_says_so() {
        let e = parse(
            "2026-09-25T13:57:31Z  it   0  RunStarted         4 nodes in 3 waves, concurrency 2",
        )
        .expect("a run log line");
        assert_eq!(e.node, None);
        assert_eq!(e.iteration, 0);
        assert_eq!(e.detail, "4 nodes in 3 waves, concurrency 2");
    }

    #[test]
    fn a_four_digit_iteration_is_read_rather_than_running_into_the_kind() {
        // The column is three wide and a long run overflows it. A parser
        // reading by offset would take `1234 IterationStarted` as the kind.
        let e = parse("2026-09-25T13:57:31Z  it 1234  IterationStarted   iteration 1234")
            .expect("a run log line");
        assert_eq!(e.iteration, 1234);
        assert_eq!(e.kind, "IterationStarted");
    }

    #[test]
    fn anything_that_is_not_a_run_log_line_is_not_an_event() {
        // Everything a provider writes to stderr arrives on the same stream.
        for line in [
            "",
            "run run-1790344651347 finished after 4 iteration(s)",
            "error: connection reset by peer",
            "2026-09-25T13:57:31Z something else entirely",
            "12345 it 1 Nope no timestamp",
        ] {
            assert_eq!(parse(line), None, "`{line}` should not be an event");
        }
    }
}
