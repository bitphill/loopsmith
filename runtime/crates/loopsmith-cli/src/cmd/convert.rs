//! `loopsmith convert` — translate a config between YAML and Markdown.
//!
//! The two are the same model in two grammars, so conversion is load-then-emit.
//! Direction is inferred from the input: a `.md` config converts to YAML, and
//! anything else converts to Markdown.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn execute(config: &Path, out: Option<PathBuf>, to_yaml: bool) -> Result<ExitCode, String> {
    let cfg = loopsmith_core::load(config).map_err(|e| e.to_string())?;
    let want_yaml = to_yaml || loopsmith_core::is_markdown(config);

    let text = if want_yaml {
        serde_yaml::to_string(&cfg).map_err(|e| e.to_string())?
    } else {
        loopsmith_core::render_md(&cfg)
    };

    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
            std::fs::write(&path, &text).map_err(|e| e.to_string())?;
            println!("wrote {}", path.display());
        }
        None => print!("{text}"),
    }
    Ok(ExitCode::SUCCESS)
}
