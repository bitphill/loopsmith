# {{name}}

A loopsmith loop.

**Purpose:** {{purpose}}

## Run it

```bash
./run.sh          # macOS, Linux, BSD, Git Bash, WSL
run.cmd           # Windows cmd.exe or PowerShell
```

That is `loopsmith run {{config_file}}` with this directory's absolute paths already filled in. If the loop stops before it is done, `./resume.sh <run-id>` (or `resume.cmd <run-id>`) picks up from the last checkpoint — the run id is printed at the end of every run and appears in `logs/`.

Both launchers are written on every platform, so this directory keeps working after it moves to a different kind of machine.

The long way, when you want to see each step:

```bash
loopsmith validate {{config_file}}   # the A-J model must be complete
loopsmith plan     {{config_file}}   # waves, critical path, predicted speedup
loopsmith run      {{config_file}}
```

## Before the first run

`pre_execution` in `{{config_file}}` is deliberately unfinished. Run the task by hand once, record what you learned, and set each step to `done: true`. Validation fails until you do, because automating a process you cannot describe produces fast, confident garbage.

## Secrets

Providers name the environment variables they need under `requires_env`. loopsmith checks that those variables **exist** and never reads their values, so a key never reaches a prompt, a log, or the ledger. Export them in your shell:

```bash
export OPENAI_API_KEY=...   # in your shell, not in this repo
```

Never paste a key into a chat window, a config file, or an issue. If one is ever pasted somewhere it should not be, rotate it rather than deleting the message.

## Layout

| Path | What it is |
|---|---|
| `{{config_file}}` | The A-J config: goals, validations, success, stop gates, schedules, constraints, phases, default skills |
| `run.sh` / `resume.sh` | This loop's exact commands, with absolute paths (POSIX `sh`) |
| `run.cmd` / `resume.cmd` | The same two commands for `cmd.exe` |
| `scripts/compat.sh` | Source this in a detector: `sed_i`, `stat_size`, `readlink_f`, `sha256`, `require`, `need_bash` |
| `.mcp.json` | MCP server definition, so an agent can read this loop's memory |
| `.claude/settings.local.json` | Permission grant this config needs |
| `marketplaces.json` | Sub-agent index sources |
| `state/` | sled memory: episodes, goal state, ledger, checkpoints, summaries |
| `logs/` | Plain-text run logs, one per run |
| `out/` | Deliverables the nodes produce |
| `proposals/` | Changes the loop wants to make to itself — review these |
| `generated-skills/` | Auto-created sub-agents awaiting promotion |
