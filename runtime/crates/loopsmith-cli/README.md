# loopsmith

Self-evolving agent loops. The gate is code, so "done" cannot be argued.

```bash
cargo install loopsmith
loopsmith new --path ~/loops/nightly-refactor --purpose "keep the module simple"
cd ~/loops/nightly-refactor
loopsmith validate loop.yaml && loopsmith plan loop.yaml && ./run.sh
```

You describe a purpose in a config — goals, how each is checked, what counts as
success, when to stop, what the loop may never do. loopsmith handles scheduling,
provider routing, memory, verification, and termination, and can run for weeks
without you. One rule holds the whole design up:

> A model must not be the thing that certifies its own completion.

`goal_satisfied` is written by a deterministic Rust gate and by nothing else,
and the gate can **revoke**: delete a required artifact and a satisfied goal
flips back.

`loopsmith validate` fails on purpose until every `pre_execution` step is marked
done. Do the task by hand once — the manual run *is* the spec.

## Platforms

Linux, macOS, and Windows. `loopsmith doctor` reports what the host actually is
— which bash, GNU or BSD userland, which scheduler — rather than inferring it
from the build target, and every new loop ships both POSIX `sh` and `.cmd`
launchers so a loop directory keeps working after it moves between machines.

## The rest of the workspace

This crate is the binary. The libraries behind it are published separately and
compile automatically as its dependencies: `loopsmith-util`, `loopsmith-core`,
`loopsmith-memory`, `loopsmith-graph`, `loopsmith-gate`, `loopsmith-provider`,
`loopsmith-skills`, `loopsmith-mcp`.

Full documentation, the A–J config reference, and thirteen worked examples:
<https://github.com/bitphill/loopsmith>

MIT licensed.
