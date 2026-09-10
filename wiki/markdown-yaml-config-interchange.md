# Markdown/YAML Config Interchange

# Markdown/YAML Config Interchange

`loopsmith-core::md` lets a loop config be written as a Markdown document instead of YAML, and converts back the other way. Both grammars describe the same `LoopConfig` — the A–J section model — so nothing about the config model lives in this module beyond one small lookup table.

The point of the Markdown form is co-located documentation: prose at the left margin is ignored by the parser, so the reason a goal exists can sit directly next to the goal.

## Public surface

| Item | Location | Signature |
|---|---|---|
| `parse_md` | `md/parse.rs` | `fn(text: &str, origin: &str) -> Result<LoopConfig, CoreError>` |
| `render_md` | `md/render.rs` | `fn(cfg: &LoopConfig) -> String` |

Both are re-exported from `md/mod.rs` and again from the crate root, so callers use `loopsmith_core::parse_md` / `loopsmith_core::render_md`.

Everything else in the module — `SectionShape`, `section_shape`, `heading_to_key` — is `pub(crate)` and shared between the parser and the renderer so the two cannot drift.

## The central design decision

The parser does not know that `Goal` or `StopGates` exist. It converts the document into a `serde_yaml::Value` tree and hands that to the same `Deserialize` impls the YAML path uses:

```rust
let value = build_document(&toks)?;
serde_yaml::from_value::<LoopConfig>(value)
```

The renderer is the mirror image, working from `serde_yaml::to_value(cfg)`.

The consequence is the reason to keep it this way: every `#[serde(default)]`, every alias, and every `deny_unknown_fields` rule applies identically on both paths, for free. A new field on `LoopConfig` needs **no** change in this module. A new *section* needs, at most, one line in `section_shape` and one in `SECTIONS`.

```mermaid
flowchart LR
    MD[".md document"] -->|tokenize| TOK["Vec&lt;Tok&gt;"]
    TOK -->|build_document| VAL["serde_yaml::Value"]
    VAL -->|from_value| CFG["LoopConfig"]
    CFG -->|to_value| VAL
    VAL -->|render_md| MD
    YAML[".yaml document"] -->|serde_yaml| CFG
    CFG -->|serde_yaml| YAML
```

## Document grammar

```markdown
# my-loop

- version: 0.1.0
- description: what this loop is for

Prose at the left margin is ignored. Put the reasoning here.

## C. Goals

### ship-it
- description: the thing is shipped and the suite is green
- priority: 1

## F. Stop gates
- max_iterations: 12
- max_cost_usd: 10.0
```

Four structural rules cover the whole format:

- **`#`** sets the config's `name`.
- **`##`** opens a section. The heading is normalised by `heading_to_key`.
- **`###`** opens an entry inside the current section. Its text fills one field, decided by `section_shape`.
- **`- key: value`** bullets are fields — of the open `###` entry if there is one, otherwise of the open `##` section, otherwise of the document root.

Anything else is documentation and is dropped.

### `heading_to_key`

`A. Information` → `information`, `F. Stop gates` → `stop_gates`, `I. Execution guidelines` → `execution_guidelines`. Section letters are navigation aids for humans, not grammar, so a leading `A.` / `B)` / `J -` is stripped — but only when the delimiter is at index ≤ 2 and the prefix is alphanumeric. That window is what keeps `Pre-execution` (hyphen at index 3) intact while still stripping `B. Pre-execution`. Writing the raw key (`## stop_gates`) works too.

### `SectionShape`

The only section-specific knowledge in the module:

```rust
pub(crate) struct SectionShape {
    pub list_field: Option<&'static str>, // None when the section *is* the list
    pub key_field: &'static str,          // field a `###` heading fills
}
```

It exists because `### ship-it` must become `name: ship-it` for a goal but `id: ship-it` for a graph node. Two axes:

- **`key_field`** — `information` → `key`, `pre_execution` → `step`, `goals`/`validations`/`success`/`default_skills`/`execution_guidelines` → `name`, `schedules` → `type`, `graph`/`providers` → `id`.
- **`list_field`** — `None` for sections that *are* a sequence (`goals` deserialises straight into `Vec<Goal>`); `Some("nodes")` for `graph` and `Some("providers")` for `providers`, whose entries nest under a named field of a mapping, alongside sibling scalar fields.

Sections absent from the table (`stop_gates`, `constraints`, `skills`, `context`) take no `###` entries at all — they are plain field bags, and a `###` under them is a parse error rather than a silent drop.

## Parsing (`md/parse.rs`)

### `tokenize`

Single pass over lines producing `Tok::{H1, H2, H3, Bullet { indent, text }}`. Three behaviours worth knowing:

- **Fences** toggle on ```` ``` ```` at column 0 only, and their contents are skipped entirely. That is what makes the example block in this document, and in `LOOP-TEMPLATE.md`, safe to include inside a real config.
- **Headings are recognised at column 0 only.** An indented `###` is not a heading.
- **Continuation lines** fold into the bullet above them: a non-bullet line indented past the preceding bullet, with no blank line between, appends to that bullet's text with a `\n`. This is how a long `instruction` spans several lines without becoming prose. A blank line breaks the association (`after_blank`), which is the difference between a wrapped value and a paragraph of documentation.

### `build_document`

Walks the token stream holding two pieces of state: the open `section` and the open `entry`. Every heading calls `flush_entry` first, which appends the finished entry mapping to the right place — `root[section]` as a sequence, or `root[section][list_field]` when the shape names one — creating the container if absent.

A `###` heading's text is inserted as a `Value::from(String)`, never re-parsed as YAML. Without that, `### Recorded the baseline: test count, coverage` would deserialise as a one-key mapping and land on a field expecting text.

Bullets are handled in runs: the next non-bullet token bounds the block, and `build_block` turns the whole run into one value that is then merged into the entry, the section, or the root.

### `build_block`

Recursive over indentation. At each level it accumulates either a `Mapping` or a `Sequence` and refuses to mix them (`a bullet list mixes \`key: value\` entries with bare items`). A bullet whose value is empty (`- key:`) owns the more-deeply-indented bullets below it; with nothing below, it becomes `Value::Null`.

### `split_field` and `scalar`

These two functions carry most of the format's ergonomics.

`split_field` decides whether `- text` is a field or a bare list item. It requires `key: ` (colon-space) or a trailing bare `key:`, **and** that the key contain no whitespace. That guard is why `- Never git stash. Never git reset.` stays a list item instead of becoming a field named `Never git stash. Never git reset.`.

`scalar` runs single-line values through `serde_yaml::from_str`, so `12`, `true`, and `[a, b]` arrive as the types they look like, falling back to a string on failure. Multi-line values are kept verbatim as strings — prose containing a colon is not a mapping.

### Errors

Structural failures and serde failures both surface as `CoreError::Parse { path, yaml, json }`, with `json` set to `"not attempted: the file was read as markdown"` — the Markdown path never tries the JSON fallback the YAML path has.

## Rendering (`md/render.rs`)

`render_md` serialises the config to a `Value`, emits `# {name}`, then the `PREAMBLE` fields (`version`, `description`), then walks `SECTIONS` in order. That order matches `LOOP-TEMPLATE.md`, so a rendered config and the template read the same way. Absent or blank sections are skipped.

`render_section` branches on the same `section_shape` the parser uses:

- sequence + shape → each element becomes a `###` entry via `render_entry`;
- mapping + shape → scalar siblings first as bullets, then the `list_field` sequence as `###` entries;
- mapping, no shape → a flat bullet list.

`push_field` recurses for nested mappings (indent + 2) and delegates strings to `push_string`.

### What the emitter is careful about

**Quoting.** `push_string` writes values bare when YAML would read them back as the same string, and quotes (via JSON flow) otherwise. A value like `12` or `true` on a string field would otherwise change type on the way back in.

**Multi-line values** are emitted with the first line after `- key: ` and the rest at indent + 4, which the tokenizer's continuation rule folds back together.

**Nested lists of objects** have no bullet form that survives a round trip, so they are emitted as inline JSON. JSON is a subset of YAML, which is exactly what `scalar` feeds `serde_yaml::from_str` — the value returns as itself.

**`is_blank`** omits nulls and empty collections; a config full of `- skills: []` is noise, and serde defaults put them back on the way in. An empty **string** is deliberately not blank: `value: ""` is a choice the author made, and dropping it from a required field produces a document that no longer parses. That distinction was added after it broke `information[].value`.

### Known lossy edge

Trailing whitespace inside a value cannot survive — a bullet ends where the line ends — so values are emitted `trim_end`ed. Interior indentation inside a multi-line value is likewise normalised, since continuation lines are re-emitted trimmed. Nothing else is lost.

## Tests that hold the module together

- `md/mod.rs` — `heading_to_key` normalisation, including the `Pre-execution` hyphen case.
- `md/render.rs` — `the_renderer_knows_about_every_top_level_config_key` serialises a config and asserts every top-level key appears in `SECTIONS`, `PREAMBLE`, or is `name`. A key in none of those lists is silently dropped on render; this test is how the missing `context` section was found.
- `loopsmith-core/tests/md_roundtrip.rs` — the round trip is a property test, not a hope: `roundtrip` renders then reparses; `a_markdown_config_parses_into_the_same_model_as_its_yaml_twin` pins the two grammars to one model; `prose_and_fenced_blocks_are_documentation_not_config` and `a_multi_line_instruction_survives` cover the tokenizer's two subtle rules; `a_misspelled_field_is_refused_in_markdown_too` confirms `deny_unknown_fields` reaches the Markdown path.

## Callers

**Parse side** — `loopsmith_core::load` dispatches on `is_markdown(path)` and calls `parse_md`, so every command that loads a config (`run`, `gate`, `schedule`, …) accepts `.md` without knowing it. `loopsmith-cli`'s `scaffold` parses a user-supplied Markdown config when creating a new loop.

**Render side** — `guided::render` and the web `assemble::render` both emit Markdown as the guided flow's output format, which is why the guided builder produces a self-documenting config rather than YAML.

**Both** — `loopsmith convert` (`loopsmith-cli/src/cmd/convert.rs`) is load-then-emit. Direction is inferred: `is_markdown(config)` means the input is Markdown, so the output is YAML; anything else renders to Markdown. `--to-yaml` forces YAML output. With `--out`, parent directories are created and the path is printed; otherwise the text goes to stdout.

## Adding a section

1. Add the field to `LoopConfig` — the YAML path and the Markdown parser both pick it up with no further work.
2. Add `("my_section", "K. My section")` to `SECTIONS` in `render.rs`, or the renderer drops it. The `the_renderer_knows_about_every_top_level_config_key` test fails if you forget.
3. Only if the section takes `###` entries, add a `section_shape` arm naming its `key_field` and, if the entries nest under a field of a mapping, its `list_field`.