//! `loopsmith web` — the browser UI.
//!
//! Everything this binary does from a terminal, done by clicking. It exists
//! for the person who has a job to automate and no interest in learning a YAML
//! schema first: the machine is probed for agent CLIs, the form is the A–J
//! model with every field explained in place, and the buttons run the same
//! commands the CLI runs.
//!
//! Three properties hold, and each is load-bearing:
//!
//! - **Localhost only.** The listener binds `127.0.0.1`. Nothing about this is
//!   safe to expose, and binding it to an interface would hand a network the
//!   ability to run commands as this user.
//! - **No new behaviour.** Every action shells out to this same binary. The
//!   browser cannot do anything `loopsmith --help` does not list.
//! - **Self-contained.** The frontend is compiled in, so a machine with no
//!   checkout and no Node has a working UI.

pub mod api;
pub mod assemble;
pub mod assets;
pub mod catalog;
pub mod detect;
pub mod examples;
pub mod exec;
pub mod help;
pub mod picker;
pub mod secrets;
pub mod ws;

use std::net::{Ipv4Addr, SocketAddr, TcpListener};

/// Where to start looking for a free port.
const DEFAULT_PORT: u16 = 3000;

/// How far to walk up before giving up. Twenty ports is more than any machine
/// has genuinely occupied, and stopping is better than scanning to 65535.
const PORT_ATTEMPTS: u16 = 20;

/// Bind the first free port at or above `start`.
///
/// A busy port is the normal case on a developer's machine — 3000 is the most
/// contested port there is — so failing on it would make the common case an
/// error. The chosen port is printed and opened, so stepping up is invisible
/// rather than confusing.
fn bind(start: u16) -> Result<(TcpListener, SocketAddr), String> {
    let mut last = String::new();
    for offset in 0..PORT_ATTEMPTS {
        let port = start.saturating_add(offset);
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        match TcpListener::bind(addr) {
            Ok(l) => {
                // The listener is handed to tokio, which needs it non-blocking.
                l.set_nonblocking(true)
                    .map_err(|e| format!("could not prepare the listener: {e}"))?;
                // `local_addr`, not `addr`: port 0 means "any free port", and
                // the caller needs the one the OS actually chose. Returning the
                // requested address would print `http://127.0.0.1:0` and open a
                // browser tab at a URL that goes nowhere.
                let bound = l
                    .local_addr()
                    .map_err(|e| format!("could not read the bound address: {e}"))?;
                return Ok((l, bound));
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!(
        "no free port between {start} and {} ({last}). Something is using all of them.",
        start + PORT_ATTEMPTS - 1
    ))
}

/// Open a URL in the default browser.
///
/// Hand-rolled rather than pulled from a crate: it is one command per platform,
/// and this workspace keeps its dependency tree short on purpose. Failure is
/// not an error — the URL is printed either way, and a headless machine or a
/// user with no default browser should get a link, not a stack trace.
fn open_browser(url: &str) -> bool {
    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        // `start` is a cmd builtin, and the empty string is the window title
        // slot — without it, a quoted URL is taken as the title and nothing
        // opens.
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    std::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok()
}

pub fn serve(port: Option<u16>, no_open: bool) -> Result<(), String> {
    let (listener, addr) = bind(port.unwrap_or(DEFAULT_PORT))?;
    let url = format!("http://{}:{}", addr.ip(), addr.port());

    let state = api::AppState {
        jobs: exec::Jobs::new(),
        version: env!("CARGO_PKG_VERSION"),
    };
    // The API is merged over the asset router rather than nested, so
    // `/api/…` wins and everything else falls through to the app shell.
    let app = api::router(state).merge(assets::router());

    println!("loopsmith web {}", env!("CARGO_PKG_VERSION"));
    println!("  {url}");
    if port.is_some_and(|p| p != addr.port()) || addr.port() != DEFAULT_PORT {
        println!("  (port {} was busy)", port.unwrap_or(DEFAULT_PORT));
    }
    println!("  bound to localhost only — nothing else on the network can reach it");
    println!("\nPress Ctrl-C to stop.");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("could not start the async runtime: {e}"))?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|e| format!("could not take over the listener: {e}"))?;

        if !no_open && !open_browser(&url) {
            println!("(could not open a browser — the URL above still works)");
        }

        axum::serve(listener, app)
            .await
            .map_err(|e| format!("the server stopped: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_steps_past_a_busy_port_instead_of_failing() {
        // Occupy one, then ask for it. The common case on a machine that has
        // anything else running.
        let (held, addr) = bind(0).expect("the OS always has one free port");
        let busy = addr.port();
        let (_next, next_addr) = bind(busy).expect("must step up rather than fail");
        assert_ne!(next_addr.port(), busy, "stepped past the busy port");
        assert!(next_addr.port() > busy, "steps up, never down");
        drop(held);
    }

    #[test]
    fn the_listener_is_localhost_and_nothing_else() {
        // The single most consequential line in this module. A UI that spawns
        // commands as this user must not be reachable from the network.
        let (_l, addr) = bind(0).unwrap();
        assert!(addr.ip().is_loopback(), "bound to {addr}, which is not loopback");
    }
}
