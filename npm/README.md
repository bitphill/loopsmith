# @bitphill/loopsmith

Self-evolving agent loops. The gate is code, so "done" cannot be argued.

```bash
npm install -g @bitphill/loopsmith
loopsmith doctor
loopsmith new --path ~/loops/my-loop --purpose "keep the module simple"
```

This package downloads the prebuilt Rust binary for your platform from the
matching [GitHub release](https://github.com/bitphill/loopsmith/releases) and
verifies it against the release's published `SHA256SUMS` before installing it.
The installed command is `loopsmith`.

Prebuilt for Linux (x86_64 glibc and musl, aarch64), macOS (x86_64 and Apple
silicon), and Windows (x86_64). Anywhere else, build from source:

```bash
cargo install loopsmith
```

The package name is scoped because `loopsmith` on npm was already registered by
an unrelated project. The command is `loopsmith` regardless.

Full documentation: <https://github.com/bitphill/loopsmith>

MIT licensed.
