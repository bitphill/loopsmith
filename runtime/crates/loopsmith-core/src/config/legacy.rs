//! Reading a 0.3 config, and rewriting one.
//!
//! There is exactly one description of where each old key went, and it lives
//! in [`MOVES`]. The parser uses it to accept old files; `loopsmith migrate`
//! uses it to rewrite them. That is the whole reason it is a table rather than
//! two pieces of code: a migrator that disagrees with the parser silently
//! produces a file that means something other than what it replaced, and the
//! only way to be sure they agree is for there to be one of them.
//!
//! The transform runs on `serde_yaml::Value`, before typing. Working on
//! untyped values is what lets it move a key it does not understand — the old
//! and new shapes never have to be representable in the same struct, so there
//! is no compatibility shim in the model itself.

use serde_yaml::{Mapping, Value};

/// One old top-level key and the dotted path it now lives at.
///
/// Order matters only for reporting; the moves are independent.
pub const MOVES: &[(&str, &str)] = &[
    // A–E: what the loop is for.
    ("information", "intent.background"),
    ("pre_execution", "intent.prerequisites"),
    ("goals", "intent.goals"),
    ("success", "intent.success"),
    // D, F, H: what constrains it.
    ("validations", "safety.checks"),
    ("stop_gates", "safety.gates.stop"),
    ("constraints", "safety.limits"),
    // G, I, J and the machinery: how it runs.
    ("graph", "execution.graph"),
    ("providers", "execution.providers"),
    ("execution_guidelines", "execution.phases"),
    ("default_skills", "execution.default_skills"),
    ("skills", "execution.skills"),
    ("context", "execution.memory"),
    ("schedules", "execution.triggers.triggers"),
];

/// A key that was moved, for reporting back to the author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub from: &'static str,
    pub to: &'static str,
}

impl std::fmt::Display for Moved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` is now `{}`", self.from, self.to)
    }
}

/// Whether this document uses any 0.3 top-level key.
pub fn is_legacy(doc: &Value) -> bool {
    let Some(map) = doc.as_mapping() else {
        return false;
    };
    MOVES
        .iter()
        .any(|(from, _)| map.contains_key(Value::from(*from)))
}

/// Rewrite a 0.3 document into the 1.0 shape, reporting what moved.
///
/// Keys already in the new shape are left alone, so running this on a file
/// that is half-migrated — or fully migrated — is a no-op. That matters
/// because `migrate --check` runs it on everything.
pub fn migrate(doc: &Value) -> (Value, Vec<Moved>) {
    let Some(src) = doc.as_mapping() else {
        return (doc.clone(), Vec::new());
    };

    let mut out = src.clone();
    let mut moved = Vec::new();

    for (from, to) in MOVES {
        let key = Value::from(*from);
        let Some(value) = out.remove(&key) else {
            continue;
        };
        insert_path(&mut out, to, value);
        moved.push(Moved { from, to });
    }

    // Field-level repairs run last, on the canonical paths, rather than on the
    // value as it was being moved. A markdown config headed `## Graph` writes
    // straight to `execution.graph` and never passes through the legacy key at
    // all, so a repair attached to that key would silently skip it — which is
    // exactly how `isolated: true` survived into a 1.0 document.
    repair_nodes(&mut out);
    repair_triggers(&mut out);

    (Value::Mapping(reorder(out)), moved)
}

/// The order a config's top-level keys are written in.
///
/// A mapping keeps insertion order, and relocating a key appends it — so a
/// migrated file came out with `safety` above `intent` purely because
/// `validations` happened to be removed before `information` was. Nobody would
/// write it that way, and `migrate --write` produces a file people then read.
const KEY_ORDER: &[&str] = &[
    "name",
    "version",
    "description",
    "environment",
    "features",
    "intent",
    "execution",
    "safety",
    "evolution",
];

/// Sort top-level keys into [`KEY_ORDER`], keeping anything unrecognised at the
/// end in the order it arrived.
fn reorder(map: Mapping) -> Mapping {
    let mut out = Mapping::new();
    let mut rest = map;
    for key in KEY_ORDER {
        if let Some(v) = rest.remove(Value::from(*key)) {
            out.insert(Value::from(*key), v);
        }
    }
    for (k, v) in rest {
        out.insert(k, v);
    }
    out
}

/// Follow a dotted path to a mutable value, if every segment exists.
fn at_mut<'a>(root: &'a mut Mapping, path: &str) -> Option<&'a mut Value> {
    let mut parts = path.split('.');
    let mut cur = root.get_mut(Value::from(parts.next()?))?;
    for part in parts {
        cur = cur.as_mapping_mut()?.get_mut(Value::from(part))?;
    }
    Some(cur)
}

/// Nodes carried `isolated: true`, meaning a worktree. 1.0 has three levels,
/// so the boolean becomes the level it always meant.
fn repair_nodes(out: &mut Mapping) {
    let Some(Value::Sequence(nodes)) = at_mut(out, "execution.graph.nodes") else {
        return;
    };
    for node in nodes.iter_mut() {
        *node = migrate_node(std::mem::replace(node, Value::Null));
    }
}

/// 0.3 wrote a bare trigger per entry: `- {type: cron, expr: "…"}`. 1.0 wraps
/// it so the entry can also carry an idempotency key, which it cannot do
/// flattened without giving up `deny_unknown_fields`.
fn repair_triggers(out: &mut Mapping) {
    let Some(Value::Sequence(items)) = at_mut(out, "execution.triggers.triggers") else {
        return;
    };
    for item in items.iter_mut() {
        // Already wrapped? Leave it — this runs over new documents too.
        if item
            .as_mapping()
            .is_some_and(|m| m.contains_key(Value::from("on")))
        {
            continue;
        }
        let mut m = Mapping::new();
        m.insert(Value::from("on"), std::mem::replace(item, Value::Null));
        *item = Value::Mapping(m);
    }
}

fn migrate_node(node: Value) -> Value {
    let Value::Mapping(mut n) = node else {
        return node;
    };
    let Some(old) = n.remove(Value::from("isolated")) else {
        return Value::Mapping(n);
    };
    // Only rewrite when the node does not already say `isolation`, so a
    // half-migrated file does not have its new key clobbered by its old one.
    if !n.contains_key(Value::from("isolation")) {
        let mode = if old.as_bool() == Some(true) {
            "worktree"
        } else {
            "none"
        };
        let mut iso = Mapping::new();
        iso.insert(Value::from("mode"), Value::from(mode));
        n.insert(Value::from("isolation"), Value::Mapping(iso));
    }
    Value::Mapping(n)
}

/// Insert `value` at a dotted path, creating intermediate mappings.
///
/// An existing value at the destination wins: a file that already says
/// `safety.checks` and *also* says `validations` keeps the new one. The old
/// key is still consumed and reported, so the author is told the duplicate was
/// ignored rather than finding out from behaviour.
fn insert_path(root: &mut Mapping, path: &str, value: Value) {
    let mut parts = path.split('.').peekable();
    let mut cursor = root;

    while let Some(part) = parts.next() {
        let key = Value::from(part);
        if parts.peek().is_none() {
            cursor.entry(key).or_insert(value);
            return;
        }
        let slot = cursor
            .entry(key)
            .or_insert_with(|| Value::Mapping(Mapping::new()));
        // A scalar sitting where a bundle should be means the file is not
        // something this transform can repair; leave it and let typing report
        // it with a proper error.
        let Value::Mapping(next) = slot else {
            return;
        };
        cursor = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Value {
        serde_yaml::from_str(s).expect("test yaml parses")
    }

    /// Walk a dotted path. A numeric segment indexes a sequence, so a test can
    /// reach into a list without unwrapping it by hand.
    fn at(doc: &Value, path: &str) -> Option<Value> {
        let mut cur = doc.clone();
        for part in path.split('.') {
            cur = match part.parse::<usize>() {
                Ok(i) => cur.as_sequence()?.get(i)?.clone(),
                Err(_) => cur.get(part)?.clone(),
            };
        }
        Some(cur)
    }

    #[test]
    fn every_legacy_key_lands_where_the_table_says() {
        let doc = yaml(
            "name: t\ninformation: []\npre_execution: []\ngoals: []\nvalidations: []\n\
             success: []\nstop_gates: {max_iterations: 4}\nschedules: []\nconstraints: {}\n\
             execution_guidelines: {}\ndefault_skills: []\ngraph: {}\nproviders: {}\n\
             skills: {}\ncontext: {}\n",
        );
        let (out, moved) = migrate(&doc);
        assert_eq!(moved.len(), MOVES.len(), "every key should have moved");
        for (_, to) in MOVES {
            assert!(at(&out, to).is_some(), "nothing landed at {to}");
        }
        assert_eq!(at(&out, "safety.gates.stop.max_iterations"), Some(4.into()));
        // `name` is not a section and must survive untouched.
        assert_eq!(at(&out, "name"), Some("t".into()));
    }

    #[test]
    fn migrating_an_already_migrated_document_changes_nothing() {
        // `migrate --check` runs this over every file, including new ones.
        let doc = yaml("name: t\nintent:\n  goals: []\nsafety:\n  checks: []\n");
        let (out, moved) = migrate(&doc);
        assert!(moved.is_empty());
        assert_eq!(out, doc);
        assert!(!is_legacy(&doc));
    }

    #[test]
    fn a_bare_trigger_is_wrapped_and_a_wrapped_one_is_left_alone() {
        let doc = yaml(
            "name: t\nschedules:\n  - {type: cron, expr: \"0 2 * * *\"}\n  - on: {type: manual}\n",
        );
        let (out, _) = migrate(&doc);
        let list = at(&out, "execution.triggers.triggers").expect("triggers moved");
        let items = list.as_sequence().expect("a sequence");
        assert_eq!(items.len(), 2);
        for item in items {
            assert!(
                item.get("on").is_some(),
                "every entry should be wrapped exactly once: {item:?}"
            );
            assert!(
                item.get("on").and_then(|o| o.get("on")).is_none(),
                "an already-wrapped entry must not be wrapped twice"
            );
        }
    }

    #[test]
    fn isolated_true_becomes_worktree_and_false_becomes_none() {
        let doc = yaml(
            "name: t\ngraph:\n  nodes:\n    - {id: a, isolated: true}\n    - {id: b, isolated: false}\n    - {id: c}\n",
        );
        let (out, _) = migrate(&doc);
        let nodes = at(&out, "execution.graph.nodes").unwrap();
        let nodes = nodes.as_sequence().unwrap();
        assert_eq!(nodes[0].get("isolation").unwrap().get("mode").unwrap(), "worktree");
        assert_eq!(nodes[1].get("isolation").unwrap().get("mode").unwrap(), "none");
        // A node that never said `isolated` gets no key, and picks up the
        // struct default instead.
        assert!(nodes[2].get("isolation").is_none());
        for n in nodes {
            assert!(n.get("isolated").is_none(), "the old key must be consumed");
        }
    }

    #[test]
    fn a_new_key_wins_over_the_legacy_key_it_replaced() {
        // Both spellings present is a mistake, but a silent one is worse: the
        // new key is what the author most recently wrote, so it wins, and the
        // move is still reported so they learn the old one did nothing.
        let doc = yaml("name: t\nvalidations: [{old: 1}]\nsafety:\n  checks: [{new: 1}]\n");
        let (out, moved) = migrate(&doc);
        assert_eq!(at(&out, "safety.checks.0.new"), Some(1.into()));
        assert!(moved.iter().any(|m| m.from == "validations"));
    }

    #[test]
    fn a_migrated_document_is_ordered_the_way_a_person_would_write_it() {
        // Relocation appends, so without the reorder the bundle that happened
        // to be created first wins — which put `safety` above `intent` purely
        // because `validations` sorts before `information` in the move table.
        let doc = yaml("validations: []\ngoals: []\nname: t\ngraph: {}\n");
        let (out, _) = migrate(&doc);
        let keys: Vec<&str> = out
            .as_mapping()
            .unwrap()
            .keys()
            .filter_map(|k| k.as_str())
            .collect();
        assert_eq!(keys, vec!["name", "intent", "execution", "safety"]);
    }

    #[test]
    fn a_key_the_order_does_not_know_about_survives_at_the_end() {
        let doc = yaml("goals: []\nname: t\nsomething_new: 1\n");
        let (out, _) = migrate(&doc);
        let m = out.as_mapping().unwrap();
        assert_eq!(m.get("something_new"), Some(&Value::from(1)));
        assert_eq!(
            m.keys().filter_map(|k| k.as_str()).last(),
            Some("something_new")
        );
    }

    #[test]
    fn a_document_that_is_not_a_mapping_is_returned_untouched() {
        let doc = yaml("- just\n- a list\n");
        let (out, moved) = migrate(&doc);
        assert_eq!(out, doc);
        assert!(moved.is_empty());
    }
}
