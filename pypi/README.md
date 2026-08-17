# loopsmith-cli

Self-evolving agent loops. The gate is code, so "done" cannot be argued.

```bash
pip install loopsmith-cli
loopsmith doctor
loopsmith new --path ~/loops/my-loop --purpose "keep the module simple"
```

This package downloads the prebuilt Rust binary for your platform on first run,
from the matching
[GitHub release](https://github.com/bitphill/loopsmith/releases), and verifies it
against the release's published `SHA256SUMS` before executing it. The installed
command is `loopsmith`.

Prebuilt for Linux (x86_64 glibc and musl, aarch64), macOS (x86_64 and Apple
silicon), and Windows (x86_64). Anywhere else, build from source:

```bash
cargo install loopsmith
```

The distribution is named `loopsmith-cli` because `loopsmith` on PyPI was already
registered by an unrelated project. The command is `loopsmith` regardless.

The download happens on first run rather than during `pip install`, so an image
that is built without a network and run with one still works.

Full documentation: <https://github.com/bitphill/loopsmith>

MIT licensed.
