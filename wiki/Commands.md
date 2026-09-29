# The command surface

Four nouns and five top-level commands. `loopsmith <noun> --help` prints the
same list this page does, generated from the same source — when the two
disagree, the binary is right.

Every 0.3 spelling still works and prints one line saying where it went. See
[Migration 0.3 → 1.0](Migration-0-3-To-1-0.md).

## `loop` — make and maintain configs

| Command | Does |
|---|---|
| `loop new --path <dir>` | Scaffold a purpose-specific loop. `--path` is required |
| `loop guided [dir] [--edit f]` | Build or edit a config by answering one question at a time. Also `--guided`. `:back` `:next` `:help` `:quit` at any prompt |
| `loop validate <config>` | Check the config against the model. Fails on unfinished manual work, on purpose |
| `loop plan <config>` | Waves, critical path, parallel fraction, predicted speedup |
| `loop convert <config>` | Translate between YAML and Markdown. Both are the same model |
| `loop migrate <config>` | Rewrite a 0.3 config into the 1.0 shape. `--check` / `--write` |
| `loop permissions <config>` | Derive the narrowest grant this config needs. `--write <file>` merges it |

## `run` — a loop, and everything that follows from starting one

| Command | Does |
|---|---|
| `run start <config>` | Execute once. `--dry-run` plans without spending anything. `loopsmith run <config>` is the same command |
| **`run watch <config>`** | **Stay resident and run whenever a trigger fires — this is what makes a loop live for weeks** |
| `run schedule <config>` | Print the launchd agent, crontab line, or scheduled task. `--install` writes it |
| `run resume <config> <run-id>` | Continue from the last checkpoint. `--answer` answers an escalation |
| `run status <config> <run-id>` | Gate rulings per goal |
| `run ledger <config> <run-id>` | Everything that happened, including every stop-gate trigger |
| `run gate <config> --target <goal>` | Ask the gate now, without a provider call |
| `run proposals <config> <run-id>` | What the loop wants changed about itself. It cannot apply these |
| `run prune <config>` | Remove the git worktrees this loop created |

`run resume` and `run resume --answer` are deliberately different commands. A
plain resume must never refund a stuck node's revisions; answering an
escalation is an explicit act by a person.

## `memory` — what the loop remembers across runs

| Command | Does |
|---|---|
| `memory list <config>` | Every record, promoted or not, with where it came from |
| `memory promote <config> <id>` | Promote a record so later runs reuse it |
| `memory forget <config> <id>` | Delete a record |

`memory promote` is the only way a namespace whose promotion rule is
`human_approval` ever promotes anything. See `execution.memory.namespaces` in
[HOW-TO-USE](https://github.com/bitphill/loopsmith/blob/main/HOW-TO-USE.md).

## `skills` — sub-agents

| Command | Does |
|---|---|
| `skills list <config> [--all]` | Sub-agents this loop can see |
| `skills search <terms...>` | Search claudemarketplaces.com and the skills CLI |
| `skills acquire <config> <name>` | Install a sub-agent into quarantine |
| `skills install <config>` | Install everything `execution.default_skills` declares |
| `skills scores <config>` | Rank sub-agents by the gate outcomes that followed their use |

## The rest

| Command | Does |
|---|---|
| `doctor [config]` | What this machine is, and what that stops you doing |
| `providers <config>` | Which providers are usable right now, and why not |
| `web [--port n] [--no-open]` | Build and run loops from a local browser UI. Also `--web` |
| `mcp --state <dir>` | Serve the control plane over stdio MCP |

`web` binds `127.0.0.1` and has no public mode. It spawns this same binary for
every action, so the browser cannot do anything `loopsmith --help` does not
list.

## Global flags

| Flag | Does |
|---|---|
| `--web` | Identical to the `web` subcommand |
| `--guided` | Identical to `loop guided` |
| `--novice` | Walk every question with its explanation, whatever was remembered |
| `--expert` | Hand me a filled-in config in `$EDITOR` instead of asking questions |
| `--ask` | Forget which path was remembered and ask again |

The novice/expert answer is remembered in `~/.loopsmith/wizard.json`. It is
asked once.
