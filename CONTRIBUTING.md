# Contributing to loopsmith

Thanks for looking. loopsmith is a Rust workspace with a small web front end, a
set of install scripts, and a handful of packaging wrappers. Bug reports, doc
fixes, new example loops, tests and code are all welcome.

If you are not sure where to start, look for issues labelled
[`good first issue`](https://github.com/bitphill/loopsmith/labels/good%20first%20issue)
or [`help wanted`](https://github.com/bitphill/loopsmith/labels/help%20wanted).
For a question or an idea that is not a bug yet, use
[Discussions](https://github.com/bitphill/loopsmith/discussions).

By taking part you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
To report a security problem, read [SECURITY.md](SECURITY.md) instead of opening
an issue.

## Where things live

| Path | What it is |
|---|---|
| `runtime/` | The Cargo workspace. Twelve crates: `loopsmith-core` (model and validator), `-run` (engine), `-graph`, `-memory`, `-provider`, `-gate`, `-skills`, `-wizard`, `-web`, `-mcp`, `-util`, and `-cli` (the binary). |
| `runtime/crates/loopsmith-web/web/` | The browser UI source (TypeScript). It is built into `../src/dist/`, which is committed. |
| `config/examples/` | The worked example loops, as `.yaml` and a generated `.md` twin. |
| `config/loop.schema.json` | The config schema. |
| `skills/` | The two agent skills, `loopsmith` and `loopsmith-reference`. |
| `install.sh`, `install.bat`, `install.ps1`, `installers/` | The universal installer. |
| `npm/`, `pypi/`, `Formula/`, `packaging/` | Wrappers and README sources for each registry. |
| `site/`, `wiki/` | The landing page and the code wiki. |
| `tools/` | The scripts CI uses to keep generated files honest. |

## Build and test

You need Rust 1.85 or newer (the declared `rust-version`). Everything below runs
from the repository root unless it says otherwise.

```bash
cd runtime
cargo build --release                                  # the binary: runtime/target/release/loopsmith
cargo test --workspace --no-fail-fast                  # the whole suite
cargo clippy --workspace --all-targets -- -D warnings  # CI treats warnings as errors
```

CI does not run `cargo fmt --check`: the tree is not rustfmt-clean and some
layouts are done by hand on purpose. Match the style of the file you are in
instead, and please do not send a reformatting pass.

The browser UI can be left out of a build. CI also checks that this still
works and that it pulls in no async runtime:

```bash
cd runtime
cargo clippy -p loopsmith --no-default-features --all-targets -- -D warnings
```

### Checks that keep generated files in step

CI runs these, so run the ones that match what you touched:

```bash
./tools/sync-version.sh --check          # the version lives in runtime/Cargo.toml; other files repeat it
./tools/sync-examples.sh --check         # each example's .md twin and its copy inside the web crate
./tools/sync-package-readmes.sh --check  # npm, PyPI and crates.io READMEs are built from packaging/
./tools/render-diagrams.sh --check       # assets/architecture.* come from assets/architecture.mmd
./tools/check-readability.sh             # README-FOR-DUMMIES.md must stay at grade 6 or below
./tools/check-readability.sh packaging/README.body.md 10
```

The `sync-*` and `render-diagrams` scripts have a no-argument form that rewrites
the files for you. Run that, then commit the result.

### Changing the browser UI

The binary serves the committed `runtime/crates/loopsmith-web/src/dist/`. If you
change anything under `web/`, rebuild and commit `dist` too, or CI will fail:

```bash
cd runtime/crates/loopsmith-web/web
npm ci
npm run build
```

### Changing the installers

`install.sh` and `install.ps1` both read `installers/manifest.json`, so a fact
they share (the repository URL, the build command, the install directory) goes
there once. CI runs the installer for the host OS twice, to prove it is safe to
re-run. If you change an installer, run it on the OS you changed it for.

## Proposing a new example loop

The examples are how most people meet loopsmith, so a new one has to be a job a
real person would repeat, with a gate a script can decide.

1. **Open an issue first** using the "Example loop or new idea" template. Say what
   the job is, who does it, and how you would tell it was done well in terms a
   script could check.
2. Start from an existing example in `config/examples/`, or from
   `loopsmith loop new --path <dir> --config-file <example>` in a scratch
   directory outside the repository. Write the `.yaml`; do not hand-write the
   `.md`.
3. Keep `intent.prerequisites` unfinished, like every shipped example. Each
   example must refuse to validate for exactly one reason: that step.
   `loopsmith loop validate config/examples/<name>-loop.yaml` should report
   `1 error(s), 0 warning(s)`.
4. Regenerate the twin and the embedded copy with `./tools/sync-examples.sh`
   (it needs a built `loopsmith`; see above). Then add the new file to the
   `EMBEDDED` list in `runtime/crates/loopsmith-web/src/examples.rs`.
5. Add a row to [`wiki/Examples.md`](wiki/Examples.md) and to the list in the
   README's Examples section. The README, `README-FOR-DUMMIES.md`, the site and the
   web UI each say how many examples there are; update those numbers too, and
   rebuild the UI as above.
6. Run `cargo test --workspace`, then the checks above.

Never put a real key, token or personal address in an example. Name the
environment variable under `requires_env`, and nothing more.

## Pull requests

Small, focused pull requests are easiest to review. For anything larger than a
typo or a doc fix, please open an issue or a Discussion first so we can agree on
the approach before you spend the time.

Before you open one, check that:

- [ ] `cargo test --workspace` passes from `runtime/`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] you added or changed a test for any behaviour you changed
- [ ] if you touched `config/examples/`, `packaging/`, `assets/architecture.mmd`,
      the version, or the web UI source, you ran the matching sync script or
      rebuild and committed the output
- [ ] you updated the README, the wiki or `CHANGELOG.md` if a user would notice
      the change
- [ ] the pull request says what changed and why, and links the issue it closes

Write commit messages as a plain sentence about what changed. Do not change the
version number or publish anything in a pull request; releases are cut by the
maintainer.

## Reporting a bug

Open an issue with the bug report template. The output of `loopsmith --version`
and `loopsmith doctor` answers most of the first questions. Never paste an API
key into an issue. If one ends up there by mistake, rotate it: deleting the
message is not enough.
