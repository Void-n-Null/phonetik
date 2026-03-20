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

        let req: RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = RpcResponse::error(None, -32700, format!("Parse error: {e}"));
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                let _ = stdout.flush();
                continue;
            }
        };

        if let Some(resp) = handle_request(&engine, &req) {
            let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
            let _ = stdout.flush();
        }
    }
}
