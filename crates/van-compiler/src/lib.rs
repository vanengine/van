mod resolve;
pub mod render;

use std::collections::HashMap;

pub use render::PageAssets;
pub use resolve::ResolvedComponent;
pub use resolve::resolve_single;
pub use resolve::resolve_with_files;
pub use resolve::resolve_with_files_debug;

/// Compile a multi-file `.van` project into a full HTML page.
///
/// This is the main API: resolves imports from an in-memory file map,
/// then renders the result into a complete HTML page.
pub fn compile_page(
    entry_path: &str,
    files: &HashMap<String, String>,
    mock_data_json: &str,
) -> Result<String, String> {
    compile_page_with_debug(entry_path, files, mock_data_json, false, &HashMap::new())
}

/// Like `compile_page`, but with debug HTML comments at component/slot boundaries.
///
/// `file_origins` maps file paths to theme names for debug comment attribution.
pub fn compile_page_debug(
    entry_path: &str,
    files: &HashMap<String, String>,
    mock_data_json: &str,
    file_origins: &HashMap<String, String>,
) -> Result<String, String> {
    compile_page_with_debug(entry_path, files, mock_data_json, true, file_origins)
}

fn compile_page_with_debug(
    entry_path: &str,
    files: &HashMap<String, String>,
    mock_data_json: &str,
    debug: bool,
    file_origins: &HashMap<String, String>,
) -> Result<String, String> {
    let data: serde_json::Value = serde_json::from_str(mock_data_json)
        .map_err(|e| format!("Invalid JSON: {e}"))?;
    let resolved = if debug {
        resolve::resolve_with_files_debug(entry_path, files, &data, file_origins)?
    } else {
        resolve::resolve_with_files(entry_path, files, &data)?
    };
    render::render_page(&resolved, &data)
}

/// Compile a multi-file `.van` project with separated assets.
///
/// Like `compile_page`, but returns HTML + assets map instead of a single HTML string.
/// CSS/JS are returned as separate entries, HTML references them via `<link>`/`<script src>`.
pub fn compile_page_assets(
    entry_path: &str,
    files: &HashMap<String, String>,
    mock_data_json: &str,
    asset_prefix: &str,
) -> Result<PageAssets, String> {
    compile_page_assets_with_debug(entry_path, files, mock_data_json, asset_prefix, false, &HashMap::new())
}

/// Like `compile_page_assets`, but with debug HTML comments at component/slot boundaries.
///
/// `file_origins` maps file paths to theme names for debug comment attribution.
pub fn compile_page_assets_debug(
    entry_path: &str,
    files: &HashMap<String, String>,
    mock_data_json: &str,
    asset_prefix: &str,
    file_origins: &HashMap<String, String>,
) -> Result<PageAssets, String> {
    compile_page_assets_with_debug(entry_path, files, mock_data_json, asset_prefix, true, file_origins)
}

fn compile_page_assets_with_debug(
    entry_path: &str,
    files: &HashMap<String, String>,
    mock_data_json: &str,
    asset_prefix: &str,
    debug: bool,
    file_origins: &HashMap<String, String>,
) -> Result<PageAssets, String> {
    let data: serde_json::Value = serde_json::from_str(mock_data_json)
        .map_err(|e| format!("Invalid JSON: {e}"))?;
    let resolved = if debug {
        resolve::resolve_with_files_debug(entry_path, files, &data, file_origins)?
    } else {
        resolve::resolve_with_files(entry_path, files, &data)?
    };

    // Derive page name from entry path: "pages/index.van" → "pages/index"
    let page_name = entry_path.trim_end_matches(".van");

    render::render_page_assets(&resolved, &data, page_name, asset_prefix)
}

/// Compile a single `.van` file source into a full HTML page.
///
/// Convenience wrapper: wraps the source into a single-file map and calls `compile_page`.
pub fn compile_single(source: &str, mock_data_json: &str) -> Result<String, String> {
    let mut files = HashMap::new();
    files.insert("main.van".to_string(), source.to_string());
    compile_page("main.van", &files, mock_data_json)
}

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn compile_van(
    entry_path: &str,
    files_json: &str,
    mock_data_json: &str,
) -> Result<String, JsValue> {
    // Parse files_json: {"filename": "content", ...}
    let files_value: serde_json::Value = serde_json::from_str(files_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid files JSON: {e}")))?;

    let files_obj = files_value
        .as_object()
        .ok_or_else(|| JsValue::from_str("files_json must be a JSON object"))?;

    let mut files = HashMap::new();
    for (key, val) in files_obj {
        let content = val
            .as_str()
            .ok_or_else(|| JsValue::from_str(&format!("File '{}' content must be a string", key)))?;
        files.insert(key.clone(), content.to_string());
    }

    compile_page(entry_path, &files, mock_data_json).map_err(|e| JsValue::from_str(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_single_basic() {
        let source = r#"
<template>
  <h1>{{ title }}</h1>
</template>
"#;
        let mock = r#"{"title": "Hello World"}"#;
        let html = compile_single(source, mock).unwrap();
        assert!(html.contains("<h1>Hello World</h1>"));
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_compile_single_with_signals() {
        let source = r#"
<template>
  <div>
    <p>Count: {{ count }}</p>
    <button @click="increment">+1</button>
  </div>
</template>

<script setup>
const count = ref(0)
function increment() { count.value++ }
</script>
"#;
        let mock = r#"{}"#;
        let html = compile_single(source, mock).unwrap();
        assert!(html.contains("Van"));
        assert!(!html.contains("@click"));
        assert!(html.contains("effect"));
    }

    #[test]
    fn test_compile_page_invalid_json() {
        let mut files = HashMap::new();
        files.insert("main.van".to_string(), "<template><p>Hi</p></template>".to_string());
        let result = compile_page("main.van", &files, "not json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid JSON"));
    }

    #[test]
    fn test_compile_page_with_style() {
        let source = r#"
<template>
  <h1>Hello</h1>
</template>

<style>
h1 { color: blue; }
</style>
"#;
        let html = compile_single(source, "{}").unwrap();
        assert!(html.contains("color: blue"));
    }

    #[test]
    fn test_compile_page_multi_file() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <hello :name="title" />
</template>

<script setup>
import Hello from './hello.van'
</script>
"#
            .to_string(),
        );
        files.insert(
            "hello.van".to_string(),
            r#"
<template>
  <h1>Hello, {{ name }}!</h1>
</template>

<style>
h1 { color: green; }
</style>
"#
            .to_string(),
        );

        let mock = r#"{"title": "Van"}"#;
        let html = compile_page("index.van", &files, mock).unwrap();
        assert!(html.contains("<h1>Hello, Van!</h1>"));
        assert!(html.contains("color: green"));
    }

    #[test]
    fn test_compile_page_with_ts_import() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <div>
    <p>Count: {{ count }}</p>
    <button @click="increment">+1</button>
  </div>
</template>

<script setup lang="ts">
import { formatDate } from './utils/format.ts'
import type { User } from './types.ts'
const count = ref(0)
function increment() { count.value++ }
</script>
"#
            .to_string(),
        );
        files.insert(
            "utils/format.ts".to_string(),
            r#"function formatDate(d) { return d.toISOString(); }
return { formatDate: formatDate };"#
                .to_string(),
        );
        // types.ts is NOT in files map — type-only imports are erased, so it's fine

        let mock = r#"{}"#;
        let html = compile_page("index.van", &files, mock).unwrap();
        // Should contain the inlined module code
        assert!(html.contains("__mod_0"));
        assert!(html.contains("formatDate"));
        // Should still have signal code
        assert!(html.contains("Van"));
        assert!(html.contains("effect"));
        // Type-only import should be erased (no __mod_1)
        assert!(!html.contains("__mod_1"));
    }

    #[test]
    fn test_compile_page_type_only_import_erased() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <div><p>{{ count }}</p></div>
</template>

<script setup lang="ts">
import type { Config } from './config.ts'
const count = ref(0)
</script>
"#
            .to_string(),
        );

        let mock = r#"{}"#;
        let html = compile_page("index.van", &files, mock).unwrap();
        // No module should be inlined (type-only)
        assert!(!html.contains("__mod_"));
        // Signal code still works
        assert!(html.contains("V.signal(0)"));
    }
}
