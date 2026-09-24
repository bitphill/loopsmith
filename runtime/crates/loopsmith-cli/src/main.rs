//! `loopsmith` — the control plane for self-evolving agent loops.
//!
//! This file is the entry point and nothing more. The argument grammar lives in
//! [`cli`], and each subcommand body lives in its own module under [`cmd`]. The
//! engine, the wizard, and the browser UI are their own crates —
//! `loopsmith-run`, `loopsmith-wizard`, and `loopsmith-web` — so this one is
//! only the front door.

mod cli;
mod cmd;
mod guided;
mod scaffold;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    // The 0.3 verbs are rewritten into their 1.0 nouns before clap sees them,
    // so the grammar below holds only the 1.0 spellings and the compatibility
    // layer is one table in `cli::alias`.
    let (args, moved) = cli::alias::rewrite(std::env::args());
    if let Some(moved) = &moved {
        eprintln!("{}", moved.notice());
    }

    // `resolve` collapses `--web` and the `web` subcommand into one value, so
    // everything downstream sees a single `Command` and neither spelling is
    // privileged. Its errors are the same shape as a command's, so they take
    // the same exit path.
    let result = cli::Cli::parse_from(args).resolve().and_then(cmd::dispatch);
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
