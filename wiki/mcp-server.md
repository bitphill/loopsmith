# MCP Server

# MCP Server (`loopsmith-mcp`)

A local, stdio-only [Model Context Protocol](https://modelcontextprotocol.io) server that exposes three slices of the loopsmith control plane — the schedule, the ledger, and the gate's verdict — to any MCP client (an editor, an agent, Claude Code).

The whole crate is one file: `runtime/crates/loopsmith-mcp/src/lib.rs`.

## The invariant this module is built around

> A model must not be the thing that certifies its own completion.

Everything else in the design follows from that. There is no `set_goal_satisfied` tool, no `mark_done`, no way to write a `TargetVerdict` back into the store. `loopsmith_gate_evaluate` *reports* the gate's ruling by calling `loopsmith_gate::evaluate` on freshly collected evidence; it does not persist anything. `goal_satisfied` is written by `loopsmith-gate` and by nothing else.

This is enforced by a test rather than by convention — `there_is_no_tool_for_declaring_a_goal_satisfied` walks the names returned by `tools()` and fails if any contains `satisfy`, `set_goal`, or `mark_done`. If you add a tool, that test is the gate you have to get past.

The two tools that *do* mutate — `loopsmith_record_episode` and `loopsmith_scratchpad` — write records of what happened, not rulings about it. An episode is refused outright without `provider_id`, because unattributed work cannot later be judged for independence.

## Transport and protocol

JSON-RPC 2.0, newline-delimited, over stdin/stdout. Three methods matter: `initialize`, `tools/list`, `tools/call`. That is the entire surface an MCP client needs, and implementing it directly is why this crate takes no async runtime as a dependency — `serve` is a blocking `for line in input.lines()` loop.

`PROTOCOL_VERSION` is pinned to `"2024-11-05"` and returned verbatim from `initialize`, alongside `serverInfo.name = "loopsmith"` and the crate version from `CARGO_PKG_VERSION`.

Stdio-only is a deliberate boundary, not an unfinished feature. A loop's ledger and goal state are the record of what happened on one machine; putting them on a socket is a different security question than the one this crate answers.

## Request lifecycle

```mermaid
flowchart TD
    serve["serve()<br/>read line"] -->|parse ok| handle["handle()"]
    serve -->|parse fail| parse["Response::err(-32700)"]
    handle -->|initialize / tools/list| ok["Response::ok"]
    handle -->|tools/call| call["call()"]
    handle -->|other| unknown["Response::err(-32601)"]
    call --> tool["tool_plan / tool_gate / tool_ledger /<br/>tool_goal_states / tool_record / tool_scratchpad"]
    tool -->|Ok / Err| wrap["Response::ok with<br/>content[] + isError"]
```

`serve` checks `req.id.is_none()` *before* dispatching: notifications are still handled (so `notifications/initialized` does its work) but the response is dropped rather than written, per the spec. The test `serve_answers_requests_and_stays_silent_on_notifications` feeds three lines in and asserts exactly two come out.

Output is flushed after every line. A malformed line produces a `-32700` parse error with `id: null` and the loop continues — `malformed_json_gets_a_parse_error_rather_than_a_panic` covers that.

## Two error channels, and when each is used

This distinction trips people up, so it is worth stating plainly:

| Failure | Shape | Example |
|---|---|---|
| Protocol-level | JSON-RPC `error` object, `result` absent | unknown method → `-32601`; unparseable line → `-32700` |
| Tool-level | JSON-RPC **success**, `result.isError == true`, message in `content[0].text` | missing `run_id`, config that won't load, unknown tool name |

Every failure inside `call` — including `unknown tool` — comes back as a *successful* JSON-RPC response carrying `isError: true`. That is what MCP clients expect: a tool that failed is a normal result the model should see and react to, not a transport fault. Internally the tool methods all return `Result<Value, String>` and `call` does the wrapping in one place.

Successful results are serialized with `serde_json::to_string_pretty` into a single text content block, so the client sees readable JSON rather than a nested object.

## Tool catalogue

`tools()` returns the catalogue as a `serde_json::Value` array — a free function, not a method, which is what lets the independence test inspect it without opening a store.

| Tool | Required args | What it does |
|---|---|---|
| `loopsmith_plan` | `config_path` | Loads the config, runs `loopsmith_graph::plan` on `cfg.graph`, returns waves, critical path and cost, parallel fraction, chosen concurrency, predicted speedup, speedup ceiling |
| `loopsmith_gate_evaluate` | `config_path`, `target` | Builds `Evidence` from `workdir` + supplied metrics/artifacts, returns the `TargetVerdict` from `loopsmith_gate::evaluate` |
| `loopsmith_ledger` | `run_id` | `Store::ledger`, optionally tail-limited |
| `loopsmith_goal_states` | `run_id` | `Store::goal_states` — current gate rulings for every goal plus `overall` |
| `loopsmith_record_episode` | `run_id`, `node_id`, `provider_id`, `output` | Builds an `Episode` and `Store::put_episode`; returns the assigned `seq` |
| `loopsmith_scratchpad` | `run_id`, `key` | Read/write in one tool — omit `value` to read |

Details that aren't obvious from the schemas:

**`loopsmith_ledger`'s `limit` is a tail, not a head.** `tool_ledger` computes `entries.len().saturating_sub(limit)` and `split_off`s at that point, so you get the *last* N entries. That's the useful default for an agent asking "what just happened," and `saturating_sub` means an oversized limit returns everything rather than panicking.

**`loopsmith_gate_evaluate` silently drops ill-typed evidence.** Metrics are taken only via `as_f64` and artifacts only via `as_str`; a metric sent as a JSON string, or an artifact sent as a number, is skipped without complaint. `workdir` defaults to `"."`, which means the gate resolves artifact paths relative to wherever the *server* process was started — not the client's cwd. Pass it explicitly if the two can differ.

**`tool_record` accepts more than it advertises.** Beyond the declared schema it reads optional `tokens`, `cost_usd`, and `duration_ms`. `role` defaults to `"builder"`, `iteration` to `0`, `error` is always `None`, and `prompt_digest` is written as an empty string — the MCP path has no prompt to digest. `created_ms` comes from `loopsmith_memory::now_ms()`.

**Argument validation is one helper.** `str_arg(args, key)` returns `Err("missing required argument \`key\`")`, and the error text carries the key name. Tests assert on that substring (`a_missing_argument_is_reported_as_a_tool_error` looks for `run_id`; `recording_without_provenance_is_refused` looks for `provider_id`), so the message format is load-bearing.

## Types

```rust
pub struct Request  { jsonrpc: String, id: Option<Value>, method: String, params: Value }
pub struct Response { jsonrpc: &'static str, id: Option<Value>, result: Option<Value>, error: Option<Value> }
pub struct Server<S: Store> { pub store: S }
```

`Request` uses `#[serde(default)]` on everything except `method` — a request missing `jsonrpc` or `params` still deserializes, and only a missing `method` is a parse error. `Response` skips `None` fields on serialize, so `result` and `error` are never both present on the wire. Construct responses through `Response::ok` / `Response::err` rather than by hand; they set `jsonrpc` for you.

`Server` is generic over `loopsmith_memory::Store`, which is what makes the handler testable against any backing store. The tests use `SledStore` over a `loopsmith_util::testing::temp_path`.

## Where it sits

`loopsmith-mcp` is a leaf in the dependency graph — nothing in the workspace calls into it; the `loopsmith` binary's `mcp` subcommand constructs a `Server` and hands it stdin/stdout. It reaches downward into four crates:

- **`loopsmith-core`** — `load(path)` for the A–J config, used by both `tool_plan` and `tool_gate`
- **`loopsmith-graph`** — `plan(&cfg.graph)` for waves and Amdahl-driven concurrency
- **`loopsmith-gate`** — `Evidence`, `TargetVerdict`, `evaluate`
- **`loopsmith-memory`** — the `Store` trait, `Episode`, `LedgerEntry`, `now_ms`

Everything returned to the client is `serde_json::to_value` over those crates' own types, so their `Serialize` impls *are* this crate's public response schema. Changing a field on `TargetVerdict` or `LedgerEntry` changes the MCP output with no compiler error here to warn you.

## Running it

Register the binary as a local MCP server (see `runtime/crates/loopsmith-cli/templates/mcp.template.json`):

```bash
claude mcp add loopsmith -- loopsmith mcp --state ./state
```

or drop the `mcpServers` block into `.mcp.json` at your project root. The template also carries fallbacks for when the binary isn't on `PATH` — an absolute path, or `cargo run --release -p loopsmith -- mcp --state ./state` for development.

## Contributing notes

**Adding a tool** means three edits in lockstep: an entry in `tools()`, an arm in `call`'s match, and a `tool_*` method returning `Result<Value, String>`. Use `str_arg` for required strings so the error text stays uniform. If the tool mutates, be able to say why what it writes is a record rather than a ruling.

**The crate's `include` is deliberately narrow** — only `/src/**/*` and `/README.md`. The integration tests read `config/examples/` and `config/loop.schema.json` from the repository root, which no crate tarball can contain; shipping them would hand a published crate tests that cannot pass. Don't widen `include` to "fix" missing test fixtures.

**Test hygiene:** each test opens a `SledStore` in a temp directory and removes it at the end. There is no shared fixture and no cleanup guard, so a panicking test leaks its directory — acceptable, but don't build on the assumption that it's cleaned up.