## What this changes

<!-- A sentence or two. Link the issue it closes: "Closes #123". -->

## Why

## Checklist

- [ ] `cargo test --workspace` passes (from `runtime/`)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] I added or changed a test for any behaviour I changed
- [ ] If I touched `config/examples/`, `packaging/`, `assets/architecture.mmd`, the version, or the web UI source, I ran the matching `tools/sync-*.sh` (or rebuilt `dist`) and committed the output
- [ ] I updated the README, wiki or `CHANGELOG.md` if a user would notice the change
- [ ] I did not change the version number, publish anything, or include a secret

See [CONTRIBUTING.md](../CONTRIBUTING.md).
