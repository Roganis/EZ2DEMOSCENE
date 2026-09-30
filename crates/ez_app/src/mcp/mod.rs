//! A Model Context Protocol server on stdin/stdout (`ez2demoscene --mcp`):
//! lets an AI assistant browse the presets, write scenes, look at renders
//! of them and export loops.
//!
//! Messages are newline-delimited JSON-RPC. The server is dual-era: it
//! answers modern requests (protocol 2026-07-28, which carry their version
//! and capabilities in `_meta`, with `server/discover` for probing) and the
//! legacy `initialize` handshake (2025-11-25 and earlier). Requests are
//! served one at a time; nothing but protocol messages goes to stdout.

mod tools;

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// The modern revision this server implements.
const MODERN_VERSION: &str = "2026-07-28";
/// Legacy revisions answered through `initialize`, newest first.
const LEGACY_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

const INSTRUCTIONS: &str = "\
EZ2DEMOSCENE renders short, seamlessly looping 3D scenes in a demoscene style.

A scene is a project (the same JSON as an .ez2.json file). Everything animates \
as a function of the loop phase (0 at the start, 1 at the end, which equals \
the start), and every motion runs a whole number of cycles per loop, so the \
last frame leads straight back into the first. The loop lasts \
timing.loop_beats beats at timing.bpm.

Good workflow: list_presets, then get_scene on a preset close to what you \
want and edit that JSON (the presets are the best examples of the format; \
unlisted fields keep their defaults). scene_schema documents every type and \
field, with allowed values and defaults. Run check_scene on your edit, look at \
it with render_frame or preview_loop, and iterate. save_scene writes a \
project file the editor opens; export_loop renders the video.

Animatable values (Param) are a plain number, or an object such as \
{\"base\": 1.0, \"amp\": 0.5, \"wave\": \"Sine\", \"cycles\": 2} (cycles must \
be a whole number so the loop closes). For something that happens once, add \
\"ramp\": {\"start\": 0, \"length\": 2, \"by\": 1, \"ease\": \"Out\"}: the value \
changes by `by` from beat `start` of its timeline clip (of the loop outside a \
timeline) and then holds.

Text and Logo layers show numbers: write {0}, {1}... in the text and list \
them in \"values\" ({\"value\": Param, \"digits\", \"decimals\", \"group\"}), \
e.g. a timer counting down or a score counting up with a ramp.";

/// Serves on stdin/stdout until stdin closes.
pub fn serve() -> Result<()> {
    let mut server = Server::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_line(&line) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

struct Server {
    tools: tools::Tools,
}

/// A JSON-RPC error.
struct RpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> RpcError {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }
}

fn server_info() -> Value {
    json!({
        "name": "ez2demoscene",
        "title": "EZ2DEMOSCENE",
        "version": env!("CARGO_PKG_VERSION"),
    })
}

fn capabilities() -> Value {
    json!({ "tools": {} })
}

impl Server {
    fn new() -> Server {
        Server {
            tools: tools::Tools::new(),
        }
    }

    /// One incoming line; the reply to send, if any.
    fn handle_line(&mut self, line: &str) -> Option<String> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let err = RpcError::new(PARSE_ERROR, format!("parse error: {e}"));
                return Some(error_response(Value::Null, err).to_string());
            }
        };
        self.handle(msg).map(|v| v.to_string())
    }

    fn handle(&mut self, msg: Value) -> Option<Value> {
        let Some(obj) = msg.as_object() else {
            let err = RpcError::new(INVALID_REQUEST, "expected a JSON-RPC object");
            return Some(error_response(Value::Null, err));
        };
        let method = obj.get("method").and_then(Value::as_str);
        let Some(id) = obj.get("id").cloned() else {
            // A notification (initialized, cancelled…): nothing to answer.
            return None;
        };
        let Some(method) = method else {
            // A response: this server never sends requests, so ignore it.
            return None;
        };
        let params = obj.get("params").cloned().unwrap_or(json!({}));
        Some(match self.request(method, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => error_response(id, e),
        })
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        // A modern request names its protocol version in `_meta`.
        let modern = match params
            .get("_meta")
            .and_then(|m| m.get(META_VERSION))
            .and_then(Value::as_str)
        {
            Some(MODERN_VERSION) => true,
            Some(other) => {
                return Err(RpcError {
                    code: UNSUPPORTED_PROTOCOL_VERSION,
                    message: "Unsupported protocol version".into(),
                    data: Some(json!({
                        "supported": supported_versions(),
                        "requested": other,
                    })),
                })
            }
            None => false,
        };
        let mut result = match method {
            "initialize" => initialize(params),
            "server/discover" => Ok(json!({
                "supportedVersions": supported_versions(),
                "capabilities": capabilities(),
                "instructions": INSTRUCTIONS,
                "ttlMs": 0,
                "cacheScope": "public",
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": tools::Tools::list(),
                "ttlMs": 0,
                "cacheScope": "public",
            })),
            "tools/call" => self.call_tool(params),
            _ => Err(RpcError::new(
                METHOD_NOT_FOUND,
                format!("method not found: {method}"),
            )),
        }?;
        if modern {
            result["resultType"] = json!("complete");
            result["_meta"] = json!({ META_SERVER_INFO: server_info() });
        } else if let Some(r) = result.as_object_mut() {
            // Cache hints are modern-only.
            r.remove("ttlMs");
            r.remove("cacheScope");
        }
        Ok(result)
    }

    fn call_tool(&mut self, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "tools/call needs a tool name"))?;
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        if !args.is_object() {
            return Err(RpcError::new(INVALID_PARAMS, "arguments must be an object"));
        }
        if !tools::Tools::exists(name) {
            return Err(RpcError::new(
                INVALID_PARAMS,
                format!("unknown tool: {name}"),
            ));
        }
        Ok(self.tools.call(name, &args).into_json())
    }
}

fn supported_versions() -> Vec<&'static str> {
    std::iter::once(MODERN_VERSION)
        .chain(LEGACY_VERSIONS)
        .collect()
}

/// The legacy handshake: agree on the client's version when it is one we
/// speak, otherwise offer our newest legacy one.
fn initialize(params: &Value) -> Result<Value, RpcError> {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| LEGACY_VERSIONS.contains(v))
        .unwrap_or(LEGACY_VERSIONS[0]);
    Ok(json!({
        "protocolVersion": version,
        "capabilities": capabilities(),
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    }))
}

fn error_response(id: Value, e: RpcError) -> Value {
    let mut error = json!({ "code": e.code, "message": e.message });
    if let Some(data) = e.data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modern(method: &str, extra: Value) -> Value {
        let mut params = json!({
            "_meta": {
                META_VERSION: MODERN_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {},
            }
        });
        if let (Some(p), Some(e)) = (params.as_object_mut(), extra.as_object()) {
            p.extend(e.clone());
        }
        json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
    }

    fn ask(server: &mut Server, msg: Value) -> Value {
        server.handle(msg).expect("a reply")
    }

    #[test]
    fn legacy_handshake_agrees_on_a_version() {
        let mut s = Server::new();
        let r = ask(
            &mut s,
            json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "t", "version": "1"}}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"], "ez2demoscene");
        // An unknown version gets our newest legacy one.
        let r = ask(
            &mut s,
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2099-01-01"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], LEGACY_VERSIONS[0]);
        // Notifications get no reply.
        assert!(s
            .handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .is_none());
    }

    #[test]
    fn modern_discovery_and_versions() {
        let mut s = Server::new();
        let r = ask(&mut s, modern("server/discover", json!({})));
        let res = &r["result"];
        assert_eq!(res["resultType"], "complete");
        assert_eq!(res["supportedVersions"][0], MODERN_VERSION);
        assert!(res["capabilities"]["tools"].is_object());
        assert_eq!(res["_meta"][META_SERVER_INFO]["name"], "ez2demoscene");

        let mut bad = modern("tools/list", json!({}));
        bad["params"]["_meta"][META_VERSION] = json!("1900-01-01");
        let r = ask(&mut s, bad);
        assert_eq!(r["error"]["code"], UNSUPPORTED_PROTOCOL_VERSION);
        assert_eq!(r["error"]["data"]["requested"], "1900-01-01");
        assert_eq!(r["error"]["data"]["supported"][0], MODERN_VERSION);
    }

    #[test]
    fn lists_and_calls_tools() {
        let mut s = Server::new();
        let r = ask(&mut s, modern("tools/list", json!({})));
        let tools = r["result"]["tools"].as_array().unwrap();
        assert!(tools.len() >= 7);
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
            assert!(t["description"].as_str().unwrap().len() > 20);
        }
        let r = ask(
            &mut s,
            modern(
                "tools/call",
                json!({"name": "list_presets", "arguments": {}}),
            ),
        );
        assert_eq!(r["result"]["resultType"], "complete");
        assert_ne!(r["result"]["isError"], true);
        let presets = r["result"]["structuredContent"]["presets"]
            .as_array()
            .unwrap();
        assert_eq!(presets.len(), ez_core::presets::INDEX.len());

        // Legacy results carry no modern fields.
        let r = ask(
            &mut s,
            json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list", "params": {}}),
        );
        assert!(r["result"].get("ttlMs").is_none());
        assert!(r["result"].get("resultType").is_none());
    }

    #[test]
    fn protocol_errors() {
        let mut s = Server::new();
        let r = ask(&mut s, modern("nope/nothing", json!({})));
        assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
        let r = ask(
            &mut s,
            modern(
                "tools/call",
                json!({"name": "no_such_tool", "arguments": {}}),
            ),
        );
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
        let reply: Value = serde_json::from_str(&s.handle_line("{not json").unwrap()).unwrap();
        assert_eq!(reply["error"]["code"], PARSE_ERROR);
        assert_eq!(reply["id"], Value::Null);
    }
}
