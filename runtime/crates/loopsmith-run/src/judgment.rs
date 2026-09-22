//! Turning a judge node's prose into verdicts the gate can act on.
//!
//! Without this, a `judge` detector always fails closed with "no judgment
//! recorded" — the judge node runs, its output is stored, and nothing ever
//! reads it. That is worse than useless: it looks like rigour while making
//! every subjective validation permanently unsatisfiable.
//!
//! The contract is a block format rather than JSON because judges are
//! language models writing prose, and asking for strict JSON in the middle of
//! an explanation is how you get truncated objects. Unparseable output is
//! reported as such rather than guessed at.
//!
//! ```text
//! VERDICT: every-claim-cited PASS
//! STANDARD: the citation policy in AGENTS.md
//! EVIDENCE: all 14 claims carry a source line; checked lines 12-96
//! SCORE: 9
//! ```

use loopsmith_gate::Judgment;

/// The instruction appended to every judge node's prompt. Kept next to the
/// parser so the two cannot drift apart.
pub const JUDGE_OUTPUT_CONTRACT: &str = "\
## Required output format

Emit one block per check, exactly in this shape. Text outside these blocks is\n\
ignored, so put your reasoning in EVIDENCE where it will be read.

VERDICT: <check-name> PASS|FAIL
STANDARD: <the external standard you checked against>
EVIDENCE: <specific evidence — quote or cite, do not summarise>
SCORE: <0-10, optional>

A verdict without evidence cannot be acted on. If you cannot check something,\n\
say FAIL and give the reason as evidence rather than passing it by default.";

/// Parse judge output into judgments.
///
/// `judge_provider` and `builder_provider` come from the episode record, not
/// from the text — a judge cannot be trusted to report which model it was.
pub fn parse(
    text: &str,
    judge_provider: &str,
    builder_provider: &str,
) -> Vec<Judgment> {
    let mut out: Vec<Judgment> = Vec::new();
    let mut current: Option<Judgment> = None;

    for raw in text.lines() {
        let line = raw.trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_uppercase().as_str() {
            "VERDICT" => {
                if let Some(j) = current.take() {
                    out.push(j);
                }
                let mut parts = value.rsplitn(2, char::is_whitespace);
                let decision = parts.next().unwrap_or("").trim();
                let name = parts.next().unwrap_or("").trim();
                if name.is_empty() {
                    continue;
                }
                let passed = decision.eq_ignore_ascii_case("PASS");
                let recognised =
                    passed || decision.eq_ignore_ascii_case("FAIL") || decision.eq_ignore_ascii_case("NEEDS_REVISION");
                if !recognised {
                    continue;
                }
                current = Some(Judgment {
                    validation: name.to_string(),
                    provider_id: judge_provider.to_string(),
                    builder_provider_id: builder_provider.to_string(),
                    passed,
                    score: None,
                    standard: String::new(),
                    evidence: String::new(),
                });
            }
            "STANDARD" => {
                if let Some(j) = current.as_mut() {
                    j.standard = value.to_string();
                }
            }
            "EVIDENCE" => {
                if let Some(j) = current.as_mut() {
                    j.evidence = value.to_string();
                }
            }
            "SCORE" => {
                if let Some(j) = current.as_mut() {
                    // Judges write scores as `8`, `8.5`, `8/10`, or `8 out of
                    // 10`. Take the numerator in every case.
                    j.score = value
                        .split_whitespace()
                        .next()
                        .and_then(|tok| tok.split('/').next())
                        .map(|s| s.trim_end_matches('.'))
                        .and_then(|s| s.parse::<f64>().ok());
                }
            }
            _ => {}
        }
    }
    if let Some(j) = current.take() {
        out.push(j);
    }

    // A pass with no evidence is not a judgment, it is an assertion. Demote it
    // rather than let it satisfy the gate.
    for j in out.iter_mut() {
        if j.passed && j.evidence.trim().is_empty() {
            j.passed = false;
            j.evidence = "no evidence supplied; a bare PASS cannot be acted on".into();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_block_parses() {
        let text = "\
VERDICT: every-claim-cited PASS
STANDARD: the citation policy
EVIDENCE: all 14 claims carry a source line
SCORE: 9";
        let j = parse(text, "openai", "claude");
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].validation, "every-claim-cited");
        assert!(j[0].passed);
        assert_eq!(j[0].score, Some(9.0));
        assert_eq!(j[0].provider_id, "openai");
        assert_eq!(j[0].builder_provider_id, "claude");
    }

    #[test]
    fn several_blocks_parse_independently() {
        let text = "\
VERDICT: a PASS
EVIDENCE: fine
VERDICT: b FAIL
EVIDENCE: missing section 3";
        let j = parse(text, "p1", "p2");
        assert_eq!(j.len(), 2);
        assert!(j[0].passed);
        assert!(!j[1].passed);
        assert_eq!(j[1].evidence, "missing section 3");
    }

    #[test]
    fn prose_around_the_blocks_is_ignored() {
        let text = "\
I looked at this carefully and here is what I found.

VERDICT: check-one PASS
EVIDENCE: line 4 matches

Hope that helps!";
        let j = parse(text, "p1", "p2");
        assert_eq!(j.len(), 1);
        assert_eq!(j[0].validation, "check-one");
    }

    #[test]
    fn a_bare_pass_without_evidence_is_demoted_to_fail() {
        // The whole point of a judge is the evidence. A PASS with none is an
        // assertion, and letting it through would reopen the exact hole the
        // gate exists to close.
        let j = parse("VERDICT: x PASS", "p1", "p2");
        assert_eq!(j.len(), 1);
        assert!(!j[0].passed);
        assert!(j[0].evidence.contains("no evidence"));
    }

    #[test]
    fn a_fail_needs_no_evidence_to_stay_a_fail() {
        let j = parse("VERDICT: x FAIL", "p1", "p2");
        assert!(!j[0].passed);
    }

    #[test]
    fn needs_revision_counts_as_not_passed() {
        let j = parse("VERDICT: x NEEDS_REVISION\nEVIDENCE: close", "p1", "p2");
        assert_eq!(j.len(), 1);
        assert!(!j[0].passed);
    }

    #[test]
    fn an_unrecognised_decision_word_is_skipped_not_guessed() {
        let j = parse("VERDICT: x PROBABLY\nEVIDENCE: hmm", "p1", "p2");
        assert!(j.is_empty(), "must not invent a verdict");
    }

    #[test]
    fn unparseable_output_yields_nothing_so_the_gate_fails_closed() {
        for text in ["", "the code looks fine to me", "{\"verdict\": \"pass\"}"] {
            assert!(parse(text, "p1", "p2").is_empty(), "{text:?}");
        }
    }

    #[test]
    fn lowercase_keys_and_decisions_are_accepted() {
        let j = parse("verdict: x pass\nevidence: fine", "p1", "p2");
        assert_eq!(j.len(), 1);
        assert!(j[0].passed);
    }

    #[test]
    fn a_score_written_as_a_fraction_still_parses() {
        let j = parse("VERDICT: x PASS\nEVIDENCE: e\nSCORE: 8/10", "p1", "p2");
        assert_eq!(j[0].score, Some(8.0));
    }

    #[test]
    fn a_hyphenated_check_name_survives() {
        let j = parse("VERDICT: no-broken-links PASS\nEVIDENCE: 41 links resolved", "p1", "p2");
        assert_eq!(j[0].validation, "no-broken-links");
    }
}
