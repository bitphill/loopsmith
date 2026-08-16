//! `loopsmith mcp` — serve the local MCP server on stdio.

use std::path::PathBuf;
use std::process::ExitCode;

pub fn execute(state: PathBuf) -> Result<ExitCode, String> {
    let store = loopsmith_memory::open(state).map_err(|e| e.to_string())?;
    let server = loopsmith_mcp::Server::new(store);
    let stdin = std::io::stdin();
    server
        .serve(stdin.lock(), std::io::stdout())
        .map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}
