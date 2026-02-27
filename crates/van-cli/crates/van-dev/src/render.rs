use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use van_parser::PropDef;

const CLIENT_JS: &str = include_str!("client.js");

/// Insert `content` before a closing tag (e.g. `</head>`, `</body>`),
/// with indentation matching the surrounding HTML structure.
fn inject_before_close(html: &mut String, close_tag: &str, content: &str) {
    if content.is_empty() {
        return;
    }
    if let Some(pos) = html.find(close_tag) {
        let before = &html[..pos];
        let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_prefix = &before[line_start..];
        let indent_len = line_prefix.len() - line_prefix.trim_start().len();
        let child_indent = format!("{}  ", &line_prefix[..indent_len]);
        let mut injection = String::new();
        for line in content.lines() {
            injection.push_str(&child_indent);
            injection.push_str(line);
            injection.push('\n');
        }
        html.insert_str(line_start, &injection);
    }
}

/// Render a page from pre-collected files with live reload client and debug comments.
///
/// Delegates compilation to `van_compiler`, then injects the WebSocket-based
/// live reload `client.js` before `</body>`.
pub fn render_from_files(
    entry_path: &str,
    files: &HashMap<String, String>,
    data: &Value,
    file_origins: &HashMap<String, String>,
) -> Result<String> {
    let data_json = serde_json::to_string(data)?;
    let mut html =
        van_compiler::compile_page_debug(entry_path, files, &data_json, file_origins)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

    let client_script = format!("<script>{CLIENT_JS}</script>");
    inject_before_close(&mut html, "</body>", &client_script);
    Ok(html)
}

/// Render a page from pre-collected files for static output (no live reload).
pub fn render_static_from_files(
    entry_path: &str,
    files: &HashMap<String, String>,
    data: &Value,
) -> Result<String> {
    let data_json = serde_json::to_string(data)?;
    van_compiler::compile_page(entry_path, files, &data_json)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Validate data against `defineProps` declarations.
///
/// Prints warnings to stderr for:
/// - Missing required props
/// - Extra keys not declared in defineProps
/// - Type mismatches (String vs Number vs Boolean vs Array vs Object)
///
/// Never blocks rendering -- warnings only.
pub(crate) fn validate_data(props: &[PropDef], data: &Value, page_label: &str) {
    let yellow = "\x1b[33m";
    let reset = "\x1b[0m";

    let map = match data.as_object() {
        Some(m) => m,
        None => return,
    };

    // Check for missing required props
    for prop in props {
        if prop.required && !map.contains_key(&prop.name) {
            let type_hint = prop.prop_type.as_deref().unwrap_or("any");
            eprintln!(
                "{yellow}  \u{26a0} {page_label}: missing required prop \"{}\" ({type_hint}){reset}",
                prop.name
            );
        }
    }

    // Check for extra keys not in defineProps
    let prop_names: std::collections::HashSet<&str> =
        props.iter().map(|p| p.name.as_str()).collect();
    for key in map.keys() {
        if !prop_names.contains(key.as_str()) {
            eprintln!(
                "{yellow}  \u{26a0} {page_label}: extra data key \"{key}\" not in defineProps{reset}"
            );
        }
    }

    // Check type mismatches
    for prop in props {
        let Some(ref expected_type) = prop.prop_type else {
            continue;
        };
        let Some(value) = map.get(&prop.name) else {
            continue;
        };
        let actual_type = json_value_type_name(value);
        let expected_lower = expected_type.to_lowercase();
        if actual_type != expected_lower {
            eprintln!(
                "{yellow}  \u{26a0} {page_label}: prop \"{}\" expects {expected_type}, got {actual_type}{reset}",
                prop.name
            );
        }
    }
}

/// Map a serde_json::Value to a lowercase type name matching Vue prop types.
fn json_value_type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_render_from_files_basic() {
        let source = r#"
<template>
  <h1>{{ title }}</h1>
</template>

<style scoped>
h1 { color: red; }
</style>
"#;
        let mut files = HashMap::new();
        files.insert("pages/index.van".to_string(), source.to_string());
        let data = json!({"title": "Hello"});
        let html =
            render_from_files("pages/index.van", &files, &data, &HashMap::new()).unwrap();
        assert!(html.contains("Hello"), "Should contain interpolated title");
        assert!(html.contains("color: red"), "Should contain scoped CSS");
        assert!(html.contains("__van/ws"), "Should contain live reload client");
    }

    #[test]
    fn test_render_static_from_files() {
        let source = r#"
<template>
  <h1>{{ title }}</h1>
</template>
"#;
        let mut files = HashMap::new();
        files.insert("pages/index.van".to_string(), source.to_string());
        let data = json!({"title": "World"});
        let html = render_static_from_files("pages/index.van", &files, &data).unwrap();
        assert!(html.contains("World"));
        assert!(!html.contains("__van/ws"), "Static output should not have live reload");
    }

    // --- validate_data tests ---

    #[test]
    fn test_validate_all_good() {
        let props = vec![
            PropDef { name: "title".into(), prop_type: Some("String".into()), required: true },
            PropDef { name: "count".into(), prop_type: Some("Number".into()), required: false },
        ];
        let data = json!({"title": "Hello", "count": 42});
        // Should produce no warnings (no panic)
        validate_data(&props, &data, "pages/index.van");
    }

    #[test]
    fn test_validate_missing_required() {
        let props = vec![
            PropDef { name: "user".into(), prop_type: Some("Object".into()), required: true },
        ];
        let data = json!({});
        validate_data(&props, &data, "pages/index.van");
    }

    #[test]
    fn test_validate_extra_keys() {
        let props = vec![
            PropDef { name: "title".into(), prop_type: Some("String".into()), required: false },
        ];
        let data = json!({"title": "Hi", "typo": "oops"});
        validate_data(&props, &data, "pages/index.van");
    }

    #[test]
    fn test_validate_type_mismatch() {
        let props = vec![
            PropDef { name: "count".into(), prop_type: Some("Number".into()), required: false },
        ];
        let data = json!({"count": "not a number"});
        validate_data(&props, &data, "pages/index.van");
    }

    #[test]
    fn test_json_value_type_name() {
        assert_eq!(json_value_type_name(&json!("hello")), "string");
        assert_eq!(json_value_type_name(&json!(42)), "number");
        assert_eq!(json_value_type_name(&json!(true)), "boolean");
        assert_eq!(json_value_type_name(&json!([1, 2])), "array");
        assert_eq!(json_value_type_name(&json!({"a": 1})), "object");
        assert_eq!(json_value_type_name(&json!(null)), "null");
    }
}
