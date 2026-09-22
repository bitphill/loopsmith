//! Memory that outlives a run: four namespaces, each with its own retention
//! and its own bar for promotion.
//!
//! The policy is `execution.memory.namespaces`; this module is the only code
//! that reads it, shared by the run engine (which writes failures and
//! procedures) and the MCP server (through which an agent writes facts). The
//! line that matters is between a record being *written* and being
//! *promoted*: anything may be written, but only a promoted record is
//! retrieved into a later prompt, and each namespace decides what promotion
//! takes. A loop that reused its first success as a standing procedure would
//! have learned a superstition.

use crate::{now_ms, Namespace, Record, Result, Store};
use loopsmith_core::{MemoryPolicy, NamespacePolicy, Promotion};

/// The policy one namespace is kept under.
pub fn policy_for(p: &MemoryPolicy, ns: Namespace) -> &NamespacePolicy {
    let n = &p.namespaces;
    match ns {
        Namespace::Episodic => &n.episodic,
        Namespace::Semantic => &n.semantic,
        Namespace::Procedural => &n.procedural,
        Namespace::Failure => &n.failure,
    }
}

/// Something to remember.
pub struct Note<'a> {
    pub namespace: Namespace,
    pub key: &'a str,
    pub content: &'a str,
    pub provenance: Option<&'a str>,
    pub confidence: f64,
    /// The run writing it. A second write from the same run is not
    /// corroboration.
    pub run_id: &'a str,
}

/// What became of a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remembered {
    /// Written. `promoted` is whether it may now be reused; `newly` is
    /// whether this write is what promoted it.
    Written { promoted: bool, newly: bool },
    /// Refused, and why.
    Refused(String),
    /// The namespace is switched off.
    Disabled,
}

/// Write a note under its namespace's policy.
///
/// Episodes have their own table and are not written through here; the
/// episodic namespace's policy governs their retention only.
pub fn remember<S: Store>(store: &S, policy: &MemoryPolicy, note: &Note) -> Result<Remembered> {
    let rules = policy_for(policy, note.namespace);
    if !rules.enabled {
        return Ok(Remembered::Disabled);
    }
    if note.namespace == Namespace::Episodic {
        return Ok(Remembered::Refused(
            "episodes are recorded as episodes, not as records".into(),
        ));
    }
    let provenance = note.provenance.map(str::trim).filter(|p| !p.is_empty());
    if rules.require_provenance && provenance.is_none() {
        return Ok(Remembered::Refused(format!(
            "the {} namespace requires provenance: say where this came from",
            note.namespace.as_str()
        )));
    }

    let now = now_ms();
    let existing = store.record(note.namespace, note.key)?;
    let was_promoted = existing.as_ref().is_some_and(|r| r.promoted);
    let mut runs = existing.as_ref().map(|r| r.runs.clone()).unwrap_or_default();
    if !runs.iter().any(|r| r == note.run_id) {
        runs.push(note.run_id.to_string());
    }
    let promoted = match rules.promotion {
        Promotion::Never => false,
        Promotion::Automatic => true,
        Promotion::RepeatedValidation { times } => runs.len() as u32 >= times,
        // Only `promote` flips it; a write never does.
        Promotion::HumanApproval => was_promoted,
    };
    store.put_record(&Record {
        namespace: note.namespace,
        key: note.key.to_string(),
        content: note.content.to_string(),
        provenance: provenance.map(str::to_string),
        confidence: note.confidence.clamp(0.0, 1.0),
        runs,
        promoted,
        created_ms: existing.as_ref().map_or(now, |r| r.created_ms),
        updated_ms: now,
    })?;
    Ok(Remembered::Written {
        promoted,
        newly: promoted && !was_promoted,
    })
}

/// Promoted records a later prompt may reuse, most recently confirmed first,
/// at most `execution.memory.max_retrieved` of them.
///
/// A switched-off namespace, a record below its namespace's confidence floor,
/// and anything not yet promoted are all left out.
pub fn recall<S: Store>(
    store: &S,
    policy: &MemoryPolicy,
    namespaces: &[Namespace],
) -> Result<Vec<Record>> {
    let mut out = Vec::new();
    for ns in namespaces {
        let rules = policy_for(policy, *ns);
        if !rules.enabled {
            continue;
        }
        out.extend(
            store
                .records(*ns)?
                .into_iter()
                .filter(|r| r.promoted && r.confidence >= rules.min_confidence),
        );
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.updated_ms));
    out.truncate(policy.max_retrieved);
    Ok(out)
}

/// Drop what each namespace's `retention_days` says has expired. Returns how
/// many records and episodes went.
pub fn expire<S: Store>(store: &S, policy: &MemoryPolicy, now: u64) -> Result<usize> {
    let mut dropped = 0;
    for ns in Namespace::ALL {
        let Some(days) = policy_for(policy, ns).retention_days else {
            continue;
        };
        let cutoff = now.saturating_sub(u64::from(days) * 86_400_000);
        if ns == Namespace::Episodic {
            dropped += store.prune_episodes(cutoff)?;
            continue;
        }
        for r in store.records(ns)? {
            if r.updated_ms < cutoff {
                store.remove_record(ns, &r.key)?;
                dropped += 1;
            }
        }
    }
    Ok(dropped)
}

/// A human's approval: promote a record whatever its namespace's rule.
/// Returns whether there was such a record.
pub fn promote<S: Store>(store: &S, ns: Namespace, key: &str) -> Result<bool> {
    let Some(mut r) = store.record(ns, key)? else {
        return Ok(false);
    };
    r.promoted = true;
    r.updated_ms = now_ms();
    store.put_record(&r)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SledStore;
    use loopsmith_core::Namespaces;

    fn store(tag: &str) -> (SledStore, std::path::PathBuf) {
        let p = loopsmith_util::testing::temp_path(tag);
        (SledStore::open(&p).unwrap(), p)
    }

    fn note<'a>(ns: Namespace, run: &'a str) -> Note<'a> {
        Note {
            namespace: ns,
            key: "k",
            content: "the fact",
            provenance: Some("a test"),
            confidence: 0.9,
            run_id: run,
        }
    }

    #[test]
    fn a_procedure_is_promoted_only_after_distinct_runs_corroborate_it() {
        let (s, p) = store("ns-procedural");
        let policy = MemoryPolicy::default(); // procedural: repeated_validation, 3 runs
        let n = |run| note(Namespace::Procedural, run);
        assert_eq!(remember(&s, &policy, &n("r1")).unwrap(), Remembered::Written { promoted: false, newly: false });
        // The same run saying it twice is not a second witness.
        assert_eq!(remember(&s, &policy, &n("r1")).unwrap(), Remembered::Written { promoted: false, newly: false });
        remember(&s, &policy, &n("r2")).unwrap();
        assert_eq!(remember(&s, &policy, &n("r3")).unwrap(), Remembered::Written { promoted: true, newly: true });
        assert_eq!(recall(&s, &policy, &[Namespace::Procedural]).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn a_failure_is_promoted_as_soon_as_it_is_written() {
        let (s, p) = store("ns-failure");
        let got = remember(&s, &MemoryPolicy::default(), &note(Namespace::Failure, "r1")).unwrap();
        assert_eq!(got, Remembered::Written { promoted: true, newly: true });
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn a_fact_without_provenance_is_refused_where_provenance_is_required() {
        let (s, p) = store("ns-provenance");
        let mut n = note(Namespace::Semantic, "r1");
        n.provenance = Some("   ");
        assert!(matches!(remember(&s, &MemoryPolicy::default(), &n).unwrap(), Remembered::Refused(_)));
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn human_approval_is_the_only_road_to_promotion_for_its_namespace() {
        let (s, p) = store("ns-human");
        let policy = MemoryPolicy {
            namespaces: Namespaces {
                semantic: NamespacePolicy {
                    promotion: Promotion::HumanApproval,
                    ..NamespacePolicy::default()
                },
                ..Namespaces::default()
            },
            ..MemoryPolicy::default()
        };
        for run in ["r1", "r2", "r3", "r4"] {
            remember(&s, &policy, &note(Namespace::Semantic, run)).unwrap();
        }
        assert!(recall(&s, &policy, &[Namespace::Semantic]).unwrap().is_empty());
        assert!(promote(&s, Namespace::Semantic, "k").unwrap());
        assert_eq!(recall(&s, &policy, &[Namespace::Semantic]).unwrap().len(), 1);
        // And a later write does not take the approval away.
        remember(&s, &policy, &note(Namespace::Semantic, "r5")).unwrap();
        assert_eq!(recall(&s, &policy, &[Namespace::Semantic]).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn a_record_below_its_confidence_floor_is_never_recalled() {
        let (s, p) = store("ns-floor");
        let mut n = note(Namespace::Failure, "r1");
        n.confidence = 0.5; // default floor is 0.75
        remember(&s, &MemoryPolicy::default(), &n).unwrap();
        assert!(recall(&s, &MemoryPolicy::default(), &[Namespace::Failure]).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn retention_drops_what_has_expired_and_keeps_what_has_not() {
        let (s, p) = store("ns-expire");
        let mut policy = MemoryPolicy::default();
        policy.namespaces.failure.retention_days = Some(1);
        remember(&s, &policy, &note(Namespace::Failure, "r1")).unwrap();
        assert_eq!(expire(&s, &policy, now_ms()).unwrap(), 0);
        let two_days_later = now_ms() + 2 * 86_400_000;
        assert_eq!(expire(&s, &policy, two_days_later).unwrap(), 1);
        assert!(s.records(Namespace::Failure).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(p);
    }
}
