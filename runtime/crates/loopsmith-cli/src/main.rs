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
mod worktree;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    match cmd::dispatch(cli::Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
