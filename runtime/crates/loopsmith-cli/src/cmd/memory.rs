//! `loopsmith memory` — what this loop remembers across runs.
//!
//! The engine writes failure modes and procedures; an agent writes facts
//! through the MCP server. This is the human's view of all of it, and the
//! human half of promotion: a namespace whose rule is `human_approval` only
//! ever promotes a record through `promote`.

use super::open_store;
use loopsmith_memory::{namespaces, Namespace, Store};
use std::path::Path;
use std::process::ExitCode;

fn namespace(name: &str) -> Result<Namespace, String> {
    match Namespace::parse(name) {
        Some(Namespace::Episodic) | None => Err(format!(
            "`{name}` is not a memory namespace; use semantic, procedural, or failure"
        )),
        Some(ns) => Ok(ns),
    }
}

pub fn list(config: &Path, only: Option<&str>) -> Result<ExitCode, String> {
    let store = open_store(config)?;
    let wanted = match only {
        Some(n) => vec![namespace(n)?],
        None => vec![Namespace::Semantic, Namespace::Procedural, Namespace::Failure],
    };
    let mut any = false;
    for ns in wanted {
        let records = store.records(ns).map_err(|e| e.to_string())?;
        if records.is_empty() {
            continue;
        }
        any = true;
        println!("{}", ns.as_str());
        for r in records {
            println!(
                "  {} {}  (confidence {:.2}, {} run(s))",
                if r.promoted { "promoted" } else { "recorded" },
                r.key,
                r.confidence,
                r.runs.len()
            );
            println!("      {}", r.content);
            if let Some(p) = &r.provenance {
                println!("      from: {p}");
            }
        }
    }
    if !any {
        println!("nothing remembered yet");
    }
    Ok(ExitCode::SUCCESS)
}

pub fn promote(config: &Path, ns: &str, key: &str) -> Result<ExitCode, String> {
    let store = open_store(config)?;
    if namespaces::promote(&store, namespace(ns)?, key).map_err(|e| e.to_string())? {
        println!("promoted {ns} record: {key}\nlater runs will reuse it");
        Ok(ExitCode::SUCCESS)
    } else {
        Err(format!("no {ns} record has the key {key}; `loopsmith memory list` shows them"))
    }
}

pub fn forget(config: &Path, ns: &str, key: &str) -> Result<ExitCode, String> {
    let store = open_store(config)?;
    let ns_ = namespace(ns)?;
    if store.record(ns_, key).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("no {ns} record has the key {key}"));
    }
    store.remove_record(ns_, key).map_err(|e| e.to_string())?;
    println!("forgot {ns} record: {key}");
    Ok(ExitCode::SUCCESS)
}
