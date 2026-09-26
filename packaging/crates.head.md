<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/main/assets/loopsmith-logo-256.png" alt="loopsmith" width="180" />
  <h1>loopsmith</h1>
  <p><em>Hand a repeating job to an AI. A checker you wrote — not the AI — decides when it is done.</em></p>
</div>

[![crates.io](https://img.shields.io/crates/v/loopsmith?logo=rust&logoColor=white&label=crates.io&color=C1272D)](https://crates.io/crates/loopsmith)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![platforms](https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A)
![rust](https://img.shields.io/badge/rust-1.85%2B-C1272D?logo=rust&logoColor=white)

```bash
cargo install loopsmith
loopsmith doctor
```

> **This crate is the binary.** `cargo install loopsmith` builds it from source
> — a few minutes on a cold cache. The browser UI is on by default; to drop the
> whole async dependency tree, `cargo install loopsmith --no-default-features`.
>
> The library crates underneath are published separately and are what you want
> if you are building on the pieces rather than running loops:
> [`loopsmith-core`](https://crates.io/crates/loopsmith-core) (the config model
> and its validation),
> [`loopsmith-gate`](https://crates.io/crates/loopsmith-gate) (the deterministic
> verdicts), [`loopsmith-graph`](https://crates.io/crates/loopsmith-graph) (DAG
> scheduling and Amdahl sizing),
> [`loopsmith-run`](https://crates.io/crates/loopsmith-run) (the run lifecycle),
> and seven more.

---
