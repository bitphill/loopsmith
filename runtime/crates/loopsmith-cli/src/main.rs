//! `loopsmith` — the control plane for self-evolving agent loops.
//!
//! This file is the entry point and nothing more. The argument grammar lives in
//! [`cli`], and each subcommand body lives in its own module under [`cmd`].

mod cli;
mod cmd;
mod judgment;
mod logging;
mod permissions;
mod run;
mod scaffold;
mod schedule;
#[cfg(feature = "web")]
mod web;
mod worktree;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    // `resolve` collapses `--web` and the `web` subcommand into one value, so
    // everything downstream sees a single `Command` and neither spelling is
    // privileged. Its errors are the same shape as a command's, so they take
    // the same exit path.
    let result = cli::Cli::parse().resolve().and_then(cmd::dispatch);
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
