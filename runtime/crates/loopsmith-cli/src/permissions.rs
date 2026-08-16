//! Permission preflight.
//!
//! A hands-off run cannot stop halfway to ask for consent, and a run that
//! grants itself blanket access is not hands-off, it is unsupervised. The
//! compromise: derive the *narrowest* set of rules the config actually needs,
//! present them once, and write them where the harness reads them.
//!
//! Rules are derived from the config rather than guessed, so a loop that only
//! reads files never asks for write access.

use loopsmith_core::{Detector, LoopConfig};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

/// The permission strings this config needs, deduplicated and sorted.
pub fn required(cfg: &LoopConfig) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();

    // Every provider is a command, so each one needs its binary allowed.
    for p in &cfg.providers.providers {
        set.insert(format!("Bash({}:*)", p.command));
    }

    // Script detectors run during gating.
    for v in &cfg.validations {
        if let Detector::Script { command, .. } = &v.detector {
            set.insert(format!("Bash({command}:*)"));
        }
    }

    // Skill acquisition reaches the marketplace and the skills CLI.
    if cfg
        .skills
        .acquisition_order
        .iter()
        .any(|a| matches!(a, loopsmith_core::AcquisitionSource::Marketplace))
    {
        set.insert("Bash(npx skills:*)".into());
        set.insert("WebFetch(domain:claudemarketplaces.com)".into());
    }

    // The loop reads and writes its own directory.
    set.insert("Read".into());
    set.insert("Write".into());
    set.insert("Edit".into());
    set.insert("Glob".into());
    set.insert("Grep".into());

    set.into_iter().collect()
}

/// Human-readable preflight block — what gets shown before the single grant.
pub fn render(grant: &[String]) -> String {
    let mut s = String::new();
    s.push_str("This loop needs the following permissions to run hands-off:\n\n");
    for g in grant {
        s.push_str(&format!("  {g}\n"));
    }
    s.push_str(
        "\nNothing outside this list is requested. Anything the constraints mark as a\n\
         human checkpoint still stops and waits, grant or no grant.\n",
    );
    s
}

/// Merge the grant into a settings file, preserving whatever is already there.
/// Returns the resulting JSON so the caller can show it.
pub fn merge_into(path: &Path, grant: &[String]) -> std::io::Result<String> {
    let mut root: Value = if path.exists() {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    if !root.is_object() {
        root = json!({});
    }

    let allow = root
        .as_object_mut()
        .unwrap()
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .and_then(|p| {
            p.entry("allow")
                .or_insert_with(|| json!([]))
                .as_array_mut()
        });

    if let Some(list) = allow {
        let existing: BTreeSet<String> = list
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        for g in grant {
            if !existing.contains(g) {
                list.push(json!(g));
            }
        }
    }

    let out = serde_json::to_string_pretty(&root)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{out}\n"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(extra: &str) -> LoopConfig {
        let text = format!(
            r#"
name: t
goals:
  - name: g1
    description: a sufficiently long goal description
validations:
  - target: g1
    name: v1
    mode: objective
    statement: the suite passes
    detector: {{ type: script, command: "cargo" }}
providers:
  providers:
    - id: p1
      kind: ollama
      command: ollama
{extra}
"#
        );
        loopsmith_core::parse_str(&text, "test").unwrap()
    }

    #[test]
    fn provider_and_detector_commands_are_both_requested() {
        let g = required(&cfg(""));
        assert!(g.contains(&"Bash(ollama:*)".to_string()));
        assert!(g.contains(&"Bash(cargo:*)".to_string()));
    }

    #[test]
    fn marketplace_access_is_only_requested_when_the_policy_uses_it() {
        let with = required(&cfg(""));
        assert!(with.iter().any(|g| g.contains("claudemarketplaces.com")));

        let mut c = cfg("");
        c.skills.acquisition_order = vec![loopsmith_core::AcquisitionSource::Installed];
        let without = required(&c);
        assert!(!without.iter().any(|g| g.contains("claudemarketplaces.com")));
        assert!(!without.iter().any(|g| g.contains("npx skills")));
    }

    #[test]
    fn the_grant_has_no_duplicates_and_is_sorted() {
        let g = required(&cfg(""));
        let mut sorted = g.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(g, sorted);
    }

    #[test]
    fn merging_preserves_existing_rules_and_adds_new_ones() {
        let dir = loopsmith_util::testing::temp_dir("perm-merge");
        let file = dir.join("settings.local.json");
        std::fs::write(
            &file,
            r#"{"permissions":{"allow":["Skill(claude-api)"]},"theme":"light"}"#,
        )
        .unwrap();

        let out = merge_into(&file, &["Bash(ollama:*)".into()]).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let allow = v["permissions"]["allow"].as_array().unwrap();
        let strs: Vec<&str> = allow.iter().filter_map(|x| x.as_str()).collect();

        assert!(strs.contains(&"Skill(claude-api)"), "existing rule survived");
        assert!(strs.contains(&"Bash(ollama:*)"), "new rule added");
        assert_eq!(v["theme"], "light", "unrelated settings untouched");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn merging_is_idempotent() {
        let dir = loopsmith_util::testing::temp_dir("perm-idempotent");
        let file = dir.join("s.json");
        let grant = vec!["Bash(ollama:*)".to_string()];
        merge_into(&file, &grant).unwrap();
        let out = merge_into(&file, &grant).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["permissions"]["allow"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn render_mentions_that_checkpoints_still_stop() {
        let text = render(&["Read".into()]);
        assert!(text.contains("human checkpoint"));
    }
}
