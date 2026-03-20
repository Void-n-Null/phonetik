//! MCP (Model Context Protocol) stdio transport.
//!
//! Exposes phonetik's analysis as tools that AI assistants can call
//! directly. Speaks JSON-RPC 2.0 over stdin/stdout — no network, no
//! auth, no runtime dependencies beyond serde_json (already required
//! by the library).
//!
//! Start with `phonetik-mcp` and point your MCP client at it:
//!
//! ```json
//! { "phonetik": { "command": "phonetik-mcp" } }
//! ```

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{DocumentAnalyzeOptions, Phonetik};

// ── JSON-RPC types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ── Tool definitions ────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!([
        {
            "name": "lookup",
            "description": "Look up phonetic information for an English word: phonemes, syllable count, stress pattern, and pronunciation variants.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "word": { "type": "string", "description": "The word to look up." }
                },
                "required": ["word"]
            }
        },
        {
            "name": "rhymes",
            "description": "Find words that rhyme with the given word. Returns perfect, slant, and near rhymes sorted by quality.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "word": { "type": "string", "description": "The word to find rhymes for." },
                    "limit": { "type": "integer", "description": "Maximum results (default 20, max 500).", "default": 20 }
                },
                "required": ["word"]
            }
        },
        {
            "name": "scan",
            "description": "Analyze the meter and stress pattern of a line of text. Identifies iambic pentameter, trochaic tetrameter, etc.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "line": { "type": "string", "description": "A line of verse or prose to scan." }
                },
                "required": ["line"]
            }
        },
        {
            "name": "compare",
            "description": "Compare two words phonetically. Returns similarity score, rhyme type (perfect/slant/near/none), and confidence.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "word1": { "type": "string", "description": "First word." },
                    "word2": { "type": "string", "description": "Second word." }
                },
                "required": ["word1", "word2"]
            }
        },
        {
            "name": "analyze_document",
            "description": "Full prosody analysis of multiple lines: per-line scansion, syllable counts, meter detection, dictionary coverage, dominant meter, and optional rhyme map. Designed to generate metadata that AI can use for lyric/poetry analysis and generation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "lines": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Lines of text to analyze."
                    },
                    "includeRhymeMap": {
                        "type": "boolean",
                        "description": "Include phoneme repetition pattern analysis across lines (default false).",
                        "default": false
                    }
                },
                "required": ["lines"]
            }
        }
    ])
}

// ── Tool dispatch ───────────────────────────────────────────────────────

fn call_tool(engine: &Phonetik, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "lookup" => {
            let word = args["word"]
                .as_str()
                .ok_or("Missing required parameter: word")?;
            engine
                .lookup(word)
                .map(|info| serde_json::to_value(info).unwrap())
                .ok_or_else(|| format!("Word not found in dictionary: {word}"))
        }
        "rhymes" => {
            let word = args["word"]
                .as_str()
                .ok_or("Missing required parameter: word")?;
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let matches = engine.rhymes(word, limit);
            Ok(serde_json::to_value(matches).unwrap())
        }
        "scan" => {
            let line = args["line"]
                .as_str()
                .ok_or("Missing required parameter: line")?;
            let result = engine.scan(line);
            Ok(serde_json::to_value(result).unwrap())
        }
        "compare" => {
            let w1 = args["word1"]
                .as_str()
                .ok_or("Missing required parameter: word1")?;
            let w2 = args["word2"]
                .as_str()
                .ok_or("Missing required parameter: word2")?;
            engine
                .compare(w1, w2)
                .map(|cmp| serde_json::to_value(cmp).unwrap())
                .ok_or_else(|| "One or both words not found in dictionary.".into())
        }
        "analyze_document" => {
            let lines: Vec<&str> = args["lines"]
                .as_array()
                .ok_or("Missing required parameter: lines")?
                .iter()
                .filter_map(|v| v.as_str())
                .collect();
            if lines.len() > 100 {
                return Err("Maximum 100 lines per request.".into());
            }
            let include_rhyme_map = args["includeRhymeMap"].as_bool().unwrap_or(false);
            let opts = DocumentAnalyzeOptions {
                stress_mode: None,
                include_rhyme_map,
            };
            let result = engine.analyze_document(&lines, &opts);
            Ok(serde_json::to_value(result).unwrap())
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

// ── Protocol handler ────────────────────────────────────────────────────

fn handle_request(engine: &Phonetik, req: &RpcRequest) -> Option<RpcResponse> {
    match req.method.as_str() {
        "initialize" => Some(RpcResponse::success(
            req.id.clone(),
            json!({
                "protocolVersion": "2025-03-26",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "phonetik",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }),
        )),

        // Notification — no response.
        "notifications/initialized" | "notifications/cancelled" => None,

        "tools/list" => Some(RpcResponse::success(
            req.id.clone(),
            json!({ "tools": tool_definitions() }),
        )),

        "tools/call" => {
            let name = req.params["name"].as_str().unwrap_or("");
            let args = &req.params["arguments"];

            match call_tool(engine, name, args) {
                Ok(value) => {
                    let text = serde_json::to_string_pretty(&value).unwrap();
                    Some(RpcResponse::success(
                        req.id.clone(),
                        json!({
                            "content": [{ "type": "text", "text": text }]
                        }),
                    ))
                }
                Err(msg) => Some(RpcResponse::success(
                    req.id.clone(),
                    json!({
                        "content": [{ "type": "text", "text": msg }],
                        "isError": true
                    }),
                )),
            }
        }

        _ => {
            // Unknown method — return error if it has an id (request),
            // silently ignore if it's a notification.
            req.id.as_ref().map(|_| {
                RpcResponse::error(
                    req.id.clone(),
                    -32601,
                    format!("Method not found: {}", req.method),
                )
            })
        }
    }
}

// ── Main loop ───────────────────────────────────────────────────────────

/// Process a single JSON-RPC line and return the response JSON (if any).
/// Exposed for testing — production callers should use [`run_stdio`].
pub fn handle_line(engine: &Phonetik, line: &str) -> Option<String> {
    let req: RpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            let resp = RpcResponse::error(None, -32700, format!("Parse error: {e}"));
            return Some(serde_json::to_string(&resp).unwrap());
        }
    };
    handle_request(engine, &req).map(|resp| serde_json::to_string(&resp).unwrap())
}

/// Run the MCP stdio transport. Blocks until stdin closes.
pub fn run_stdio(engine: Phonetik) {
    let stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    for line in stdin.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        if let Some(resp) = handle_line(&engine, &line) {
            let _ = writeln!(stdout, "{resp}");
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn engine() -> Phonetik {
        Phonetik::new()
    }

    /// Send a JSON-RPC request and parse the response.
    fn rpc(engine: &Phonetik, request: Value) -> Value {
        let line = serde_json::to_string(&request).unwrap();
        let resp = handle_line(engine, &line).expect("expected a response");
        serde_json::from_str(&resp).unwrap()
    }

    /// Extract the text content from a tools/call result.
    fn tool_text(resp: &Value) -> String {
        resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// Parse the text content of a tools/call result as JSON.
    fn tool_json(resp: &Value) -> Value {
        serde_json::from_str(&tool_text(resp)).unwrap()
    }

    // ── Protocol ────────────────────────────────────────────────────

    #[test]
    fn initialize_returns_capabilities() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-03-26",
                "capabilities":{},
                "clientInfo":{"name":"test","version":"1.0"}
            }}),
        );
        assert_eq!(resp["result"]["serverInfo"]["name"], "phonetik");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
        assert!(resp["error"].is_null());
    }

    #[test]
    fn notifications_return_no_response() {
        let e = engine();
        let line = json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string();
        assert!(handle_line(&e, &line).is_none());

        let line = json!({"jsonrpc":"2.0","method":"notifications/cancelled"}).to_string();
        assert!(handle_line(&e, &line).is_none());
    }

    #[test]
    fn tools_list_returns_all_tools() {
        let e = engine();
        let resp = rpc(&e, json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"lookup"));
        assert!(names.contains(&"rhymes"));
        assert!(names.contains(&"scan"));
        assert!(names.contains(&"compare"));
        assert!(names.contains(&"analyze_document"));
    }

    #[test]
    fn unknown_method_returns_error() {
        let e = engine();
        let resp = rpc(&e, json!({"jsonrpc":"2.0","id":99,"method":"bogus/method"}));
        assert_eq!(resp["error"]["code"], -32601);
        assert!(resp["result"].is_null());
    }

    #[test]
    fn unknown_method_notification_is_silent() {
        let e = engine();
        let line = json!({"jsonrpc":"2.0","method":"bogus/notification"}).to_string();
        assert!(handle_line(&e, &line).is_none());
    }

    #[test]
    fn parse_error_returns_error() {
        let e = engine();
        let resp_str = handle_line(&e, "this is not json").unwrap();
        let resp: Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["error"]["code"], -32700);
    }

    #[test]
    fn empty_lines_are_skipped_by_run_stdio() {
        // handle_line itself doesn't skip empties — run_stdio does.
        // But handle_line on whitespace should return a parse error, not panic.
        let e = engine();
        let resp_str = handle_line(&e, "   ").unwrap();
        let resp: Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["error"]["code"], -32700);
    }

    // ── Tool: lookup ────────────────────────────────────────────────

    #[test]
    fn lookup_known_word() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"lookup","arguments":{"word":"hello"}
            }}),
        );
        let data = tool_json(&resp);
        assert_eq!(data["word"], "HELLO");
        assert_eq!(data["syllableCount"], 2);
        assert!(resp["result"]["isError"].is_null());
    }

    #[test]
    fn lookup_unknown_word_returns_tool_error() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"lookup","arguments":{"word":"xyzzyplugh"}
            }}),
        );
        assert_eq!(resp["result"]["isError"], true);
        assert!(tool_text(&resp).contains("not found"));
    }

    #[test]
    fn lookup_missing_word_param() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"lookup","arguments":{}
            }}),
        );
        assert_eq!(resp["result"]["isError"], true);
        assert!(tool_text(&resp).contains("Missing"));
    }

    // ── Tool: rhymes ────────────────────────────────────────────────

    #[test]
    fn rhymes_returns_results() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"rhymes","arguments":{"word":"cat","limit":5}
            }}),
        );
        let data = tool_json(&resp);
        let arr = data.as_array().unwrap();
        assert!(!arr.is_empty());
        assert!(arr.len() <= 5);
        assert_eq!(arr[0]["rhymeType"], "perfect");
    }

    #[test]
    fn rhymes_uses_default_limit() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"rhymes","arguments":{"word":"the"}
            }}),
        );
        let data = tool_json(&resp);
        assert!(data.as_array().unwrap().len() <= 20);
    }

    // ── Tool: scan ──────────────────────────────────────────────────

    #[test]
    fn scan_detects_meter() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"scan","arguments":{"line":"uneasy lies the head that wears the crown"}
            }}),
        );
        let data = tool_json(&resp);
        assert_eq!(data["syllableCount"], 10);
        assert!(data["meter"]["name"].as_str().unwrap().contains("iambic"));
        assert_eq!(data["visual"], "x / x / x / x / x /");
    }

    // ── Tool: compare ───────────────────────────────────────────────

    #[test]
    fn compare_rhyming_pair() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"compare","arguments":{"word1":"cat","word2":"bat"}
            }}),
        );
        let data = tool_json(&resp);
        assert_eq!(data["rhymeType"], "perfect");
        assert!(data["similarity"].as_f64().unwrap() > 0.5);
    }

    #[test]
    fn compare_unknown_word() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"compare","arguments":{"word1":"cat","word2":"xyzzyplugh"}
            }}),
        );
        assert_eq!(resp["result"]["isError"], true);
    }

    // ── Tool: analyze_document ──────────────────────────────────────

    #[test]
    fn analyze_document_returns_summary() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"analyze_document","arguments":{
                    "lines":["shall I compare thee to a summer's day","thou art more lovely and more temperate"]
                }
            }}),
        );
        let data = tool_json(&resp);
        assert_eq!(data["summary"]["lineCount"], 2);
        assert!(data["summary"]["totalSyllables"].as_u64().unwrap() > 0);
        assert!(data["rhymeMap"].is_null());
    }

    #[test]
    fn analyze_document_with_rhyme_map() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"analyze_document","arguments":{
                    "lines":["the cat sat on the mat","the bat sat on the hat"],
                    "includeRhymeMap":true
                }
            }}),
        );
        let data = tool_json(&resp);
        assert!(!data["rhymeMap"].is_null());
    }

    #[test]
    fn analyze_document_too_many_lines() {
        let e = engine();
        let lines: Vec<String> = (0..101).map(|i| format!("line {i}")).collect();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"analyze_document","arguments":{"lines":lines}
            }}),
        );
        assert_eq!(resp["result"]["isError"], true);
        assert!(tool_text(&resp).contains("100"));
    }

    // ── Tool: unknown ───────────────────────────────────────────────

    #[test]
    fn unknown_tool_returns_error() {
        let e = engine();
        let resp = rpc(
            &e,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
                "name":"bogus_tool","arguments":{}
            }}),
        );
        assert_eq!(resp["result"]["isError"], true);
        assert!(tool_text(&resp).contains("Unknown tool"));
    }
}
