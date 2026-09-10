# MCP Server

# MCP Server (`loopsmith-mcp`)

A local Model Context Protocol server that lets any MCP client — an editor, an agent, Claude Code — read a loop's schedule, ledger, and gate verdict over stdio. It is the read side of the loopsmith control plane, deliberately shaped so that a reasoning agent can see everything about its own run except the one thing it must not control: whether a goal is satisfied.

## The design constraint

`goal_satisfied` is written by `loopsmith-gate` and by nothing else. The MCP surface has no tool that sets it, and there is a test that fails if one ever appears:

```rust
// there_is_no_tool_for_declaring_a_goal_satisfied
assert!(!names.iter().any(|n| n.contains("satisfy")
    || n.contains("set_goal")
    || n.contains("mark_done")));
```

That test is a guardrail, not a formality. `loopsmith_gate_evaluate` exists and calls into the gate, but it *reports* a ruling — the gate re-derives its verdict from evidence on every call, so a client cannot smuggle a conclusion in through the evaluation path either. Everything else the server can mutate (`loopsmith_record_episode`, `loopsmith_scratchpad`) writes records of what happened, never judgements about it.

The transport reinforces this. stdio only, no socket, no bind address. A loop's ledger and goal state are a local record; exposing them over a network would be a different security question than the one this crate answers.

## Protocol

JSON-RPC 2.0, one object per line, newline-delimited. The crate implements the protocol directly rather than pulling in an MCP SDK — the surface is three methods, and hand-writing them avoids taking an async runtime as a dependency for a single stdio loop.

| Method | Behaviour |
|---|---|
| `initialize` | Returns `PROTOCOL_VERSION` (`"2024-11-05"`), `capabilities.tools`, and `serverInfo` with the crate version from `CARGO_PKG_VERSION` |
| `notifications/initialized` | Accepted and answered with `{}` by `handle`, but suppressed by `serve` because it carries no `id` |
| `tools/list` | Returns the catalogue from `tools()` |
| `tools/call` | Dispatches on `params.name` with `params.arguments` |
| anything else | JSON-RPC error `-32601` |

```mermaid
flowchart LR
  stdin[stdin lines] --> serve
  serve --> handle
  handle -->|tools/call| call
  handle -->|initialize / tools/list| R[Response::ok]
  call --> T[tool_* methods]
  T --> core[loopsmith-core / graph / gate]
  T --> store[Store: sled]
  R --> stdout[stdout]
```

## Two kinds of failure

This distinction is the thing most likely to trip up a contributor, and it is intentional.

**Protocol failures** produce a JSON-RPC `error` object: `-32700` for a line that will not parse into a `Request`, `-32601` for an unknown method. Built by `Response::err`.

**Tool failures** produce a *successful* JSON-RPC response whose result carries `isError: true` and the message as text content. A missing `run_id`, a config that will not load, a gate evaluation that blows up on serialization — all of these come back through `Response::ok`. That is what MCP clients expect: the call reached the server and the server has something to say about it, so the model gets a readable message instead of a transport error it cannot reason about.

Concretely, in `Server::call`:

```rust
Err(msg) => Response::ok(
    req.id.clone(),
    json!({ "content": [{ "type": "text", "text": msg }], "isError": true }),
),
```

Every tool method therefore returns `Result<Value, String>` — a plain `String` error, not a typed one, because it is going straight into text content for a model to read. `str_arg` is the shared helper that turns a missing or non-string argument into `missing required argument \`key\``.

Successful results are serialized with `serde_json::to_string_pretty` and wrapped in a single text content block. There is no structured-content path; clients parse the text.

## Tools

`tools()` returns the catalogue as a `serde_json::Value` literal — the JSON Schema for each tool is written by hand next to its description, so adding a tool means editing that array and adding a `match` arm in `Server::call`. Nothing derives one from the other.

### `loopsmith_plan`
Loads a config via `loopsmith_core::load`, runs `loopsmith_graph::plan` over `cfg.graph`, and flattens the result: waves (each `{index, nodes}`), critical path and its cost, total cost, parallel fraction, chosen concurrency, predicted speedup, and the Amdahl ceiling. Pure computation — the store is not touched.

### `loopsmith_gate_evaluate`
Builds an `Evidence` rooted at `workdir` (defaulting to `.`), folds in caller-supplied `metrics` (only values that are `as_f64()` survive) and `artifacts` (only `as_str()`), then calls `loopsmith_gate::evaluate(&cfg, &target, &ev)` and serializes the `TargetVerdict`. `target` is a goal name or the literal `"overall"`.

Note the filtering: a metric passed as a JSON string is silently dropped rather than rejected. Evidence is also re-read from disk on every call, which is what makes revocation work — delete a required artifact and a previously satisfied goal flips back.

### `loopsmith_ledger`
Reads the run's append-only ledger from the store. `limit` returns the **most recent** N entries, not the first N:

```rust
let n = entries.len().saturating_sub(limit as usize);
entries = entries.split_off(n);
```

### `loopsmith_goal_states`
Current gate rulings for every goal plus `overall`, straight from `Store::goal_states`.

### `loopsmith_record_episode`
Constructs an `Episode` and calls `Store::put_episode`, returning `{ recorded: true, seq }`. `run_id`, `node_id`, `provider_id`, and `output` are required; a call without `provider_id` is refused with a tool error. That refusal is the point — an episode without provenance cannot later be judged for independence, so the ledger declines to hold it.

Defaults and quiet extras worth knowing: `iteration` defaults to `0`, `role` defaults to `"builder"`, `prompt_digest` is always written empty, `created_ms` comes from `loopsmith_memory::now_ms()`, and `error` is always `None`. The handler also reads `tokens`, `cost_usd`, and `duration_ms` from the arguments even though the published `inputSchema` does not list them — a client can supply them, but a schema-driven client will never know to.

### `loopsmith_scratchpad`
One tool, two operations, keyed on presence of `value`: supply it to write (`{ written: true }`), omit it to read (`{ value: … }`). The scratchpad is the per-goal reasoning carried between iterations.

## Key types

**`Server<S: Store>`** — generic over the memory backend rather than tied to `SledStore`, which is what makes the tests cheap: they open a store under `loopsmith_util::testing::temp_path` and tear it down afterwards. It holds a single public field, `store`, and every method takes `&self`; there is no interior mutability and no per-connection state, so handling is trivially reentrant.

**`Request`** — every field except `method` is `#[serde(default)]`, so a malformed-but-parseable request degrades into a dispatchable one rather than a parse error. `id: Option<Value>` is what distinguishes a request from a notification.

**`Response`** — `jsonrpc` is a `&'static str` pinned to `"2.0"`; `id`, `result`, and `error` all skip serialization when `None`, so the wire form stays clean. Build it through `Response::ok` / `Response::err` rather than the struct literal.

**`serve`** — the loop. Takes `impl BufRead` and `impl Write` rather than binding stdin/stdout directly, which is why `serve_answers_requests_and_stays_silent_on_notifications` can drive it with a `Cursor` over a string literal. Blank lines are skipped, notifications get no reply per spec, and each response is flushed immediately so a client blocking on a read is not left waiting behind a buffer.

## Where it sits

`loopsmith-mcp` depends on `loopsmith-core` (config loading), `loopsmith-graph` (planning), `loopsmith-gate` (verdicts), and `loopsmith-memory` (the `Store` trait, `Episode`, `LedgerEntry`). Nothing depends on it except the `loopsmith` binary, which wires it up behind `loopsmith mcp --state ./state`. It has no incoming calls from elsewhere in the workspace — it is a leaf, an adapter over the layers below.

Registration, from `runtime/crates/loopsmith-cli/templates/mcp.template.json`:

```json
{ "mcpServers": { "loopsmith": {
    "command": "loopsmith",
    "args": ["mcp", "--state", "./state"]
} } }
```

or `claude mcp add loopsmith -- loopsmith mcp --state ./state`. The template also carries fallbacks for a binary that is not on `PATH` and for `cargo run` during development.

## Contributing notes

**Adding a tool** means three edits in `src/lib.rs`: an entry in the `tools()` array with a hand-written `inputSchema`, a `match` arm in `Server::call`, and a `tool_*` method returning `Result<Value, String>`. Keep argument extraction going through `str_arg` so the error text stays uniform, and keep the return shape a `Value` — the caller handles pretty-printing and content wrapping.

**Adding a mutation** deserves a hard look first. The line the crate holds is that writes record what happened and never rule on it. A tool that lets a client assert an outcome — not just report evidence for one — breaks the independence guarantee even if it avoids the names the guardrail test greps for.

**Packaging.** `Cargo.toml` ships only `/src/**/*` and `/README.md`. The integration tests read `config/examples/` and `config/loop.schema.json` from the repository root, which no crate tarball can contain, so including them would publish tests that cannot pass. If you add a test that reads a fixture from outside `src/`, it will pass locally and be unreachable from the published crate — that is accepted, not an oversight.

**Tests** live inline in `src/lib.rs` and cover the shape of the contract rather than the plumbing: the protocol version and server name, that every listed tool has an object schema, that unknown methods are JSON-RPC errors while missing arguments are tool errors, that provenance-free episodes are refused, that notifications get no reply, and that malformed JSON produces a parse error rather than a panic. Each store-backed test removes its temp directory at the end.