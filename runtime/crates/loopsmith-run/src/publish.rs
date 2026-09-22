//! Getting isolated work back where the gate can see it.
//!
//! An isolated node runs in `state/worktrees/<node>/`. Evidence is collected
//! from the loop root, so without this step a `file_exists` detector pointing
//! at something an isolated builder produced can never pass: the work is real,
//! on disk, and invisible to the only thing allowed to rule on it. That is a
//! worse failure than the clobbering isolation exists to prevent, because it
//! looks like the builder did nothing.
//!
//! So isolation is a property of the *wave*, not of the run. Builders write in
//! parallel without colliding; when the wave joins, what each one produced is
//! published into the loop root, one node at a time, in dispatch order.
//!
//! Two rules make that safe to reason about:
//!
//! 1. **Only what the node changed is published.** Git already knows: a
//!    worktree's `status` is exactly the set of paths that differ from the
//!    commit it branched from. Copying the whole tree would republish the
//!    repository over itself.
//! 2. **The first writer of a path wins, and the second is reported.** Two
//!    nodes in one wave writing the same file is the collision isolation was
//!    meant to prevent; taking the last write silently would reintroduce it
//!    with extra steps. The loser is named in the ledger with the path and the
//!    node that got there first.
//!
//! Not fixed here, and worth knowing: a worktree branches from `HEAD`, so an
//! isolated node does not see uncommitted work published by a node earlier in
//! the same run. Downstream *unisolated* nodes do.

use crate::worktree::Isolation;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// What publishing one node amounted to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Publication {
    /// Paths copied into the loop root, relative to it.
    pub published: Vec<String>,
    /// `(path, node that claimed it first)` for paths this node was refused.
    pub conflicts: Vec<(String, String)>,
    /// Paths git reported that could not be copied, with the reason.
    pub failed: Vec<(String, String)>,
}

impl Publication {
    pub fn is_empty(&self) -> bool {
        self.published.is_empty() && self.conflicts.is_empty() && self.failed.is_empty()
    }

    /// One ledger line, or `None` when the node changed nothing.
    pub fn describe(&self, node_id: &str) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut s = format!(
            "`{node_id}` published {} path(s) from its worktree: {}",
            self.published.len(),
            if self.published.is_empty() {
                "none".to_string()
            } else {
                self.published.join(", ")
            }
        );
        for (path, owner) in &self.conflicts {
            s.push_str(&format!(
                "; refused `{path}` — `{owner}` wrote it first in this iteration"
            ));
        }
        for (path, why) in &self.failed {
            s.push_str(&format!("; could not publish `{path}`: {why}"));
        }
        Some(s)
    }
}

/// Paths that belong to the loop's own bookkeeping and must never be published
/// out of a worktree, whatever a node did to them.
fn is_reserved(rel: &str) -> bool {
    rel.starts_with("state/") || rel.starts_with("logs/") || rel.starts_with(".git/")
}

/// Copy what an isolated node changed back into the loop root.
///
/// `claimed` maps an already-published path to the node that published it, and
/// is carried across every node in one iteration so a collision is detectable
/// rather than merely possible.
pub fn publish(
    root: &Path,
    node_id: &str,
    iso: &Isolation,
    claimed: &mut BTreeMap<String, String>,
) -> Publication {
    let Isolation::Worktree { path, .. } = iso else {
        return Publication::default();
    };
    let mut out = Publication::default();

    for rel in changed_paths(path) {
        if is_reserved(&rel) {
            continue;
        }
        if let Some(owner) = claimed.get(&rel) {
            if owner != node_id {
                out.conflicts.push((rel, owner.clone()));
                continue;
            }
        }
        let from = path.join(&rel);
        let to = root.join(&rel);
        match copy_into(&from, &to) {
            Ok(()) => {
                claimed.insert(rel.clone(), node_id.to_string());
                out.published.push(rel);
            }
            Err(e) => out.failed.push((rel, e)),
        }
    }
    out.published.sort();
    out
}

/// Seed a worktree with what other nodes have already published.
///
/// A worktree branches from `HEAD`, so it starts out blind to everything the
/// run has produced since — including the output of the node it depends on.
/// Publishing solved that for the gate, which reads the loop root; it did not
/// solve it for the next isolated node, which reads its own tree and would find
/// its upstream's work missing.
///
/// A node is never handed a path it published itself. That is the one thing
/// isolation must keep: a builder's in-progress work is not overwritten by the
/// copy of it that reached the root last iteration.
///
/// Returns the paths seeded, for the ledger.
pub fn seed(
    root: &Path,
    node_id: &str,
    iso: &Isolation,
    published: &BTreeMap<String, String>,
) -> Vec<String> {
    let Isolation::Worktree { path, .. } = iso else {
        return vec![];
    };
    let mut seeded = Vec::new();
    for (rel, owner) in published {
        if owner == node_id || is_reserved(rel) {
            continue;
        }
        if copy_into(&root.join(rel), &path.join(rel)).is_ok() {
            seeded.push(rel.clone());
        }
    }
    seeded
}

fn copy_into(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_file() {
        // A directory or a path git listed and the node then removed. Neither
        // is an error worth stopping for, but neither is it a publication.
        return Err("not a regular file".into());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(from, to).map_err(|e| e.to_string())?;
    Ok(())
}

/// Every path in the worktree that differs from the commit it branched from.
///
/// `-z` because paths may contain spaces, and `--untracked-files=all` because
/// a brand-new artifact in a brand-new directory is the normal case — git's
/// default collapses it to the directory name.
fn changed_paths(worktree: &Path) -> Vec<String> {
    let Ok(out) = Command::new("git")
        .args(["status", "--porcelain", "-z", "--untracked-files=all"])
        .current_dir(worktree)
        .output()
    else {
        return vec![];
    };
    if !out.status.success() {
        return vec![];
    }
    parse_status(&String::from_utf8_lossy(&out.stdout))
}

/// Parse `git status --porcelain -z` into the paths a node produced.
///
/// Each record is `XY<space><path>`, NUL-terminated. A rename adds a second
/// NUL-terminated field for the source path, which is dropped: the destination
/// is what the node produced.
fn parse_status(raw: &str) -> Vec<String> {
    let mut fields = raw.split('\0').filter(|f| !f.is_empty());
    let mut out = Vec::new();
    while let Some(record) = fields.next() {
        if record.len() < 4 {
            continue;
        }
        let (status, path) = record.split_at(3);
        let status = status.as_bytes();
        // A rename or copy carries its origin in the next field.
        if status[0] == b'R' || status[0] == b'C' {
            let _ = fields.next();
        }
        // A deletion is not something to publish; copying would fail anyway,
        // and propagating a removal across the isolation boundary is a
        // different decision from propagating a write.
        if status[0] == b'D' || status[1] == b'D' {
            continue;
        }
        out.push(path.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_util::testing::temp_dir;
    use std::path::PathBuf;

    fn git(args: &[&str], cwd: &Path) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    fn repo(tag: &str) -> PathBuf {
        let d = temp_dir(tag);
        git(&["init", "-q"], &d);
        git(&["config", "user.email", "t@t.t"], &d);
        git(&["config", "user.name", "t"], &d);
        std::fs::write(d.join("seed.txt"), "seed").unwrap();
        git(&["add", "-A"], &d);
        git(&["commit", "-qm", "seed"], &d);
        d
    }

    fn worktree(root: &Path, node: &str) -> Isolation {
        crate::worktree::create(root, node, "r1")
    }

    #[test]
    fn a_new_file_in_a_worktree_reaches_the_loop_root() {
        let root = repo("pub-new");
        let iso = worktree(&root, "build");
        let wt = iso.workdir(&root).to_path_buf();
        std::fs::create_dir_all(wt.join("out")).unwrap();
        std::fs::write(wt.join("out/thing.txt"), "produced").unwrap();

        let mut claimed = BTreeMap::new();
        let p = publish(&root, "build", &iso, &mut claimed);

        assert_eq!(p.published, vec!["out/thing.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(root.join("out/thing.txt")).unwrap(),
            "produced"
        );
        crate::worktree::remove(&root, &iso);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_modified_tracked_file_is_published_too() {
        let root = repo("pub-mod");
        let iso = worktree(&root, "build");
        let wt = iso.workdir(&root).to_path_buf();
        std::fs::write(wt.join("seed.txt"), "changed").unwrap();

        let mut claimed = BTreeMap::new();
        let p = publish(&root, "build", &iso, &mut claimed);

        assert_eq!(p.published, vec!["seed.txt".to_string()]);
        assert_eq!(
            std::fs::read_to_string(root.join("seed.txt")).unwrap(),
            "changed"
        );
        crate::worktree::remove(&root, &iso);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_second_node_to_write_a_path_is_refused_and_named() {
        // This is the collision isolation exists to prevent. Taking the last
        // write silently would reintroduce it after doing the work to avoid it.
        let root = repo("pub-clash");
        let a = worktree(&root, "a");
        let b = worktree(&root, "b");
        std::fs::write(a.workdir(&root).join("shared.txt"), "from a").unwrap();
        std::fs::write(b.workdir(&root).join("shared.txt"), "from b").unwrap();

        let mut claimed = BTreeMap::new();
        let pa = publish(&root, "a", &a, &mut claimed);
        let pb = publish(&root, "b", &b, &mut claimed);

        assert_eq!(pa.published, vec!["shared.txt".to_string()]);
        assert!(pb.published.is_empty(), "the second writer must be refused");
        assert_eq!(pb.conflicts, vec![("shared.txt".into(), "a".into())]);
        assert_eq!(
            std::fs::read_to_string(root.join("shared.txt")).unwrap(),
            "from a",
            "first writer wins"
        );
        assert!(pb.describe("b").unwrap().contains("`a` wrote it first"));

        crate::worktree::remove(&root, &a);
        crate::worktree::remove(&root, &b);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_loops_own_bookkeeping_is_never_published_out_of_a_worktree() {
        let root = repo("pub-reserved");
        let iso = worktree(&root, "build");
        let wt = iso.workdir(&root).to_path_buf();
        for rel in ["state/ledger.db", "logs/run.log"] {
            let p = wt.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "not yours").unwrap();
        }
        std::fs::write(wt.join("real.txt"), "mine").unwrap();

        let mut claimed = BTreeMap::new();
        let p = publish(&root, "build", &iso, &mut claimed);

        assert_eq!(p.published, vec!["real.txt".to_string()]);
        assert!(!root.join("state/ledger.db").exists());
        assert!(!root.join("logs/run.log").exists());
        crate::worktree::remove(&root, &iso);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_node_that_was_never_isolated_publishes_nothing() {
        let root = temp_dir("pub-shared");
        let mut claimed = BTreeMap::new();
        let p = publish(
            &root,
            "build",
            &Isolation::Shared {
                reason: "not marked isolated".into(),
            },
            &mut claimed,
        );
        assert!(p.is_empty());
        assert!(p.describe("build").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_deletion_is_not_propagated_across_the_isolation_boundary() {
        let root = repo("pub-del");
        let iso = worktree(&root, "build");
        std::fs::remove_file(iso.workdir(&root).join("seed.txt")).unwrap();

        let mut claimed = BTreeMap::new();
        let p = publish(&root, "build", &iso, &mut claimed);

        assert!(p.published.is_empty());
        assert!(
            root.join("seed.txt").is_file(),
            "removing a file in a worktree must not remove it from the loop root"
        );
        crate::worktree::remove(&root, &iso);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_status_parser_handles_renames_and_spaces() {
        // `R  new -> old` in -z form is two fields: the record, then the origin.
        let raw = "R  a b.txt\0old name.txt\0?? out/new.txt\0 M seed.txt\0 D gone.txt\0";
        assert_eq!(
            parse_status(raw),
            vec![
                "a b.txt".to_string(),
                "out/new.txt".to_string(),
                "seed.txt".to_string()
            ]
        );
    }
}
