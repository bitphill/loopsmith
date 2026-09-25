//! The YAML printed in the documents is YAML.
//!
//! `LOOP-TEMPLATE.md` is an authoring surface: its fenced blocks are meant to
//! be copied into a `loop.yaml`, and a block whose indentation is one space out
//! reads perfectly well and does not parse. Nothing else in this repository
//! looks at those blocks, so the first person to find out would be someone
//! copying one.
//!
//! This checks the grammar, not the model. A fragment of a config is not a
//! config — it has no `name` and no goals — so asking the loader to accept it
//! would mean either rewriting the documents into whole configs or exempting
//! most of them, and neither says anything a reader cares about. What a reader
//! cares about is that the block they paste is well formed.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate is three deep in the repository")
        .to_path_buf()
}

/// Every ```yaml block in a document, with the line it starts on.
fn yaml_blocks(text: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(usize, Vec<&str>)> = None;
    for (n, line) in text.lines().enumerate() {
        match &mut open {
            None => {
                if line.trim_start().starts_with("```yaml") {
                    open = Some((n + 2, Vec::new()));
                }
            }
            Some((_, body)) => {
                if line.trim_start().starts_with("```") {
                    let (start, body) = open.take().expect("just matched");
                    blocks.push((start, body.join("\n")));
                } else {
                    body.push(line);
                }
            }
        }
    }
    blocks
}

#[test]
fn every_yaml_block_a_reader_might_copy_is_parseable() {
    let root = repo_root();
    let mut checked = 0;
    for doc in ["LOOP-TEMPLATE.md", "HOW-TO-USE.md", "wiki/Architecture.md"] {
        let path = root.join(doc);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{doc}: {e}"));
        let blocks = yaml_blocks(&text);
        assert!(!blocks.is_empty(), "{doc} has no yaml blocks; the scan is not reading it");
        for (line, body) in blocks {
            serde_yaml::from_str::<serde_yaml::Value>(&body)
                .unwrap_or_else(|e| panic!("{doc}:{line} is not YAML: {e}\n{body}"));
            checked += 1;
        }
    }
    assert!(checked > 10, "only {checked} blocks checked");
}
