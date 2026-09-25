<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/main/assets/loopsmith-logo-256.png" alt="loopsmith" width="180" />
  <h1>loopsmith</h1>
  <p><em>Hand a repeating job to an AI. A checker you wrote — not the AI — decides when it is done.</em></p>
</div>

[![npm](https://img.shields.io/npm/v/%40bitphill%2Floopsmith?logo=npm&logoColor=white&label=npm&color=cb3837)](https://www.npmjs.com/package/@bitphill/loopsmith)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![platforms](https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A)
![node](https://img.shields.io/badge/node-%E2%89%A518-339933?logo=nodedotjs&logoColor=white)

```bash
npm install -g @bitphill/loopsmith
loopsmith doctor
```

Or without installing anything permanently:

```bash
npx @bitphill/loopsmith doctor
```

> **This package is a Rust binary, not a JavaScript library.** There is nothing
> to `require()` or `import` — it installs a `loopsmith` command. To drive loops
> from Node, spawn the CLI; its exit codes are its API. The install downloads a
> prebuilt binary for your platform, so there is no Rust toolchain involved.

---
