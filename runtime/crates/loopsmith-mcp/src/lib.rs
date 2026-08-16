//! Local MCP server over stdio.
//!
//! Exposes the three things a reasoning agent legitimately needs from the
//! control plane — the schedule, the ledger, and the gate's verdict — without
//! handing it the ability to declare itself finished. Note what is *not* here:
//! there is no `set_goal_satisfied` tool. The gate decides that, and the gate
//! is not reachable as a mutation.
//!
//! Protocol: JSON-RPC 2.0, newline-delimited, `initialize` / `tools/list` /
//! `tools/call`. That is the whole surface an MCP client needs, and writing it
//! directly avoids taking an async runtime as a dependency for one server.

use loopsmith_gate::{Evidence, TargetVerdict};
use loopsmith_memory::{Episode, LedgerEntry, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

impl Response {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Response {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }
    pub fn err(id: Option<Value>, code: i64, message: &str) -> Self {
        Response {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(json!({ "code": code, "message": message })),
        }
    }
}

/// Tool catalogue. Read-only except for episode and scratchpad writes, both
/// of which are records of what happened rather than rulings about it.
pub fn tools() -> Value {
    json!([
        {
            "name": "loopsmith_plan",
            "description": "Return the execution plan for a loop config: waves, critical path, parallel fraction, chosen concurrency, and predicted speedup.",
            "inputSchema": {
                "type": "object",
                "properties": { "config_path": { "type": "string" } },
                "required": ["config_path"]
            }
        },
        {
            "name": "loopsmith_gate_evaluate",
            "description": "Run the deterministic gate against collected evidence and return per-check verdicts. This reports the gate's ruling; it cannot be used to set one.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config_path": { "type": "string" },
                    "target": { "type": "string", "description": "goal name or 'overall'" },
                    "workdir": { "type": "string" },
                    "metrics": { "type": "object" },
                    "artifacts": { "type": "object" }
                },
                "required": ["config_path", "target"]
            }
        },
        {
            "name": "loopsmith_ledger",
            "description": "Read the append-only ledger for a run: every dispatch, verdict, stop-gate trigger, and proposal.",
            "inputSchema": {
                "type": "object",
                "properties": { "run_id": { "type": "string" }, "limit": { "type": "integer" } },
                "required": ["run_id"]
            }
        },
        {
            "name": "loopsmith_goal_states",
            "description": "Current gate rulings for every goal plus 'overall' in a run.",
            "inputSchema": {
                "type": "object",
                "properties": { "run_id": { "type": "string" } },
                "required": ["run_id"]
            }
        },
        {
            "name": "loopsmith_record_episode",
            "description": "Record what a node produced. Rejected if it is missing provenance, because unattributed work cannot be judged for independence.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "iteration": { "type": "integer" },
                    "node_id": { "type": "string" },
                    "role": { "type": "string" },
                    "provider_id": { "type": "string" },
                    "output": { "type": "string" }
                },
                "required": ["run_id", "node_id", "provider_id", "output"]
            }
        },
        {
            "name": "loopsmith_scratchpad",
            "description": "Read or write the per-goal scratchpad that carries reasoning between iterations. Omit 'value' to read.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "run_id": { "type": "string" },
                    "key": { "type": "string" },
                    "value": { "type": "string" }
                },
                "required": ["run_id", "key"]
            }
        }
    ])
}

pub struct Server<S: Store> {
    pub store: S,
}

impl<S: Store> Server<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn handle(&self, req: &Request) -> Response {
        match req.method.as_str() {
            "initialize" => Response::ok(
                req.id.clone(),
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "loopsmith", "version": env!("CARGO_PKG_VERSION") }
                }),
            ),
            "notifications/initialized" => Response::ok(req.id.clone(), json!({})),
            "tools/list" => Response::ok(req.id.clone(), json!({ "tools": tools() })),
            "tools/call" => self.call(req),
            other => Response::err(req.id.clone(), -32601, &format!("unknown method `{other}`")),
        }
    }

    fn call(&self, req: &Request) -> Response {
        let name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let args = req
            .params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let result = match name {
            "loopsmith_plan" => self.tool_plan(&args),
            "loopsmith_gate_evaluate" => self.tool_gate(&args),
            "loopsmith_ledger" => self.tool_ledger(&args),
            "loopsmith_goal_states" => self.tool_goal_states(&args),
            "loopsmith_record_episode" => self.tool_record(&args),
            "loopsmith_scratchpad" => self.tool_scratchpad(&args),
            other => Err(format!("unknown tool `{other}`")),
        };

        match result {
            Ok(v) => Response::ok(
                req.id.clone(),
                json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
                    "isError": false
                }),
            ),
            Err(msg) => Response::ok(
                req.id.clone(),
                json!({
                    "content": [{ "type": "text", "text": msg }],
                    "isError": true
                }),
            ),
        }
    }

    fn tool_plan(&self, args: &Value) -> Result<Value, String> {
        let path = str_arg(args, "config_path")?;
        let cfg = loopsmith_core::load(&path).map_err(|e| e.to_string())?;
        let plan = loopsmith_graph::plan(&cfg.graph).map_err(|e| e.to_string())?;
        Ok(json!({
            "waves": plan.waves.iter().map(|w| json!({ "index": w.index, "nodes": w.nodes })).collect::<Vec<_>>(),
            "critical_path": plan.critical_path,
            "critical_path_cost": plan.critical_path_cost,
            "total_cost": plan.total_cost,
            "parallel_fraction": plan.parallel_fraction,
            "concurrency": plan.concurrency,
            "predicted_speedup": plan.predicted_speedup,
            "speedup_ceiling": plan.speedup_ceiling,
        }))
    }

    fn tool_gate(&self, args: &Value) -> Result<Value, String> {
        let path = str_arg(args, "config_path")?;
        let target = str_arg(args, "target")?;
        let workdir = args
            .get("workdir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let cfg = loopsmith_core::load(&path).map_err(|e| e.to_string())?;

        let mut ev = Evidence::new(&workdir);
        if let Some(m) = args.get("metrics").and_then(|v| v.as_object()) {
            for (k, v) in m {
                if let Some(f) = v.as_f64() {
                    ev.metrics.insert(k.clone(), f);
                }
            }
        }
        if let Some(a) = args.get("artifacts").and_then(|v| v.as_object()) {
            for (k, v) in a {
                if let Some(s) = v.as_str() {
                    ev.artifacts.insert(k.clone(), s.to_string());
                }
            }
        }
        let verdict: TargetVerdict = loopsmith_gate::evaluate(&cfg, &target, &ev);
        serde_json::to_value(verdict).map_err(|e| e.to_string())
    }

    fn tool_ledger(&self, args: &Value) -> Result<Value, String> {
        let run = str_arg(args, "run_id")?;
        let mut entries: Vec<LedgerEntry> =
            self.store.ledger(&run).map_err(|e| e.to_string())?;
        if let Some(limit) = args.get("limit").and_then(|v| v.as_u64()) {
            let n = entries.len().saturating_sub(limit as usize);
            entries = entries.split_off(n);
        }
        serde_json::to_value(entries).map_err(|e| e.to_string())
    }

    fn tool_goal_states(&self, args: &Value) -> Result<Value, String> {
        let run = str_arg(args, "run_id")?;
        let states = self.store.goal_states(&run).map_err(|e| e.to_string())?;
        serde_json::to_value(states).map_err(|e| e.to_string())
    }

    fn tool_record(&self, args: &Value) -> Result<Value, String> {
        let ep = Episode {
            run_id: str_arg(args, "run_id")?,
            iteration: args.get("iteration").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            node_id: str_arg(args, "node_id")?,
            role: args
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("builder")
                .to_string(),
            provider_id: str_arg(args, "provider_id")?,
            prompt_digest: String::new(),
            output: str_arg(args, "output")?,
            tokens: args.get("tokens").and_then(|v| v.as_u64()),
            cost_usd: args.get("cost_usd").and_then(|v| v.as_f64()),
            duration_ms: args.get("duration_ms").and_then(|v| v.as_u64()),
            error: None,
            created_ms: loopsmith_memory::now_ms(),
        };
        let seq = self.store.put_episode(&ep).map_err(|e| e.to_string())?;
        Ok(json!({ "recorded": true, "seq": seq }))
    }

    fn tool_scratchpad(&self, args: &Value) -> Result<Value, String> {
        let run = str_arg(args, "run_id")?;
        let key = str_arg(args, "key")?;
        match args.get("value").and_then(|v| v.as_str()) {
            Some(v) => {
                self.store
                    .set_scratchpad(&run, &key, v)
                    .map_err(|e| e.to_string())?;
                Ok(json!({ "written": true }))
            }
            None => {
                let v = self.store.scratchpad(&run, &key).map_err(|e| e.to_string())?;
                Ok(json!({ "value": v }))
            }
        }
    }

    /// Read newline-delimited JSON-RPC from `input`, write responses to
    /// `output`. Notifications (no `id`) get no reply, per the spec.
    pub fn serve(&self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let resp = match serde_json::from_str::<Request>(&line) {
                Ok(req) => {
                    let is_notification = req.id.is_none();
                    let r = self.handle(&req);
                    if is_notification {
                        continue;
                    }
                    r
                }
                Err(e) => Response::err(None, -32700, &format!("parse error: {e}")),
            };
            writeln!(output, "{}", serde_json::to_string(&resp)?)?;
            output.flush()?;
        }
        Ok(())
    }
}

fn str_arg(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing required argument `{key}`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use loopsmith_memory::SledStore;

    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn server() -> (Server<SledStore>, PathBuf) {
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "loopsmith-mcp-{}-{}-{n}",
            std::process::id(),
            loopsmith_memory::now_ms()
        ));
        (Server::new(SledStore::open(&dir).unwrap()), dir)
    }

    fn req(method: &str, params: Value) -> Request {
        Request {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: method.into(),
            params,
        }
    }

    fn call(name: &str, args: Value) -> Request {
        req("tools/call", json!({ "name": name, "arguments": args }))
    }

    fn text_of(r: &Response) -> String {
        r.result.as_ref().unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn is_error(r: &Response) -> bool {
        r.result.as_ref().unwrap()["isError"].as_bool().unwrap()
    }

    #[test]
    fn initialize_reports_protocol_and_name() {
        let (s, d) = server();
        let r = s.handle(&req("initialize", json!({})));
        let v = r.result.unwrap();
        assert_eq!(v["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(v["serverInfo"]["name"], "loopsmith");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn tools_list_is_non_empty_and_every_tool_has_a_schema() {
        let (s, d) = server();
        let r = s.handle(&req("tools/list", json!({})));
        let tools = r.result.unwrap()["tools"].clone();
        let arr = tools.as_array().unwrap();
        assert!(!arr.is_empty());
        for t in arr {
            assert!(t["name"].is_string());
            assert!(t["inputSchema"]["type"] == "object");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn there_is_no_tool_for_declaring_a_goal_satisfied() {
        // The gate owns that ruling. If this ever fails, the independence
        // guarantee has been quietly removed.
        let names: Vec<String> = tools()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(!names.iter().any(|n| n.contains("satisfy")
            || n.contains("set_goal")
            || n.contains("mark_done")));
    }

    #[test]
    fn unknown_method_is_a_jsonrpc_error() {
        let (s, d) = server();
        let r = s.handle(&req("nope", json!({})));
        assert!(r.error.is_some());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_missing_argument_is_reported_as_a_tool_error() {
        let (s, d) = server();
        let r = s.handle(&call("loopsmith_ledger", json!({})));
        assert!(is_error(&r));
        assert!(text_of(&r).contains("run_id"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn record_episode_then_read_it_back_through_the_ledger_tools() {
        let (s, d) = server();
        let r = s.handle(&call(
            "loopsmith_record_episode",
            json!({
                "run_id": "r1", "node_id": "n1",
                "provider_id": "p1", "output": "did it", "iteration": 2
            }),
        ));
        assert!(!is_error(&r), "{}", text_of(&r));
        assert_eq!(s.store.episodes("r1").unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn recording_without_provenance_is_refused() {
        let (s, d) = server();
        let r = s.handle(&call(
            "loopsmith_record_episode",
            json!({ "run_id": "r1", "node_id": "n1", "output": "x" }),
        ));
        assert!(is_error(&r));
        assert!(text_of(&r).contains("provider_id"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn scratchpad_writes_then_reads() {
        let (s, d) = server();
        let w = s.handle(&call(
            "loopsmith_scratchpad",
            json!({ "run_id": "r1", "key": "g1", "value": "notes" }),
        ));
        assert!(!is_error(&w));
        let r = s.handle(&call(
            "loopsmith_scratchpad",
            json!({ "run_id": "r1", "key": "g1" }),
        ));
        assert!(text_of(&r).contains("notes"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn serve_answers_requests_and_stays_silent_on_notifications() {
        let (s, d) = server();
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
            "\n"
        );
        let mut out = Vec::new();
        s.serve(std::io::Cursor::new(input), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 2, "notification must not get a reply: {text}");
        assert!(lines[0].contains("protocolVersion"));
        assert!(lines[1].contains("loopsmith_plan"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn malformed_json_gets_a_parse_error_rather_than_a_panic() {
        let (s, d) = server();
        let mut out = Vec::new();
        s.serve(std::io::Cursor::new("{not json\n"), &mut out).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("parse error"));
        let _ = std::fs::remove_dir_all(d);
    }
}
