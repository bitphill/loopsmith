# Platform Utilities

# Platform Utilities (`loopsmith-util`)

`loopsmith-util` is the base of the workspace dependency graph. Every other crate — `loopsmith-core`, `-memory`, `-graph`, `-gate`, `-provider`, `-skills`, `-mcp`, and the CLI — depends on it, and nothing depends *on* those from here. It has **zero dependencies**, not even `serde`, because anything added here is added to every build in the workspace.

The crate answers three questions that came up repeatedly across crates and were being answered differently (and sometimes wrongly) in each one:

1. **Is this command available, and where?** → `which` / `is_executable`
2. **What time is it?** → `now_ms`
3. **What is this host actually capable of?** → the `platform` module

Plus one opt-in extra: `testing::temp_dir`, the shared temp-path allocator used by test suites across the workspace.

---

## Layout

| Path | Contents |
|---|---|
| `src/lib.rs` | `which`, `is_executable`, `now_ms`, plus private `first_executable` / `path_extensions`; `testing` submodule behind the `testing` feature |
| `src/platform.rs` | `Os`, `Userland`, `BashVersion`, `Platform`, `home_dir`, `preferred_schedulers` |

Two features' worth of build surface: the default (empty) and `testing`. Test scaffolding is gated so it never lands in a release build; crates that want it enable `loopsmith-util = { …, features = ["testing"] }` from `[dev-dependencies]`, and resolver 2 keeps that activation out of the normal dependency build.

The published tarball ships `src/` and `README.md` only (`include` in `Cargo.toml`) — integration tests elsewhere in the workspace read `config/examples/` and `config/loop.schema.json` from the repo root, which a crate tarball cannot carry.

---

## Command resolution

### `which(cmd: &str) -> Option<PathBuf>`

Resolves a command the way a shell would, with two branches:

- **Anything containing a path separator** (or an absolute path) is checked directly. It is *not* joined onto `PATH` entries — three earlier private copies of this function did exactly that, and consequently returned `None` for every absolute path handed to them.
- **A bare name** is joined onto each `PATH` entry in order, first hit wins.

Both branches funnel into the same private helper:

```
which ──► first_executable ──┬─► is_executable        (the direct hit)
                             └─► path_extensions ──► is_executable  (per suffix)
```

`first_executable` tries the path as given, then appends each entry from `path_extensions()`. The suffix is **appended, never substituted** — `Path::set_extension` would rewrite `check.sh` into `check.exe`, and a command name is allowed to contain a dot. So on Windows `check.sh` may legitimately resolve to `check.sh.exe`.

`path_extensions()` returns an empty `Vec` on unix (the executable bit is the entire answer) and parses `PATHEXT` on Windows, falling back to `.COM;.EXE;.BAT;.CMD` — Windows' own default — when the variable is unset. Honouring `PATHEXT` rather than hardcoding a list is how a machine gets to declare that `.ps1` or `.py` counts as a command.

### `is_executable(p: &Path) -> bool`

On unix: `metadata(p)` must report a file with any of the `0o111` bits set. Off unix there is no permission bit to consult, so it degrades to `p.is_file()` rather than pretending to know more.

The executable check is the point of the whole function. Of the three copies of `which` this replaced, only one checked the bit — so a non-executable file named `curl` sitting on `PATH` read as "curl is available" to two callers, and the truth surfaced much later as an opaque spawn error.

### Why the Windows suffix path matters downstream

Without the `PATHEXT` loop, `which` returns `None` for every command on a Windows machine, because `git` there is a file called `git.exe` and the bare join matches nothing. Everything downstream then dutifully reports the falsehood it was handed: `doctor` (`src/cmd/doctor.rs`) says no tool is installed, `Platform::detect` finds no scheduler, and worktree isolation degrades to a shared directory citing "git not on PATH" — on a machine where git is very much on PATH.

---

## The clock

```rust
pub fn now_ms() -> u64
```

Milliseconds since the Unix epoch, and the only clock in the workspace. Timestamps are stored as plain numbers so the ledger sorts without a date parser. A clock that runs backwards yields `0` rather than panicking — a wrong ledger timestamp is a nuisance; a panic partway through an unattended run is not.

---

## The `platform` module

`cfg!(target_os = …)` describes the machine the binary was *compiled for*. Three facts that change what loopsmith and its generated scripts may assume are not knowable that way:

- **The bash on `PATH` may be from 2007.** macOS still ships 3.2.57 for licence reasons. Associative arrays, `${x,,}`, `mapfile`, and `&>>` all arrived in 4.0.
- **`sed`, `stat`, `readlink` take different flags** by userland. `sed -i` requires an argument on BSD and must not have one on GNU.
- **The scheduler is whatever is installed**, not what the OS is famous for. A container has neither `launchctl` nor `crontab`; a Mac has both.

Everything here **probes and reports**. Nothing decides on the caller's behalf, and nothing is cached across processes — a probe costs one `--version` call and a run is not a hot loop.

### `Os`

A five-variant enum (`MacOs`, `Linux`, `FreeBsd`, `Windows`, `Other`) over `std::env::consts::OS`; the BSDs collapse into one variant. This one genuinely *is* a compile-time fact, wrapped here so callers have a single place to ask, sitting next to the facts that are not. `as_str()` gives the stable lowercase name used in reports; `is_windows()` answers "does this host run `.cmd` batch files rather than `#!` scripts".

### `home_dir()`

Reads whichever variable this OS actually uses. On Windows: `HOME` first (Git Bash and MSYS set it, and it is the more specific answer when both exist), then `USERPROFILE`, then `HOMEDRIVE` + `HOMEPATH` concatenated as a last resort on bare `cmd.exe`. Everywhere else, `HOME`. Empty values are filtered out at every step rather than accepted as a home directory.

Code that reads `HOME` directly loses the home directory on Windows entirely, and the failure surfaces far away as "cannot locate LaunchAgents" or as a loop cheerfully scaffolding itself into a relative path.

### `Userland`

`Gnu` / `Bsd` / `Unknown`, probed by running `sed --version`:

| Probe result | Verdict |
|---|---|
| exit 0, output contains `gnu` or `busybox` | `Gnu` |
| exit 0, output matches neither | `Unknown` |
| ran but exited non-zero (BSD `sed` refuses `--version`) | `Bsd` |
| could not spawn | `Unknown` |

Never inferred from `Os`: Homebrew's `coreutils` puts GNU tools ahead of BSD ones on a Mac, and a stripped container may have busybox.

`sed_in_place()` returns the argv fragment for an in-place edit — `["-i"]` for GNU, `["-i", ""]` for BSD. **`Unknown` deliberately takes the BSD spelling**: GNU rejects the empty suffix loudly, whereas BSD given the GNU spelling silently swallows the *next* argument as a backup suffix, which is how a script ends up editing a file called `-e`. When guessing, guess toward the louder failure.

### `BashVersion`

```rust
pub struct BashVersion { pub major: u32, pub minor: u32, pub raw: String }
```

`parse(first_line)` reads the first line of `bash --version` — a shape unchanged since 2.0, e.g. `GNU bash, version 3.2.57(1)-release (x86_64-apple-darwin24)`. It splits on `"version "`, takes the leading run of digits and dots, and parses major (required) and minor (defaulting to 0, so `version 5` parses as 5.0). Anything that is not a bash banner yields `None`. `raw` keeps the whole trimmed line for the `doctor` report.

`MODERN_MAJOR = 4` and `is_modern()` encode the single line that matters: everything a generated script might want — associative arrays, `${x,,}`, `mapfile`, `&>>`, `**` — arrived in 4.0. Below that, the script must be written to POSIX `sh`.

The private `probe(command)` runs `<command> --version` and feeds line one to `parse`, returning `None` if the spawn fails.

### `Platform`

The whole picture, gathered by one `Platform::detect()`:

```mermaid
graph TD
    D["Platform::detect"] --> O["Os::detect"]
    D --> U["Userland::detect<br/>(sed --version["version)"]"]
    D --> B["probe: bash"]
    D --> S["probe: /bin/sh"]
    D --> P["preferred_schedulers(os)"]
    P --> W["which — keep only<br/>what is installed"]
```

Fields:

- `os`, `userland` — as above.
- `bash: Option<BashVersion>` — `bash` on `PATH`, when there is one. A machine may have none.
- `sh_bash: Option<BashVersion>` — `/bin/sh`, but only when it is a bash in POSIX mode. `None` on Debian, where it is `dash` — worth knowing, since dash rejects `[[`, `local -n`, and `echo -e` that a bash-as-sh would have accepted by accident.
- `schedulers: Vec<&'static str>` — the candidate list for this OS, filtered by `which` down to what is actually installed, in preference order.

Methods:

- `scheduler()` — the first installed scheduler, or `None` when the machine has none.
- `has_modern_bash()` — `false` when bash is missing entirely. A script that cannot be run is not a script that may assume anything.
- `portability_note()` — one line explaining why a generated script sticks to POSIX `sh`, or `None` when nothing is holding it back. Distinguishes "no bash on PATH" from "bash 3.2 predates 4.0".

### `preferred_schedulers(os)`

The **candidate** list, best first — not the answer. `Platform::detect` filters it through `which`.

| OS | Candidates |
|---|---|
| macOS | `launchctl`, `crontab` |
| Linux / BSD | `crontab`, `systemctl` |
| Windows | `schtasks`, `crontab` |
| Other | `crontab` |

launchd leads on macOS because it survives a reboot without the user enabling anything else. Windows leads with `schtasks`, the only one of the four that ships with the OS — `crontab` still follows it, since a machine with Cygwin or WSL interop on `PATH` has a real cron, and the rule everywhere is "prefer the native tool, still find the other one". Getting this wrong is a fixed bug: Windows previously fell into the catch-all arm, was offered only `crontab`, and `schedule` reported "no scheduler" on a machine with a working Task Scheduler.

---

## `testing` (feature-gated)

```rust
pub fn temp_path(tag: &str) -> PathBuf   // unique path — NOT created
pub fn temp_dir(tag: &str) -> PathBuf    // temp_path + create_dir_all
pub fn cleanup(p: &Path)                 // best-effort remove_dir_all
```

Names are `loopsmith-{tag}-{pid}-{now_ms}-{counter}` under `std::env::temp_dir()`.

The **`AtomicU64` counter is the load-bearing part.** This existed six times in four shapes across the workspace, and two of those shapes omitted it. Tests run in parallel threads within a single process, so pid and millisecond timestamp do not separate two directories created in the same millisecond — and when two tests share a directory, `sled` reports a lock error that reads like a backend bug rather than a test collision.

`cleanup` swallows its error on purpose: a leaked temp directory must never fail a test.

This is the most widely-called item in the crate. Consumers include `loopsmith-gate`, `loopsmith-provider`, `loopsmith-mcp`, `loopsmith-memory`'s stores, and CLI test suites across `schedule.rs`, `logging.rs`, `permissions.rs`, `run/publish.rs`, `run/export.rs`, `web/detect.rs`, `web/exec.rs`, and `web/assemble.rs`.

---

## How the rest of the workspace uses this

Two representative flows:

**`doctor` reporting the host.** `execute` (`src/cmd/doctor.rs`) calls `Platform::detect`, which fans out to `Userland::detect`, `BashVersion::probe` → `parse`, and the `which`-filtered scheduler list. `doctor` also calls `is_executable` directly from `config_notes` when checking user-configured command paths.

**The web UI staging a job.** `start_job` (`src/web/api.rs`) → `write_scratch` (`src/web/assemble.rs`) → `temp_dir` → `temp_path`. Run publishing takes the same route through `repo` in `src/run/publish.rs`.

The pattern in both: `loopsmith-util` supplies the raw fact, and the caller decides what to do with it. `portability_note()` returns a sentence; it does not choose a shell. `preferred_schedulers` returns candidates; `Platform::detect` narrows them and `scheduler()` picks a head, but installing anything is the CLI's business.

---

## Contributing

**The bar for adding something here is that it has already been written more than once elsewhere.** Every item currently in the crate meets that bar, and the doc comments record the specific bug each duplicate copy carried. A helper with a single caller belongs in that caller's crate.

**Do not add a dependency.** Zero dependencies is a property of the whole workspace's build, and this is the crate that would break it.

**Keep detection honest.** Probe, report, and let the caller decide. When a probe cannot answer, prefer the variant whose downstream failure is loud (`Userland::Unknown` taking the BSD `sed -i ''` spelling is the worked example) over the one that fails silently.

**Tests must not mutate shared process state.** `std::env::set_var` races every other test thread that reads the environment — the same class of collision the temp-dir counter exists to prevent. The existing tests assert through `which`'s absolute-path branch and through `is_executable` directly instead of swapping `PATH`. Likewise, platform-specific claims are asserted under `#[cfg(unix)]` / `#[cfg(not(unix))]` with each platform's own rule: "a plain file is not executable" is a unix statement, and asserting it where there is no executable bit asserts unix semantics on a system that has none.