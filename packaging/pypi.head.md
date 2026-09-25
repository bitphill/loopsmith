<div align="center">
  <img src="https://raw.githubusercontent.com/bitphill/loopsmith/main/assets/loopsmith-logo-256.png" alt="loopsmith" width="180" />
  <h1>loopsmith</h1>
  <p><em>Hand a repeating job to an AI. A checker you wrote — not the AI — decides when it is done.</em></p>
</div>

[![PyPI](https://img.shields.io/pypi/v/loopsmith-cli?logo=pypi&logoColor=white&label=PyPI&color=3775A9)](https://pypi.org/project/loopsmith-cli/)
[![license](https://img.shields.io/badge/license-MIT-C8CAD1?labelColor=222)](https://github.com/bitphill/loopsmith/blob/main/LICENSE)
![platforms](https://img.shields.io/badge/os-linux%20%7C%20macos%20%7C%20windows-2A5A8A)
![python](https://img.shields.io/badge/python-%E2%89%A53.8-3776AB?logo=python&logoColor=white)

```bash
pipx install loopsmith-cli      # or: pip install loopsmith-cli
loopsmith doctor
```

> **The distribution is `loopsmith-cli`; the command is `loopsmith`.** The
> shorter name was taken on PyPI. This package is a Rust binary with a thin
> Python launcher — there is nothing to `import`. The binary for your platform
> is downloaded on first run, so there is no Rust toolchain involved.

---
