use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use regex::Regex;
use serde_json::Value;
use van_signal_gen::{extract_initial_values, generate_signals, RUNTIME_JS};

use crate::resolve::ResolvedComponent;

/// Compute a short content hash (8 hex chars) for cache busting.
fn content_hash(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

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

/// Augment data with initial signal values from `<script setup>`.
///
/// This allows `cleanup_html()` to replace reactive `{{ count }}` with `0`
/// instead of leaving raw mustache tags in the output (bad for SEO).
fn augment_data_with_signals(data: &Value, script_setup: Option<&str>) -> Value {
    let Some(script) = script_setup else {
        return data.clone();
    };
    let initial_values = extract_initial_values(script);
    if initial_values.is_empty() {
        return data.clone();
    }
    let mut augmented = data.clone();
    if let Value::Object(ref mut map) = augmented {
        for (name, value) in initial_values {
            // Don't override existing data (server data takes priority)
            if !map.contains_key(&name) {
                map.insert(name, Value::String(value));
            }
        }
    }
    augmented
}

/// Result of compiling a `.van` page with separated assets.
pub struct PageAssets {
    /// HTML with external `<link>`/`<script src>` references (no inline CSS/JS)
    pub html: String,
    /// Asset path → content (e.g. "/themes/van1/assets/js/pages/index.js" → "var Van=...")
    pub assets: HashMap<String, String>,
}

/// Render a resolved `.van` component into a full HTML page.
///
/// Pipeline:
/// 1. `resolve_single()` → "dirty" HTML (still has @click, v-show, {{ reactive }})
/// 2. `generate_signals()` → positional signal JS from the dirty HTML
/// 3. `cleanup_html()` → strip directives, interpolate remaining {{ }}, producing clean HTML
/// 4. Inject styles + scripts into clean HTML
///
/// Unlike `van-dev-server`'s render, this does NOT inject `client.js` (WebSocket live reload).
pub fn render_page(resolved: &ResolvedComponent, data: &Value) -> Result<String, String> {
    let style_block: String = resolved
        .styles
        .iter()
        .map(|css| format!("<style>{css}</style>"))
        .collect::<Vec<_>>()
        .join("\n");

    // Collect module code for signal generation (skip type-only)
    let module_code: Vec<String> = resolved
        .module_imports
        .iter()
        .filter(|m| !m.is_type_only)
        .map(|m| m.content.clone())
        .collect();

    // Generate signal JS from the dirty HTML (before cleanup)
    let signal_scripts = if let Some(ref script_setup) = resolved.script_setup {
        if let Some(signal_js) = generate_signals(script_setup, &resolved.html, &module_code) {
            format!(
                "<script>{RUNTIME_JS}</script>\n<script>{signal_js}</script>"
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Augment data with signal initial values for SSR (e.g. {{ count }} → 0)
    let augmented_data =
        augment_data_with_signals(data, resolved.script_setup.as_deref());

    // Clean up HTML: strip directives, interpolate remaining {{ }}
    let clean_html = cleanup_html(&resolved.html, &augmented_data);

    if clean_html.contains("<html") {
        // Layout mode: inject styles before </head> and scripts before </body>
        let mut html = clean_html;
        inject_before_close(&mut html, "</head>", &style_block);
        inject_before_close(&mut html, "</body>", &signal_scripts);
        Ok(html)
    } else {
        // Default HTML shell
        let html = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>Van Playground</title>
{style_block}
</head>
<body>
{clean_html}
{signal_scripts}
</body>
</html>"#
        );

        Ok(html)
    }
}

/// Render a resolved `.van` component with separated assets.
///
/// Instead of inlining CSS/JS into the HTML, returns them as separate entries
/// in the `assets` map, with HTML referencing them via `<link>` / `<script src>`.
///
/// `asset_prefix` determines the URL path prefix for assets,
/// e.g. "/themes/van1/assets" → produces "/themes/van1/assets/css/page.css".
pub fn render_page_assets(
    resolved: &ResolvedComponent,
    data: &Value,
    page_name: &str,
    asset_prefix: &str,
) -> Result<PageAssets, String> {
    let mut assets = HashMap::new();

    // CSS asset (with content hash for cache busting)
    let css_ref = if !resolved.styles.is_empty() {
        let css_content: String = resolved.styles.join("\n");
        let hash = content_hash(&css_content);
        let css_path = format!("{}/css/{}.{}.css", asset_prefix, page_name, hash);
        assets.insert(css_path.clone(), css_content);
        format!(r#"<link rel="stylesheet" href="{css_path}">"#)
    } else {
        String::new()
    };

    // Collect module code for signal generation (skip type-only)
    let module_code: Vec<String> = resolved
        .module_imports
        .iter()
        .filter(|m| !m.is_type_only)
        .map(|m| m.content.clone())
        .collect();

    // Generate signal JS from the dirty HTML (before cleanup)
    let js_ref = if let Some(ref script_setup) = resolved.script_setup {
        if let Some(signal_js) = generate_signals(script_setup, &resolved.html, &module_code) {
            let runtime_hash = content_hash(RUNTIME_JS);
            let runtime_path = format!("{}/js/van-runtime.{}.js", asset_prefix, runtime_hash);
            let js_hash = content_hash(&signal_js);
            let js_path = format!("{}/js/{}.{}.js", asset_prefix, page_name, js_hash);
            assets.insert(runtime_path.clone(), RUNTIME_JS.to_string());
            assets.insert(js_path.clone(), signal_js);
            format!(
                r#"<script src="{runtime_path}"></script>
<script src="{js_path}"></script>"#
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Augment data with signal initial values for SSR (e.g. {{ count }} → 0)
    let augmented_data =
        augment_data_with_signals(data, resolved.script_setup.as_deref());

    // Clean up HTML: strip directives, interpolate remaining {{ }}
    let clean_html = cleanup_html(&resolved.html, &augmented_data);

    let html = if clean_html.contains("<html") {
        let mut html = clean_html;
        inject_before_close(&mut html, "</head>", &css_ref);
        inject_before_close(&mut html, "</body>", &js_ref);
        html
    } else {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>Van App</title>
{css_ref}
</head>
<body>
{clean_html}
{js_ref}
</body>
</html>"#
        )
    };

    Ok(PageAssets { html, assets })
}

/// Clean up "dirty" resolved HTML by:
/// 1. Stripping `@event="..."` attributes
/// 2. Processing `v-show="expr"` / `v-if="expr"` → evaluate initial value, add
///    `style="display:none"` if falsy, remove the directive attribute
/// 3. Interpolating remaining `{{ expr }}` expressions
fn cleanup_html(html: &str, data: &Value) -> String {
    let mut result = html.to_string();

    // 1. Strip @event="..." attributes
    let event_re = Regex::new(r#"\s*@\w+="[^"]*""#).unwrap();
    result = event_re.replace_all(&result, "").to_string();

    // 1b. Strip <Transition> / </Transition> wrapper tags (keep inner content)
    let transition_re = Regex::new(r#"</?[Tt]ransition[^>]*>"#).unwrap();
    result = transition_re.replace_all(&result, "").to_string();

    // 1c. Strip :key="..." attributes (from v-for)
    let key_re = Regex::new(r#"\s*:key="[^"]*""#).unwrap();
    result = key_re.replace_all(&result, "").to_string();

    // 2. Process v-show/v-if: evaluate initial value, add display:none if falsy
    let show_re = Regex::new(r#"\s*v-(?:show|if)="([^"]*)""#).unwrap();
    result = show_re
        .replace_all(&result, |caps: &regex::Captures| {
            let expr = &caps[1];
            let value = resolve_path(data, expr);
            let is_falsy = value == "0"
                || value == "false"
                || value.is_empty()
                || value == "null"
                || value.contains("{{");
            if is_falsy {
                r#" style="display:none""#.to_string()
            } else {
                String::new()
            }
        })
        .to_string();

    // 2b. Process v-else-if="expr" (same as v-if)
    let else_if_re = Regex::new(r#"\s*v-else-if="([^"]*)""#).unwrap();
    result = else_if_re
        .replace_all(&result, |caps: &regex::Captures| {
            let expr = &caps[1];
            let value = resolve_path(data, expr);
            let is_falsy = value == "0"
                || value == "false"
                || value.is_empty()
                || value == "null"
                || value.contains("{{");
            if is_falsy {
                r#" style="display:none""#.to_string()
            } else {
                String::new()
            }
        })
        .to_string();

    // 2c. Strip v-else (unconditional — attribute with no value)
    let else_re = Regex::new(r#"\s+v-else"#).unwrap();
    result = else_re.replace_all(&result, "").to_string();

    // 2d. Strip v-html="..." and v-text="..." attributes
    let vhtml_re = Regex::new(r#"\s*v-html="[^"]*""#).unwrap();
    result = vhtml_re.replace_all(&result, "").to_string();
    let vtext_re = Regex::new(r#"\s*v-text="[^"]*""#).unwrap();
    result = vtext_re.replace_all(&result, "").to_string();

    // 2e. Strip :class="..." and :style="..." attributes
    let bind_class_re = Regex::new(r#"\s*:class="[^"]*""#).unwrap();
    result = bind_class_re.replace_all(&result, "").to_string();
    let bind_style_re = Regex::new(r#"\s*:style="[^"]*""#).unwrap();
    result = bind_style_re.replace_all(&result, "").to_string();

    // 2f. Strip v-model="..." and optionally set initial value
    let model_re = Regex::new(r#"\s*v-model="([^"]*)""#).unwrap();
    result = model_re
        .replace_all(&result, |caps: &regex::Captures| {
            let expr = &caps[1];
            let value = resolve_path(data, expr);
            if value.contains("{{") {
                String::new()
            } else {
                format!(r#" value="{}""#, value)
            }
        })
        .to_string();

    // 3. Interpolate remaining {{ expr }}
    result = interpolate(&result, data);

    result
}

/// Escape HTML special characters in text content.
pub fn escape_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(ch),
        }
    }
    result
}

/// Perform `{{ expr }}` / `{{{ expr }}}` interpolation with dot-path resolution.
///
/// - `{{ expr }}` — HTML-escaped output (default, safe)
/// - `{{{ expr }}}` — raw output (no escaping, for trusted HTML content)
///
/// Supports paths like `user.name` which resolve to `data["user"]["name"]`.
/// Unresolved expressions are left as-is.
pub fn interpolate(template: &str, data: &Value) -> String {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);

        // Check for triple mustache {{{ }}} (raw, unescaped output)
        if rest[start..].starts_with("{{{") {
            let after_open = &rest[start + 3..];
            if let Some(end) = after_open.find("}}}") {
                let expr = after_open[..end].trim();
                let value = resolve_path(data, expr);
                result.push_str(&value);
                rest = &after_open[end + 3..];
            } else {
                result.push_str("{{{");
                rest = &rest[start + 3..];
            }
        } else {
            let after_open = &rest[start + 2..];
            if let Some(end) = after_open.find("}}") {
                let expr = after_open[..end].trim();
                let value = resolve_path(data, expr);
                result.push_str(&escape_html(&value));
                rest = &after_open[end + 2..];
            } else {
                result.push_str("{{");
                rest = after_open;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Resolve a dot-separated path like `user.name` against a JSON value.
pub fn resolve_path(data: &Value, path: &str) -> String {
    let mut current = data;
    for key in path.split('.') {
        let key = key.trim();
        match current.get(key) {
            Some(v) => current = v,
            None => return format!("{{{{{}}}}}", path),
        }
    }
    match current {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_interpolate_simple() {
        let data = json!({"name": "World"});
        assert_eq!(interpolate("Hello {{ name }}!", &data), "Hello World!");
    }

    #[test]
    fn test_interpolate_dot_path() {
        let data = json!({"user": {"name": "Alice"}});
        assert_eq!(interpolate("Hi {{ user.name }}!", &data), "Hi Alice!");
    }

    #[test]
    fn test_interpolate_missing_key() {
        let data = json!({});
        assert_eq!(interpolate("{{ missing }}", &data), "{{missing}}");
    }

    #[test]
    fn test_cleanup_html_strips_events() {
        let html = r#"<button @click="increment">+1</button>"#;
        let data = json!({});
        let clean = cleanup_html(html, &data);
        assert_eq!(clean, "<button>+1</button>");
    }

    #[test]
    fn test_cleanup_html_v_show_falsy() {
        let html = r#"<p v-show="visible">Hello</p>"#;
        let data = json!({"visible": false});
        let clean = cleanup_html(html, &data);
        assert!(!clean.contains("v-show"));
        assert!(clean.contains(r#"style="display:none""#));
    }

    #[test]
    fn test_cleanup_html_v_show_truthy() {
        let html = r#"<p v-show="visible">Hello</p>"#;
        let data = json!({"visible": true});
        let clean = cleanup_html(html, &data);
        assert!(!clean.contains("v-show"));
        assert_eq!(clean, "<p>Hello</p>");
    }

    #[test]
    fn test_cleanup_html_strips_transition_tags() {
        let html = r#"<div><Transition name="slide"><p v-show="open">Hi</p></Transition></div>"#;
        let data = json!({"open": false});
        let clean = cleanup_html(html, &data);
        assert!(!clean.contains("Transition"));
        assert!(!clean.contains("transition"));
        assert!(clean.contains("<p"));
    }

    #[test]
    fn test_interpolate_escapes_html() {
        let data = json!({"desc": "<script>alert('xss')</script>"});
        assert_eq!(
            interpolate("{{ desc }}", &data),
            "&lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;"
        );
    }

    #[test]
    fn test_interpolate_triple_mustache_raw() {
        let data = json!({"html": "<b>bold</b>"});
        assert_eq!(interpolate("{{{ html }}}", &data), "<b>bold</b>");
    }

    #[test]
    fn test_interpolate_mixed_escaped_and_raw() {
        let data = json!({"safe": "<b>bold</b>", "text": "<em>hi</em>"});
        assert_eq!(
            interpolate("{{{ safe }}} and {{ text }}", &data),
            "<b>bold</b> and &lt;em&gt;hi&lt;/em&gt;"
        );
    }

    #[test]
    fn test_render_page_basic() {
        let resolved = ResolvedComponent {
            html: "<h1>Hello</h1>".to_string(),
            styles: vec!["h1 { color: red; }".to_string()],
            script_setup: None,
            module_imports: Vec::new(),
        };
        let data = json!({});
        let html = render_page(&resolved, &data).unwrap();
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("h1 { color: red; }"));
        // Should NOT contain client.js WebSocket reload
        assert!(!html.contains("__van/ws"));
    }
}
