//! Graph plane: turn a node list into an execution schedule.
//!
//! Three jobs, all deterministic and all cheap enough to run before every
//! iteration:
//!
//! 1. **Cycle detection** — a cyclic graph is a config bug, not a runtime one.
//! 2. **Wave scheduling** — nodes with no unmet dependency run together.
//! 3. **Sizing** — the critical path is the floor on wall-clock, and Amdahl's
//!    law is the cap on what more workers can buy. Both are known before a
//!    single node is dispatched, which is the point: you can decide the fleet
//!    size from arithmetic instead of from optimism.

use loopsmith_core::{Concurrency, GraphSpec, NodeSpec, Phase, Role};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("dependency cycle among nodes: {0}")]
    Cycle(String),
    #[error("unknown node `{0}` referenced as a dependency")]
    UnknownNode(String),
}

/// One scheduling wave: every node in it may run concurrently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wave {
    pub index: usize,
    pub nodes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub waves: Vec<Wave>,
    /// Longest chain of real edges, by accumulated weight.
    pub critical_path: Vec<String>,
    pub critical_path_cost: f64,
    /// Sum of all node weights — the fully serial cost.
    pub total_cost: f64,
    /// Parallel fraction `p`, derived from the graph rather than guessed.
    pub parallel_fraction: f64,
    /// Chosen worker count.
    pub concurrency: usize,
    /// Predicted speedup at the chosen concurrency.
    pub predicted_speedup: f64,
    /// Ceiling no worker count can beat: `1 / (1 - p)`.
    pub speedup_ceiling: f64,
}

/// Amdahl's law. `p` is the parallel fraction, `n` the worker count.
///
/// This is the arithmetic that stops a fleet from being sized by vibes:
/// at p=0.95, sixteen workers buy ×9.14, not ×16.
pub fn amdahl(p: f64, n: usize) -> f64 {
    let p = p.clamp(0.0, 1.0);
    if n == 0 {
        return 0.0;
    }
    let n = n as f64;
    1.0 / ((1.0 - p) + (p / n))
}

/// The ceiling as workers approach infinity.
pub fn speedup_ceiling(p: f64) -> f64 {
    let p = p.clamp(0.0, 1.0);
    if p >= 1.0 {
        f64::INFINITY
    } else {
        1.0 / (1.0 - p)
    }
}

/// The three things scheduling actually needs from a node: what it is called,
/// what it waits for, and what it costs.
///
/// This exists so the scheduler is not welded to `NodeSpec`. The execution
/// graph (section G) and the execution-guideline phase graph (section I) are
/// different types with different fields, but they are the same DAG problem,
/// and a second copy of Kahn's algorithm is a second place for a cycle bug to
/// hide.
pub trait DagNode {
    fn id(&self) -> &str;
    fn deps(&self) -> &[String];
    /// Relative cost, used only for critical-path weighting. Nodes that have
    /// no meaningful cost should return `1.0`.
    fn weight(&self) -> f64;
}

impl DagNode for NodeSpec {
    fn id(&self) -> &str {
        &self.id
    }
    fn deps(&self) -> &[String] {
        &self.depends_on
    }
    fn weight(&self) -> f64 {
        self.weight
    }
}

/// Execution guidelines (section I) are a second DAG over the same scheduler.
/// Phases carry no cost of their own — the work is in the nodes assigned to
/// them — so every phase weighs the same and the critical path through the
/// phase graph is simply its longest chain.
impl DagNode for Phase {
    fn id(&self) -> &str {
        &self.name
    }
    fn deps(&self) -> &[String] {
        &self.depends_on
    }
    fn weight(&self) -> f64 {
        1.0
    }
}

fn index_nodes<N: DagNode>(nodes: &[N]) -> BTreeMap<&str, &N> {
    nodes.iter().map(|n| (n.id(), n)).collect()
}

/// Kahn's algorithm, grouped by level so each level is a wave.
pub fn waves<N: DagNode>(nodes: &[N]) -> Result<Vec<Wave>, GraphError> {
    let by_id = index_nodes(nodes);
    let mut indegree: BTreeMap<&str, usize> = nodes.iter().map(|n| (n.id(), 0)).collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for n in nodes {
        for d in n.deps() {
            if !by_id.contains_key(d.as_str()) {
                return Err(GraphError::UnknownNode(d.clone()));
            }
            *indegree.get_mut(n.id()).unwrap() += 1;
            dependents.entry(d.as_str()).or_default().push(n.id());
        }
    }

    let mut ready: VecDeque<&str> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(id, _)| *id)
        .collect();

    let mut out = Vec::new();
    let mut placed = 0usize;
    let mut index = 0usize;

    while !ready.is_empty() {
        let level: Vec<&str> = ready.drain(..).collect();
        let mut next: BTreeSet<&str> = BTreeSet::new();
        for id in &level {
            placed += 1;
            for dep in dependents.get(id).into_iter().flatten() {
                let e = indegree.get_mut(*dep).unwrap();
                *e -= 1;
                if *e == 0 {
                    next.insert(dep);
                }
            }
        }
        let mut names: Vec<String> = level.iter().map(|s| s.to_string()).collect();
        names.sort();
        out.push(Wave { index, nodes: names });
        index += 1;
        ready = next.into_iter().collect();
    }

    if placed != nodes.len() {
        let stuck: Vec<&str> = indegree
            .iter()
            .filter(|(_, &d)| d > 0)
            .map(|(id, _)| *id)
            .collect();
        return Err(GraphError::Cycle(stuck.join(", ")));
    }
    Ok(out)
}

/// Longest weighted path through the DAG — the floor on wall-clock time no
/// amount of parallelism can lower.
pub fn critical_path<N: DagNode>(nodes: &[N]) -> Result<(Vec<String>, f64), GraphError> {
    let by_id = index_nodes(nodes);
    let order = waves(nodes)?;
    let flat: Vec<&str> = order
        .iter()
        .flat_map(|w| w.nodes.iter().map(|s| s.as_str()))
        .collect();

    let mut best: BTreeMap<&str, f64> = BTreeMap::new();
    let mut prev: BTreeMap<&str, Option<&str>> = BTreeMap::new();

    for id in &flat {
        let node = by_id[id];
        let mut chosen: Option<&str> = None;
        let mut base = 0.0f64;
        for d in node.deps() {
            let c = best[d.as_str()];
            if c > base {
                base = c;
                chosen = Some(d.as_str());
            }
        }
        best.insert(id, base + node.weight());
        prev.insert(id, chosen);
    }

    let end = best
        .iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(id, cost)| (*id, *cost));

    let Some((mut cur, cost)) = end else {
        return Ok((vec![], 0.0));
    };
    let mut path = vec![cur.to_string()];
    while let Some(Some(p)) = prev.get(cur) {
        path.push(p.to_string());
        cur = p;
    }
    path.reverse();
    Ok((path, cost))
}

/// Derive `p` from the graph: the share of total work that is not stuck on the
/// critical path. This is the mechanical form of the "and then" test — work
/// that genuinely reads an upstream output stays serial, everything else is
/// parallelizable.
pub fn parallel_fraction(total_cost: f64, critical_cost: f64) -> f64 {
    if total_cost <= 0.0 {
        return 0.0;
    }
    ((total_cost - critical_cost) / total_cost).clamp(0.0, 1.0)
}

/// Pick a worker count. `Auto` grows the fleet only while the next worker
/// still buys `min_marginal_gain` of additional speedup, then stops — the
/// arithmetic answer to "how many agents should I run?".
pub fn choose_concurrency(
    concurrency: &Concurrency,
    waves: &[Wave],
    p: f64,
) -> (usize, f64) {
    let widest = waves.iter().map(|w| w.nodes.len()).max().unwrap_or(1).max(1);
    match concurrency {
        Concurrency::Sequential => (1, amdahl(p, 1)),
        Concurrency::Fixed { max_parallel } => {
            let n = (*max_parallel).max(1).min(widest);
            (n, amdahl(p, n))
        }
        Concurrency::Auto {
            cap,
            min_marginal_gain,
        } => {
            let ceiling = widest.min((*cap).max(1));
            let mut best = 1usize;
            let mut prev = amdahl(p, 1);
            for n in 2..=ceiling {
                let s = amdahl(p, n);
                if s - prev < *min_marginal_gain {
                    break;
                }
                prev = s;
                best = n;
            }
            (best, amdahl(p, best))
        }
    }
}

/// Full planning pass.
pub fn plan(spec: &GraphSpec) -> Result<Plan, GraphError> {
    let waves = waves(&spec.nodes)?;
    let (critical_path, critical_path_cost) = critical_path(&spec.nodes)?;
    let total_cost: f64 = spec.nodes.iter().map(|n| n.weight).sum();
    let p = parallel_fraction(total_cost, critical_path_cost);
    let (concurrency, predicted_speedup) = choose_concurrency(&spec.concurrency, &waves, p);
    Ok(Plan {
        waves,
        critical_path,
        critical_path_cost,
        total_cost,
        parallel_fraction: p,
        concurrency,
        predicted_speedup,
        speedup_ceiling: speedup_ceiling(p),
    })
}

/// Builder nodes that can land in the same wave without worktree isolation
/// will clobber each other's files. Reported rather than fixed, because the
/// fix is a config decision.
pub fn unisolated_parallel_writers(spec: &GraphSpec, waves: &[Wave]) -> Vec<String> {
    let by_id = index_nodes(&spec.nodes);
    let mut out = Vec::new();
    for w in waves {
        let writers: Vec<&str> = w
            .nodes
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).copied())
            .filter(|n| n.role == Role::Builder && !n.isolated)
            .map(|n| n.id.as_str())
            .collect();
        if writers.len() > 1 {
            out.extend(writers.into_iter().map(|s| s.to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_core::Tier;

    fn node(id: &str, deps: &[&str], weight: f64) -> NodeSpec {
        NodeSpec {
            id: id.into(),
            role: Role::Builder,
            instruction: "a sufficiently long instruction".into(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            goals: vec![],
            tier: Tier::Standard,
            provider: None,
            stage: None,
            skills: vec![],
            weight,
            isolated: false,
        }
    }

    #[test]
    fn independent_nodes_share_one_wave() {
        let nodes = vec![node("a", &[], 1.0), node("b", &[], 1.0), node("c", &[], 1.0)];
        let w = waves(&nodes).unwrap();
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].nodes, vec!["a", "b", "c"]);
    }

    #[test]
    fn a_chain_produces_one_wave_per_link() {
        let nodes = vec![
            node("a", &[], 1.0),
            node("b", &["a"], 1.0),
            node("c", &["b"], 1.0),
        ];
        let w = waves(&nodes).unwrap();
        assert_eq!(w.len(), 3);
    }

    #[test]
    fn cycles_are_detected() {
        let nodes = vec![node("a", &["b"], 1.0), node("b", &["a"], 1.0)];
        let err = waves(&nodes).unwrap_err();
        assert!(matches!(err, GraphError::Cycle(_)));
    }

    #[test]
    fn unknown_dependency_is_reported() {
        let nodes = vec![node("a", &["ghost"], 1.0)];
        assert!(matches!(waves(&nodes), Err(GraphError::UnknownNode(_))));
    }

    #[test]
    fn critical_path_follows_the_heaviest_chain() {
        // a(1) -> b(5) -> d(1)   and   a(1) -> c(1) -> d(1)
        let nodes = vec![
            node("a", &[], 1.0),
            node("b", &["a"], 5.0),
            node("c", &["a"], 1.0),
            node("d", &["b", "c"], 1.0),
        ];
        let (path, cost) = critical_path(&nodes).unwrap();
        assert_eq!(path, vec!["a", "b", "d"]);
        assert_eq!(cost, 7.0);
    }

    #[test]
    fn amdahl_matches_the_published_table() {
        assert!((amdahl(0.95, 16) - 9.142).abs() < 0.01);
        assert!((amdahl(0.70, 16) - 2.909).abs() < 0.01);
        assert!((amdahl(0.95, 256) - 18.618).abs() < 0.01);
        assert!((speedup_ceiling(0.95) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn amdahl_is_one_for_a_single_worker() {
        assert!((amdahl(0.9, 1) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_pure_chain_has_no_parallel_fraction() {
        let nodes = vec![
            node("a", &[], 1.0),
            node("b", &["a"], 1.0),
            node("c", &["b"], 1.0),
        ];
        let (_, cost) = critical_path(&nodes).unwrap();
        let total: f64 = nodes.iter().map(|n| n.weight).sum();
        assert_eq!(parallel_fraction(total, cost), 0.0);
    }

    #[test]
    fn auto_concurrency_stops_when_marginal_gain_dries_up() {
        let spec = GraphSpec {
            nodes: (0..16).map(|i| node(&format!("n{i}"), &[], 1.0)).collect(),
            concurrency: Concurrency::Auto {
                cap: 16,
                min_marginal_gain: 0.5,
            },
        };
        let p = plan(&spec).unwrap();
        // 16 independent nodes: one wave, high p, but a large min gain should
        // stop well before the cap rather than fanning out for nothing.
        assert!(p.concurrency < 16, "chose {}", p.concurrency);
        assert!(p.concurrency > 1);
    }

    #[test]
    fn sequential_mode_pins_one_worker() {
        let spec = GraphSpec {
            nodes: vec![node("a", &[], 1.0), node("b", &[], 1.0)],
            concurrency: Concurrency::Sequential,
        };
        let p = plan(&spec).unwrap();
        assert_eq!(p.concurrency, 1);
    }

    #[test]
    fn fixed_concurrency_is_capped_by_the_widest_wave() {
        let spec = GraphSpec {
            nodes: vec![node("a", &[], 1.0), node("b", &[], 1.0)],
            concurrency: Concurrency::Fixed { max_parallel: 32 },
        };
        let p = plan(&spec).unwrap();
        assert_eq!(p.concurrency, 2);
    }

    #[test]
    fn parallel_writers_without_isolation_are_reported() {
        let spec = GraphSpec {
            nodes: vec![node("a", &[], 1.0), node("b", &[], 1.0)],
            concurrency: Concurrency::default(),
        };
        let w = waves(&spec.nodes).unwrap();
        let flagged = unisolated_parallel_writers(&spec, &w);
        assert_eq!(flagged.len(), 2);
    }

    /// A node type with nothing in common with `NodeSpec` — no role, no
    /// provider, no instruction. If the scheduler still handles it, the
    /// topology layer is genuinely decoupled and the execution-guideline phase
    /// graph does not need its own copy of Kahn's algorithm.
    struct Step {
        id: String,
        deps: Vec<String>,
    }

    impl DagNode for Step {
        fn id(&self) -> &str {
            &self.id
        }
        fn deps(&self) -> &[String] {
            &self.deps
        }
        fn weight(&self) -> f64 {
            1.0
        }
    }

    fn step(id: &str, deps: &[&str]) -> Step {
        Step {
            id: id.into(),
            deps: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn any_dag_node_type_schedules_through_the_same_code() {
        let steps = vec![
            step("research", &[]),
            step("outline", &["research"]),
            step("draft", &["research"]),
            step("publish", &["outline", "draft"]),
        ];
        let w = waves(&steps).unwrap();
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].nodes, vec!["research"]);
        assert_eq!(w[1].nodes, vec!["draft", "outline"]);
        assert_eq!(w[2].nodes, vec!["publish"]);

        let (path, cost) = critical_path(&steps).unwrap();
        assert_eq!(cost, 3.0);
        assert_eq!(path.last().unwrap(), "publish");
    }

    #[test]
    fn a_cycle_in_a_foreign_node_type_is_still_caught() {
        let steps = vec![step("a", &["b"]), step("b", &["a"])];
        assert!(matches!(waves(&steps), Err(GraphError::Cycle(_))));
    }

    #[test]
    fn an_unknown_dependency_in_a_foreign_node_type_is_still_caught() {
        let steps = vec![step("a", &["nope"])];
        assert!(matches!(waves(&steps), Err(GraphError::UnknownNode(_))));
    }
}
