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
    yaml_blocks_of(text, "```yaml")
}

/// Every fenced block opening with `fence`, with the line it starts on.
fn yaml_blocks_of(text: &str, fence: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(usize, Vec<&str>)> = None;
    for (n, line) in text.lines().enumerate() {
        match &mut open {
            None => {
                if line.trim_start().starts_with(fence) {
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

/// Text as the repository holds it: line endings are a property of the
/// checkout, not of what was committed.
fn lf(text: String) -> String {
    text.replace("\r\n", "\n")
}

/// Every document that draws the architecture draws the same one.
///
/// `assets/architecture.mmd` is the source and `tools/render-diagrams.sh`
/// renders `architecture.txt` from it. Before 1.0 the picture was a PNG drawn
/// once and the text was typed by hand into three documents, which is four
/// copies of one diagram and no way to tell which was current.
///
/// This is the half a shell script cannot check: the script keeps the rendered
/// files in step with the source, and this keeps the documents in step with
/// the rendered files.
#[test]
fn the_architecture_diagram_is_the_same_one_everywhere() {
    let root = repo_root();
    // Both sides go through `lf`: git stores these files with LF, and a Windows
    // checkout with `core.autocrlf` hands them over as CRLF, which made every
    // copy look stale there while being byte-identical in the repository.
    let canonical = lf(std::fs::read_to_string(root.join("assets/architecture.txt"))
        .expect("assets/architecture.txt — run ./tools/render-diagrams.sh"));
    let canonical = canonical.trim_end();

    let mut drawn = 0;
    for doc in ["HOW-TO-USE.md", "wiki/Architecture.md"] {
        let text = lf(std::fs::read_to_string(root.join(doc)).unwrap_or_else(|e| panic!("{doc}: {e}")));
        let block = yaml_blocks_of(&text, "```text")
            .into_iter()
            .find(|(_, body)| body.starts_with("INVOCATION"))
            .unwrap_or_else(|| panic!("{doc} no longer draws the architecture"));
        assert_eq!(
            block.1.trim_end(),
            canonical,
            "{doc}:{} has a stale copy of the architecture diagram — \
             run ./tools/render-diagrams.sh and paste assets/architecture.txt",
            block.0
        );
        drawn += 1;
    }
    assert_eq!(drawn, 2);
}
