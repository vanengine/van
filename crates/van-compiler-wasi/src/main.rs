use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CompileRequest {
    entry_path: String,
    files: HashMap<String, String>,
    mock_data_json: String,
    #[serde(default)]
    asset_prefix: Option<String>,
    #[serde(default)]
    debug: bool,
    /// Maps file paths to theme names (e.g. "components/header.van" → "van1").
    #[serde(default)]
    file_origins: HashMap<String, String>,
}

#[derive(Serialize)]
struct CompileResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    html: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assets: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn compile(req: CompileRequest) -> CompileResponse {
    if let Some(ref prefix) = req.asset_prefix {
        let compile_result = if req.debug {
            van_compiler::compile_page_assets_debug(
                &req.entry_path,
                &req.files,
                &req.mock_data_json,
                prefix,
                &req.file_origins,
            )
        } else {
            van_compiler::compile_page_assets(
                &req.entry_path,
                &req.files,
                &req.mock_data_json,
                prefix,
            )
        };
        match compile_result {
            Ok(result) => CompileResponse {
                ok: true,
                html: Some(result.html),
                assets: Some(result.assets),
                error: None,
            },
            Err(e) => CompileResponse {
                ok: false,
                html: None,
                assets: None,
                error: Some(e),
            },
        }
    } else {
        let compile_result = if req.debug {
            van_compiler::compile_page_debug(
                &req.entry_path,
                &req.files,
                &req.mock_data_json,
                &req.file_origins,
            )
        } else {
            van_compiler::compile_page(&req.entry_path, &req.files, &req.mock_data_json)
        };
        match compile_result {
            Ok(html) => CompileResponse {
                ok: true,
                html: Some(html),
                assets: None,
                error: None,
            },
            Err(e) => CompileResponse {
                ok: false,
                html: None,
                assets: None,
                error: Some(e),
            },
        }
    }
}

fn write_response(resp: &CompileResponse) {
    let out = serde_json::to_string(resp).unwrap();
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(out.as_bytes()).unwrap();
    handle.write_all(b"\n").unwrap();
    handle.flush().unwrap();
}

fn main() {
    let daemon = std::env::args().any(|a| a == "--daemon");

    if daemon {
        // Daemon mode: read one JSON object per line (JSON Lines), compile, respond.
        // Exits when stdin reaches EOF.
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            let resp = match serde_json::from_str::<CompileRequest>(&line) {
                Ok(req) => compile(req),
                Err(e) => CompileResponse {
                    ok: false,
                    html: None,
                    assets: None,
                    error: Some(e.to_string()),
                },
            };
            write_response(&resp);
        }
    } else {
        // Single-shot mode: read all of stdin, compile once, write response.
        let mut input = String::new();
        io::stdin().read_to_string(&mut input).unwrap();

        let resp = match serde_json::from_str::<CompileRequest>(&input) {
            Ok(req) => compile(req),
            Err(e) => CompileResponse {
                ok: false,
                html: None,
                assets: None,
                error: Some(e.to_string()),
            },
        };
        write_response(&resp);
    }
}
