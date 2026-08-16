//! Section I — execution guidelines.
//!
//! A guideline is a **phase**: a named stretch of the run with its own standing
//! instruction, and its own place in an ordering. Nodes opt into a phase with
//! `stage:`, and a node's phase must be active before it is dispatched.
//!
//! This is the layer the execution graph deliberately does not have. `graph`
//! edges mean "this node reads that node's output" — a data dependency, and
//! nothing else. Phases express the other kind of ordering, the one that is
//! about method rather than data: *gather before you draft*, *land the tests
//! before you refactor*. Overloading `depends_on` with both would make the
//! critical path meaningless, because half the edges would not be real work
//! dependencies at all.
//!
//! Ordering is written as arrows, because the thing being described is an
//! ordering and a list of `depends_on` arrays reads like a data structure:
//!
//! ```yaml
//! execution_guidelines:
//!   items:
//!     - name: gather
//!       guideline: Collect sources. Write nothing yet.
//!     - name: draft
//!       guideline: Write only from what `gather` collected.
//!   dependency:
//!     - gather -> draft -> review
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ExecutionGuidelines {
    #[serde(default)]
    pub items: Vec<Guideline>,
    /// Ordering, one chain per entry: `a -> b`, or `a -> b -> c`.
    ///
    /// Anything not named here has no predecessor and starts immediately, so
    /// two guidelines with no arrow between them run in parallel. That is the
    /// default on purpose: sequencing should be something you asked for.
    #[serde(default)]
    pub dependency: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Guideline {
    pub name: String,
    /// The standing instruction for this phase, injected into the prompt of
    /// every node that declares `stage: <name>`.
    pub guideline: String,
    /// Optional note for the human reading the config.
    #[serde(default)]
    pub note: Option<String>,
}

/// A guideline resolved into a DAG node: its own name plus the phases it waits
/// on, derived from the arrow list.
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    pub name: String,
    pub guideline: String,
    pub depends_on: Vec<String>,
}

impl ExecutionGuidelines {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&Guideline> {
        self.items.iter().find(|g| g.name == name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.items.iter().map(|g| g.name.as_str()).collect()
    }

    /// Every edge the arrow list declares, in order.
    ///
    /// Errors describe the offending line rather than the offending character,
    /// because the author is looking at a line.
    pub fn edges(&self) -> Result<Vec<(String, String)>, String> {
        let mut out = Vec::new();
        for line in &self.dependency {
            out.extend(parse_chain(line)?);
        }
        Ok(out)
    }

    /// Resolve guidelines and arrows into DAG nodes.
    ///
    /// Does not check for cycles or unknown names — that is the scheduler's
    /// job (it already has Kahn's algorithm) and the validator's.
    pub fn phases(&self) -> Result<Vec<Phase>, String> {
        let edges = self.edges()?;
        Ok(self
            .items
            .iter()
            .map(|g| Phase {
                name: g.name.clone(),
                guideline: g.guideline.clone(),
                depends_on: edges
                    .iter()
                    .filter(|(_, to)| *to == g.name)
                    .map(|(from, _)| from.clone())
                    .collect(),
            })
            .collect())
    }
}

/// `a -> b -> c` becomes `[(a, b), (b, c)]`.
pub fn parse_chain(line: &str) -> Result<Vec<(String, String)>, String> {
    let parts: Vec<&str> = line.split("->").map(str::trim).collect();
    if parts.len() < 2 {
        return Err(format!(
            "`{line}` is not an ordering; write it as `earlier -> later`"
        ));
    }
    if let Some(blank) = parts.iter().position(|p| p.is_empty()) {
        return Err(format!(
            "`{line}` has an empty name at position {}; every `->` needs a guideline on both sides",
            blank + 1
        ));
    }
    Ok(parts
        .windows(2)
        .map(|w| (w[0].to_string(), w[1].to_string()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guidelines(items: &[&str], dependency: &[&str]) -> ExecutionGuidelines {
        ExecutionGuidelines {
            items: items
                .iter()
                .map(|n| Guideline {
                    name: n.to_string(),
                    guideline: format!("do the {n} work"),
                    note: None,
                })
                .collect(),
            dependency: dependency.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn a_two_name_arrow_is_one_edge() {
        assert_eq!(
            parse_chain("gather -> draft").unwrap(),
            vec![("gather".into(), "draft".into())]
        );
    }

    #[test]
    fn a_chain_becomes_consecutive_edges() {
        assert_eq!(
            parse_chain("a -> b -> c").unwrap(),
            vec![("a".into(), "b".into()), ("b".into(), "c".into())]
        );
    }

    #[test]
    fn spacing_around_the_arrow_does_not_matter() {
        assert_eq!(parse_chain("a->b").unwrap(), parse_chain("a  ->  b").unwrap());
    }

    #[test]
    fn a_line_without_an_arrow_is_refused() {
        let err = parse_chain("gather").unwrap_err();
        assert!(err.contains("earlier -> later"), "got: {err}");
    }

    #[test]
    fn a_dangling_arrow_is_refused() {
        for line in ["a ->", "-> b", "a -> -> c"] {
            let err = parse_chain(line).unwrap_err();
            assert!(err.contains("empty name"), "for `{line}`, got: {err}");
        }
    }

    #[test]
    fn phases_carry_the_predecessors_the_arrows_named() {
        let g = guidelines(&["gather", "draft", "review"], &["gather -> draft -> review"]);
        let phases = g.phases().unwrap();
        assert_eq!(phases[0].depends_on, Vec::<String>::new());
        assert_eq!(phases[1].depends_on, vec!["gather"]);
        assert_eq!(phases[2].depends_on, vec!["draft"]);
    }

    #[test]
    fn guidelines_with_no_arrow_between_them_are_independent() {
        // Two unrelated phases must not acquire an accidental ordering just by
        // being written one after the other.
        let g = guidelines(&["seo", "social"], &[]);
        let phases = g.phases().unwrap();
        assert!(phases.iter().all(|p| p.depends_on.is_empty()));
    }

    #[test]
    fn one_phase_can_wait_on_several() {
        let g = guidelines(
            &["a", "b", "publish"],
            &["a -> publish", "b -> publish"],
        );
        let publish = &g.phases().unwrap()[2];
        assert_eq!(publish.depends_on, vec!["a", "b"]);
    }
}
