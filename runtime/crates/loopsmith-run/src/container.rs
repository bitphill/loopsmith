//! Container isolation, and what happens on a machine without Docker.
//!
//! A node marked `isolation: container` gets a git worktree like any isolated
//! node, and its provider command runs inside `docker run --rm` with that
//! worktree mounted at `/work` and, unless the node asks for one, no network.
//! The worktree keeps its files apart from its neighbours'; the container
//! keeps its *process* apart from the machine.
//!
//! A machine without a container runtime is common — most laptops, many CI
//! images, every locked-down corporate desktop — and a loop that refused to
//! run there would be a loop that only runs on its author's machine. So a
//! container node **degrades to a worktree** when the runtime is missing, the
//! daemon is not running, or no image is named, and the ledger says which.
//! It never fails the run.
//!
//! The runtime is `docker` unless `LOOPSMITH_DOCKER` names another binary with
//! the same CLI — `podman` is the usual one.

use loopsmith_core::Isolation;
use loopsmith_provider::Container;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// A container runtime that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runtime {
    pub bin: PathBuf,
    pub version: String,
}

/// Where a node's provider command will actually run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Containment {
    /// On the host, in whatever directory its isolation gave it.
    Host,
    /// Inside this container.
    Container(Container),
    /// It asked for a container and is not getting one, for this reason.
    Degraded(String),
}

/// The runtime binary name: `LOOPSMITH_DOCKER`, or `docker`.
fn runtime_name() -> String {
    std::env::var("LOOPSMITH_DOCKER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "docker".into())
}

/// Find the container runtime and check its daemon answers.
///
/// Probed once per process: the answer does not change between nodes, and
/// asking the daemon for its version on every dispatch would put a subprocess
/// and up to five seconds in front of each one.
pub fn probe() -> Result<Runtime, String> {
    static PROBED: OnceLock<Result<Runtime, String>> = OnceLock::new();
    PROBED.get_or_init(|| probe_uncached(&runtime_name())).clone()
}

fn probe_uncached(name: &str) -> Result<Runtime, String> {
    let bin = loopsmith_util::which(name).ok_or_else(|| format!("`{name}` is not on PATH"))?;
    let mut child = Command::new(&bin)
        .args(["version", "--format", "{{.Server.Version}}"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("`{name}` could not be started: {e}"))?;

    // A daemon that is installed but stopped can hang the client for a long
    // time on some platforms. Five seconds is plenty for one that is running.
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("`{name}` did not answer within 5s; is its daemon running?"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("`{name}` could not be waited on: {e}")),
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("`{name}` failed: {e}"))?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || version.is_empty() {
        return Err(format!("`{name}` is installed but its daemon is not running"));
    }
    Ok(Runtime { bin, version })
}

/// Decide where a node runs, given what it asked for, the graph's default
/// image, and what the runtime probe found.
pub fn resolve(
    iso: &Isolation,
    graph_image: Option<&str>,
    runtime: Result<&Runtime, &str>,
) -> Containment {
    let Isolation::Container { network, .. } = iso else {
        return Containment::Host;
    };
    let Some(image) = iso.container_image(graph_image) else {
        return Containment::Degraded(
            "no image is named on the node or in `execution.graph.container_image`".into(),
        );
    };
    match runtime {
        Ok(rt) => Containment::Container(Container {
            image: image.to_string(),
            network: *network,
            runtime: rt.bin.clone(),
        }),
        Err(why) => Containment::Degraded(why.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rt() -> Runtime {
        Runtime {
            bin: "/usr/bin/docker".into(),
            version: "27.0.0".into(),
        }
    }

    fn container(image: Option<&str>) -> Isolation {
        Isolation::Container {
            image: image.map(str::to_string),
            network: false,
        }
    }

    #[test]
    fn a_node_that_did_not_ask_for_a_container_runs_on_the_host() {
        assert_eq!(resolve(&Isolation::Worktree {}, Some("img"), Ok(&rt())), Containment::Host);
    }

    #[test]
    fn without_a_runtime_a_container_node_degrades_rather_than_fails() {
        let got = resolve(&container(Some("img")), None, Err("`docker` is not on PATH"));
        assert_eq!(got, Containment::Degraded("`docker` is not on PATH".into()));
    }

    #[test]
    fn without_an_image_there_is_nothing_to_run_in() {
        let got = resolve(&container(None), None, Ok(&rt()));
        assert!(matches!(got, Containment::Degraded(ref why) if why.contains("no image")));
    }

    #[test]
    fn the_graph_image_is_the_fallback_and_the_node_image_wins() {
        let Containment::Container(c) = resolve(&container(None), Some("graph-img"), Ok(&rt()))
        else {
            panic!("expected a container");
        };
        assert_eq!(c.image, "graph-img");
        let Containment::Container(c) =
            resolve(&container(Some("node-img")), Some("graph-img"), Ok(&rt()))
        else {
            panic!("expected a container");
        };
        assert_eq!(c.image, "node-img");
        assert!(!c.network, "no network unless asked for");
    }

    #[test]
    fn a_runtime_that_is_not_installed_is_reported_by_name() {
        let err = probe_uncached("loopsmith-no-such-runtime-xyzzy").unwrap_err();
        assert!(err.contains("not on PATH"), "{err}");
    }
}
