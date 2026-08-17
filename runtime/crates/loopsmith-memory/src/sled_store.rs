//! `sled` implementation of [`Store`].
//!
//! Key layout is prefix-and-zero-padded so `scan_prefix` returns records in
//! insertion order without a secondary index:
//!
//! ```text
//! ep/<run>/<seq:020>      episode
//! gs/<run>/<target>       goal state
//! lg/<run>/<seq:020>      ledger entry
//! ck/<run>                checkpoint
//! sp/<run>/<key>          scratchpad
//! su/<run>/<iter:020>     iteration summary
//! st/<seq:020>            skill trial (global, deliberately not per run)
//! pr/<run>/<seq:020>      proposal
//! ```

use crate::{
    Checkpoint, Episode, GoalState, IterationSummary, LedgerEntry, MemError, Proposal, Result,
    SkillTrial, Store,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub struct SledStore {
    db: sled::Db,
}

impl SledStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path).map_err(|e| MemError::Backend(e.to_string()))?;
        Ok(Self { db })
    }

    /// Monotonic sequence shared by every keyspace; only ordering matters.
    fn next_seq(&self) -> Result<u64> {
        self.db
            .generate_id()
            .map_err(|e| MemError::Backend(e.to_string()))
    }

    fn put(&self, key: String, value: Vec<u8>) -> Result<()> {
        self.db
            .insert(key.as_bytes(), value)
            .map_err(|e| MemError::Backend(e.to_string()))?;
        Ok(())
    }

    fn scan<T: serde::de::DeserializeOwned>(&self, prefix: &str) -> Result<Vec<T>> {
        let mut out = Vec::new();
        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, v) = item.map_err(|e| MemError::Backend(e.to_string()))?;
            out.push(serde_json::from_slice::<T>(&v)?);
        }
        Ok(out)
    }
}

impl Store for SledStore {
    fn put_episode(&self, ep: &Episode) -> Result<u64> {
        // Validate before writing — bad data compounds.
        ep.check()?;
        let seq = self.next_seq()?;
        self.put(
            format!("ep/{}/{:020}", ep.run_id, seq),
            serde_json::to_vec(ep)?,
        )?;
        Ok(seq)
    }

    fn episodes(&self, run_id: &str) -> Result<Vec<Episode>> {
        self.scan(&format!("ep/{run_id}/"))
    }

    fn set_goal_state(&self, run_id: &str, st: &GoalState) -> Result<()> {
        if st.target.trim().is_empty() {
            return Err(MemError::Rejected("goal state target is empty".into()));
        }
        if st.total < st.passed + st.failed {
            return Err(MemError::Rejected(format!(
                "goal state for `{}` is inconsistent: passed {} + failed {} exceeds total {}",
                st.target, st.passed, st.failed, st.total
            )));
        }
        self.put(
            format!("gs/{run_id}/{}", st.target),
            serde_json::to_vec(st)?,
        )
    }

    fn goal_state(&self, run_id: &str, target: &str) -> Result<Option<GoalState>> {
        let v = self
            .db
            .get(format!("gs/{run_id}/{target}").as_bytes())
            .map_err(|e| MemError::Backend(e.to_string()))?;
        match v {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn goal_states(&self, run_id: &str) -> Result<BTreeMap<String, GoalState>> {
        let list: Vec<GoalState> = self.scan(&format!("gs/{run_id}/"))?;
        Ok(list.into_iter().map(|g| (g.target.clone(), g)).collect())
    }

    fn append_ledger(&self, entry: &LedgerEntry) -> Result<u64> {
        if entry.run_id.trim().is_empty() {
            return Err(MemError::Rejected("ledger entry run_id is empty".into()));
        }
        let seq = self.next_seq()?;
        self.put(
            format!("lg/{}/{:020}", entry.run_id, seq),
            serde_json::to_vec(entry)?,
        )?;
        Ok(seq)
    }

    fn ledger(&self, run_id: &str) -> Result<Vec<LedgerEntry>> {
        self.scan(&format!("lg/{run_id}/"))
    }

    fn save_checkpoint(&self, cp: &Checkpoint) -> Result<()> {
        self.put(format!("ck/{}", cp.run_id), serde_json::to_vec(cp)?)?;
        // A checkpoint is the resume contract; make it durable immediately
        // rather than trusting the background flusher to beat a crash.
        self.flush()
    }

    fn checkpoint(&self, run_id: &str) -> Result<Option<Checkpoint>> {
        let v = self
            .db
            .get(format!("ck/{run_id}").as_bytes())
            .map_err(|e| MemError::Backend(e.to_string()))?;
        match v {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    fn set_scratchpad(&self, run_id: &str, key: &str, value: &str) -> Result<()> {
        self.put(format!("sp/{run_id}/{key}"), value.as_bytes().to_vec())
    }

    fn scratchpad(&self, run_id: &str, key: &str) -> Result<Option<String>> {
        let v = self
            .db
            .get(format!("sp/{run_id}/{key}").as_bytes())
            .map_err(|e| MemError::Backend(e.to_string()))?;
        Ok(v.map(|b| String::from_utf8_lossy(&b).to_string()))
    }

    fn put_summary(&self, s: &IterationSummary) -> Result<()> {
        if s.run_id.trim().is_empty() {
            return Err(MemError::Rejected("iteration summary has no run_id".into()));
        }
        // Keyed by iteration rather than by sequence: re-summarising an
        // iteration must replace it, not append a second version that a later
        // read would silently include twice.
        self.put(
            format!("su/{}/{:020}", s.run_id, s.iteration),
            serde_json::to_vec(s)?,
        )?;
        Ok(())
    }

    fn summaries(&self, run_id: &str) -> Result<Vec<IterationSummary>> {
        self.scan(&format!("su/{run_id}/"))
    }

    fn put_skill_trial(&self, t: &SkillTrial) -> Result<u64> {
        if t.skill.trim().is_empty() {
            return Err(MemError::Rejected("skill trial has no skill name".into()));
        }
        if !(0.0..=1.0).contains(&t.pass_rate) {
            return Err(MemError::Rejected(format!(
                "skill trial pass_rate {} is outside 0.0..=1.0",
                t.pass_rate
            )));
        }
        let seq = self.next_seq()?;
        // Keyed globally rather than per run: a skill's track record is only
        // meaningful across runs.
        self.put(format!("st/{seq:020}"), serde_json::to_vec(t)?)?;
        Ok(seq)
    }

    fn skill_trials(&self) -> Result<Vec<SkillTrial>> {
        self.scan("st/")
    }

    fn put_proposal(&self, p: &Proposal) -> Result<u64> {
        if p.run_id.trim().is_empty() {
            return Err(MemError::Rejected("proposal has no run_id".into()));
        }
        let seq = self.next_seq()?;
        self.put(
            format!("pr/{}/{:020}", p.run_id, seq),
            serde_json::to_vec(p)?,
        )?;
        Ok(seq)
    }

    fn proposals(&self, run_id: &str) -> Result<Vec<Proposal>> {
        self.scan(&format!("pr/{run_id}/"))
    }

    fn runs(&self) -> Result<Vec<String>> {
        let mut set = BTreeSet::new();
        for item in self.db.scan_prefix(b"ck/") {
            let (k, _) = item.map_err(|e| MemError::Backend(e.to_string()))?;
            if let Some(rest) = String::from_utf8_lossy(&k).strip_prefix("ck/") {
                set.insert(rest.to_string());
            }
        }
        Ok(set.into_iter().collect())
    }

    fn flush(&self) -> Result<()> {
        self.db
            .flush()
            .map_err(|e| MemError::Backend(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{now_ms, sample_episode, LedgerKind};
    use loopsmith_util::testing::temp_path as tmp_dir;

    fn tmp(tag: &str) -> (SledStore, std::path::PathBuf) {
        let p = tmp_dir(tag);
        (SledStore::open(&p).unwrap(), p)
    }

    #[test]
    fn episodes_round_trip_in_order() {
        let (s, p) = tmp("episodes");
        for i in 0..5 {
            let mut e = sample_episode("r1", &format!("n{i}"));
            e.iteration = i;
            s.put_episode(&e).unwrap();
        }
        let got = s.episodes("r1").unwrap();
        assert_eq!(got.len(), 5);
        assert_eq!(got[0].node_id, "n0");
        assert_eq!(got[4].node_id, "n4");
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn malformed_episodes_are_rejected_not_stored() {
        let (s, p) = tmp("malformed");
        let mut bad = sample_episode("r1", "n1");
        bad.provider_id = "".into();
        assert!(matches!(s.put_episode(&bad), Err(MemError::Rejected(_))));
        assert!(s.episodes("r1").unwrap().is_empty());
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn inconsistent_goal_state_is_rejected() {
        let (s, p) = tmp("goalstate");
        let st = GoalState {
            target: "g1".into(),
            satisfied: true,
            passed: 3,
            failed: 3,
            total: 4,
            reason: "bogus".into(),
            iteration: 1,
            updated_ms: now_ms(),
        };
        assert!(matches!(
            s.set_goal_state("r1", &st),
            Err(MemError::Rejected(_))
        ));
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn runs_are_discovered_from_checkpoints() {
        let (s, p) = tmp("runs");
        for r in ["alpha", "beta"] {
            s.save_checkpoint(&Checkpoint {
                iteration: 1,
                ..Checkpoint::new(r)
            })
            .unwrap();
        }
        assert_eq!(s.runs().unwrap(), vec!["alpha", "beta"]);
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn checkpoint_survives_reopen() {
        let dir = tmp_dir("resume");
        {
            let s = SledStore::open(&dir).unwrap();
            s.save_checkpoint(&Checkpoint {
                iteration: 7,
                completed_nodes: vec!["a".into(), "b".into()],
                tokens_used: 1234,
                cost_usd: 0.5,
                revisions: [("a".to_string(), 2u32)].into_iter().collect(),
                stale_iterations: 3,
                last_signature: "g1=false".into(),
                ..Checkpoint::new("r1")
            })
            .unwrap();
            s.append_ledger(&LedgerEntry {
                run_id: "r1".into(),
                iteration: 7,
                kind: LedgerKind::IterationStarted,
                detail: "seven".into(),
                node_id: None,
                tokens: None,
                cost_usd: None,
                created_ms: now_ms(),
            })
            .unwrap();
            s.flush().unwrap();
        }
        // Drop and reopen: this is the resume-after-crash path.
        let s2 = SledStore::open(&dir).unwrap();
        let cp = s2.checkpoint("r1").unwrap().expect("checkpoint survives");
        assert_eq!(cp.iteration, 7);
        assert_eq!(cp.completed_nodes, vec!["a", "b"]);
        // The stop gates' own accounting has to survive too, or a loop that
        // resumes often gets its revision budget back every time.
        assert_eq!(cp.revisions.get("a"), Some(&2));
        assert_eq!(cp.stale_iterations, 3);
        assert_eq!(cp.last_signature, "g1=false");
        assert_eq!(s2.ledger("r1").unwrap().len(), 1);
        drop(s2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn skill_trials_accumulate_across_runs_and_rank_by_outcome() {
        let (s, p) = tmp("trials");
        let mk = |run: &str, skill: &str, ok: bool, rate: f64| SkillTrial {
            run_id: run.into(),
            iteration: 1,
            node_id: "n1".into(),
            skill: skill.into(),
            source: "marketplace".into(),
            pass_rate: rate,
            satisfied: ok,
            tokens: None,
            created_ms: now_ms(),
        };
        // `good` helps in both runs; `bad` never does.
        s.put_skill_trial(&mk("r1", "good", true, 1.0)).unwrap();
        s.put_skill_trial(&mk("r2", "good", true, 0.9)).unwrap();
        s.put_skill_trial(&mk("r1", "bad", false, 0.2)).unwrap();
        s.put_skill_trial(&mk("r2", "bad", false, 0.1)).unwrap();

        let scored = crate::score_skills(&s.skill_trials().unwrap());
        assert_eq!(scored[0].skill, "good");
        assert_eq!(scored[0].trials, 2);
        assert!((scored[0].satisfaction_rate() - 1.0).abs() < 1e-9);
        assert_eq!(scored[1].skill, "bad");
        assert!(scored[1].satisfaction_rate() < 0.01);
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn an_out_of_range_pass_rate_is_rejected() {
        let (s, p) = tmp("badtrial");
        let t = SkillTrial {
            run_id: "r1".into(),
            iteration: 1,
            node_id: "n".into(),
            skill: "x".into(),
            source: "installed".into(),
            pass_rate: 1.7,
            satisfied: true,
            tokens: None,
            created_ms: now_ms(),
        };
        assert!(matches!(s.put_skill_trial(&t), Err(MemError::Rejected(_))));
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn proposals_are_scoped_per_run() {
        let (s, p) = tmp("proposals");
        let mk = |run: &str, subject: &str| crate::Proposal {
            run_id: run.into(),
            iteration: 2,
            kind: crate::ProposalKind::AdoptSkill,
            subject: subject.into(),
            rationale: "it correlates with satisfied goals".into(),
            patch: Some("skills: [x]".into()),
            created_ms: now_ms(),
            expires_ms: None,
        };
        s.put_proposal(&mk("r1", "a")).unwrap();
        s.put_proposal(&mk("r1", "b")).unwrap();
        s.put_proposal(&mk("r2", "c")).unwrap();
        assert_eq!(s.proposals("r1").unwrap().len(), 2);
        assert_eq!(s.proposals("r2").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn scratchpad_carries_reasoning_between_iterations() {
        let (s, p) = tmp("scratchpad");
        s.set_scratchpad("r1", "g1", "depth 0 reasoning").unwrap();
        assert_eq!(
            s.scratchpad("r1", "g1").unwrap().as_deref(),
            Some("depth 0 reasoning")
        );
        assert!(s.scratchpad("r1", "missing").unwrap().is_none());
        let _ = std::fs::remove_dir_all(p);
    }
}
