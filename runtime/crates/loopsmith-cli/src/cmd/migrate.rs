//! `loopsmith migrate` — rewrite a 0.3 config into the 1.0 shape.
//!
//! Nothing forces anyone to run this. A 0.3 config loads unchanged, because
//! the loader applies the same relocation on the way in. The command exists so
//! the *file* can say what the loader already understands, which is what makes
//! the deprecation warnings stop and the docs match what is on disk.
//!
//! Three modes, and the defaults are the cautious ones: `--check` reports
//! without touching anything and is what CI runs, the bare command prints the
//! migrated text to stdout, and `--write` is the only spelling that edits a
//! file in place.

use loopsmith_core::config::legacy;
use std::path::Path;
use std::process::ExitCode;

pub fn execute(config: &Path, check: bool, write: bool) -> Result<ExitCode, String> {
    let text = std::fs::read_to_string(config)
        .map_err(|e| format!("could not read {}: {e}", config.display()))?;
    let markdown = loopsmith_core::is_markdown(config);

    // Markdown is a different grammar, so migrating it means parsing it into
    // the model and rendering it back out. YAML is rewritten as a document, so
    // that a key the model does not know about — a comment's worth of future
    // config — survives rather than being silently dropped on the way through.
    let (migrated, moved) = if markdown {
        let (cfg, moved) = loopsmith_core::parse_md_reporting(&text, &config.display().to_string())
            .map_err(|e| e.to_string())?;
        (loopsmith_core::render_md(&cfg), moved)
    } else {
        let doc: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|e| format!("could not read {} as YAML: {e}", config.display()))?;
        let (doc, moved) = legacy::migrate(&doc);
        let rendered =
            serde_yaml::to_string(&doc).map_err(|e| format!("could not render YAML: {e}"))?;
        (rendered, moved)
    };

    if moved.is_empty() {
        println!("{} is already in the 1.0 shape.", config.display());
        return Ok(ExitCode::SUCCESS);
    }

    // Refusing to write something that does not parse is the whole safety
    // property here: a migration that produces an unloadable file has taken a
    // working config away from someone.
    let reparsed = if markdown {
        loopsmith_core::parse_md(&migrated, "migrated").map(|_| ())
    } else {
        loopsmith_core::parse_str(&migrated, "migrated").map(|_| ())
    };
    if let Err(e) = reparsed {
        return Err(format!(
            "the migration produced a file that does not load, so nothing was written:\n  {e}\n\
             This is a bug — please report it with the original config."
        ));
    }

    println!("{} uses {} key(s) from 0.3:", config.display(), moved.len());
    for m in &moved {
        println!("  {m}");
    }

    if check {
        println!("\nRun `loopsmith migrate {} --write` to rewrite it.", config.display());
        return Ok(ExitCode::FAILURE);
    }

    if !write {
        println!("\n--- migrated ---");
        print!("{migrated}");
        println!("\nRe-run with --write to replace the file.");
        return Ok(ExitCode::SUCCESS);
    }

    std::fs::write(config, &migrated)
        .map_err(|e| format!("could not write {}: {e}", config.display()))?;
    println!("\nRewrote {}.", config.display());
    Ok(ExitCode::SUCCESS)
}
