//! Git worktree isolation for parallel builders.
//!
//! Two builders in one checkout overwrite each other's files. The corpus fix
//! is a worktree per crew rather than per agent, plus a frozen rule set that
//! forbids the destructive git commands. This module supplies the first half;
//! the rules live in the config's constraint block.
//!
//! Nothing here is fatal: a loop running outside a git repository, or on a
//! machine without git, falls back to the shared working directory and says
//! so. Refusing to run would be worse than running unisolated and reporting
//! it, as long as the report is honest.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Isolation {
    /// Node runs in its own worktree at this path.
    Worktree { path: PathBuf, branch: String },
    /// Node runs in the shared directory, with the reason isolation was
    /// skipped.
    Shared { reason: String },
}

impl Isolation {
    pub fn workdir<'a>(&'a self, fallback: &'a Path) -> &'a Path {
        match self {
            Isolation::Worktree { path, .. } => path,
            Isolation::Shared { .. } => fallback,
        }
    }
    pub fn describe(&self) -> String {
        match self {
            Isolation::Worktree { path, branch } => {
                format!("isolated in {} on {branch}", path.display())
            }
            Isolation::Shared { reason } => format!("shared workdir ({reason})"),
        }
    }
}

fn git(args: &[&str], cwd: &Path) -> std::io::Result<std::process::Output> {
    Command::new("git").args(args).current_dir(cwd).output()
}

pub fn is_git_repo(root: &Path) -> bool {
    git(&["rev-parse", "--is-inside-work-tree"], root)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Branch and directory names derived from a node id. Node ids are already
/// validated as config identifiers, but git is stricter, so anything odd is
/// replaced rather than passed through.
fn sanitize(node_id: &str) -> String {
    node_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

/// Create a worktree for a node, or explain why it could not be done.
pub fn create(root: &Path, node_id: &str, run_id: &str) -> Isolation {
    if which_git().is_none() {
        return Isolation::Shared {
            reason: "git not on PATH".into(),
        };
    }
    if !is_git_repo(root) {
        return Isolation::Shared {
            reason: "not a git repository".into(),
        };
    }
    let safe = sanitize(node_id);
    let branch = format!("loopsmith/{}/{safe}", sanitize(run_id));
    let path = root.join("state").join("worktrees").join(&safe);

    if path.exists() {
        // Reuse across iterations: recreating would throw away the node's
        // in-progress work every pass.
        return Isolation::Worktree { path, branch };
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let out = git(
        &["worktree", "add", "-B", &branch, &path.to_string_lossy()],
        root,
    );
    match out {
        Ok(o) if o.status.success() => Isolation::Worktree { path, branch },
        Ok(o) => Isolation::Shared {
            reason: String::from_utf8_lossy(&o.stderr)
                .lines()
                .last()
                .unwrap_or("git worktree add failed")
                .to_string(),
        },
        Err(e) => Isolation::Shared {
            reason: e.to_string(),
        },
    }
}

/// Remove a worktree. Best effort — a leftover worktree is untidy, not unsafe.
pub fn remove(root: &Path, iso: &Isolation) {
    if let Isolation::Worktree { path, .. } = iso {
        let _ = git(
            &["worktree", "remove", "--force", &path.to_string_lossy()],
            root,
        );
    }
}

fn which_git() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|d| {
        let c = d.join("git");
        c.is_file().then_some(c)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn tmp(tag: &str) -> PathBuf {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("loopsmith-wt-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn init_repo(p: &Path) {
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@t.t"],
            vec!["config", "user.name", "t"],
        ] {
            git(&args, p).unwrap();
        }
        std::fs::write(p.join("f.txt"), "hello").unwrap();
        git(&["add", "-A"], p).unwrap();
        git(&["commit", "-qm", "init"], p).unwrap();
    }

    #[test]
    fn outside_a_repo_it_degrades_to_shared_rather_than_failing() {
        let root = tmp("norepo");
        let iso = create(&root, "build", "r1");
        assert!(matches!(iso, Isolation::Shared { .. }));
        assert!(iso.describe().contains("not a git repository"));
        // The fallback must still give the caller a usable directory.
        assert_eq!(iso.workdir(&root), root.as_path());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn inside_a_repo_it_creates_a_real_worktree() {
        let root = tmp("repo");
        init_repo(&root);
        let iso = create(&root, "build", "r1");
        match &iso {
            Isolation::Worktree { path, branch } => {
                assert!(path.join("f.txt").is_file(), "worktree has repo contents");
                assert!(branch.starts_with("loopsmith/r1/"));
            }
            Isolation::Shared { reason } => panic!("expected a worktree, got shared: {reason}"),
        }
        remove(&root, &iso);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn two_nodes_get_separate_directories() {
        let root = tmp("two");
        init_repo(&root);
        let a = create(&root, "refactor-a", "r1");
        let b = create(&root, "refactor-b", "r1");
        assert_ne!(a.workdir(&root), b.workdir(&root));
        // Writes in one must not appear in the other — the whole point.
        std::fs::write(a.workdir(&root).join("only-a.txt"), "x").unwrap();
        assert!(!b.workdir(&root).join("only-a.txt").exists());
        remove(&root, &a);
        remove(&root, &b);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn recreating_reuses_the_existing_worktree_instead_of_wiping_it() {
        let root = tmp("reuse");
        init_repo(&root);
        let first = create(&root, "build", "r1");
        let marker = first.workdir(&root).join("in-progress.txt");
        std::fs::write(&marker, "work").unwrap();

        let second = create(&root, "build", "r1");
        assert_eq!(first.workdir(&root), second.workdir(&root));
        assert!(marker.exists(), "in-progress work must survive the next iteration");

        remove(&root, &first);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn odd_node_ids_do_not_escape_the_worktree_directory() {
        assert_eq!(sanitize("a/../b"), "a----b");
        assert_eq!(sanitize("refactor-a"), "refactor-a");
        assert_eq!(sanitize("x y"), "x-y");
    }
}
