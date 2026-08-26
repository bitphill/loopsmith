//! `loopsmith web` / `loopsmith --web`.

use std::process::ExitCode;

pub fn execute(port: Option<u16>, no_open: bool) -> Result<ExitCode, String> {
    // `serve` only returns when the server stops, which on a Ctrl-C is a
    // normal end rather than a failure.
    crate::web::serve(port, no_open)?;
    Ok(ExitCode::SUCCESS)
}
