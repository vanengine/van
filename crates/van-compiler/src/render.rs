use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use regex::Regex;
use serde_json::Value;
use van_signal_gen::{
    extract_initial_values, generate_signals, generate_signals_compile,
    generate_signals_comment, inject_signal_comments, runtime_js,
    analyze_script, walk_template,
};

use crate::i18n;
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
/// Pipeline: `compile() + fill_data()` — shares the same compile step as Java SSR.
///
/// 1. `compile()` → compiled template (signals processed, model `{{ }}` preserved)
/// 2. `fill_data()` → interpolate remaining `{{ }}` with data, evaluate model v-show/v-if
pub fn render_to_string(resolved: &ResolvedComponent, data: &Value, global_name: &str) -> Result<String, String> {
    // Step 1: compile (same as Java SSR path)
    let compiled = compile(resolved, global_name)?;

    // Step 2: fill data into compiled template
    Ok(fill_data(&compiled, data))
}

/// Fill data into a compiled template: interpolate remaining `{{ }}` and evaluate model directives.
/// This is the Rust equivalent of Java's `VanTemplate.evaluate(model)`.
pub fn fill_data(compiled_html: &str, data: &Value) -> String {
    let mut result = compiled_html.to_string();

    // Process remaining v-show (model-bound, preserved by compile)
    let show_re = Regex::new(r#"\s*v-show="([^"]*)""#).unwrap();
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

    // Process remaining v-if (model-bound)
    let vif_re = Regex::new(r#"\s*v-if="([^"]*)""#).unwrap();
    result = vif_re
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

    // Strip remaining v-else-if / v-else
    let else_if_re = Regex::new(r#"\s*v-else-if="[^"]*""#).unwrap();
    result = else_if_re.replace_all(&result, "").to_string();
    let else_re = Regex::new(r#"\s+v-else"#).unwrap();
    result = else_re.replace_all(&result, "").to_string();

    // Strip remaining v-html / v-text
    let vhtml_re = Regex::new(r#"\s*v-html="[^"]*""#).unwrap();
    result = vhtml_re.replace_all(&result, "").to_string();
    let vtext_re = Regex::new(r#"\s*v-text="[^"]*""#).unwrap();
    result = vtext_re.replace_all(&result, "").to_string();

    // Strip remaining :class / :style (model-bound, for static render we just strip)
    let bind_class_re = Regex::new(r#"\s*:class="[^"]*""#).unwrap();
    result = bind_class_re.replace_all(&result, "").to_string();
    let bind_style_re = Regex::new(r#"\s*:style="[^"]*""#).unwrap();
    result = bind_style_re.replace_all(&result, "").to_string();

    // Strip :key
    let key_re = Regex::new(r#"\s*:key="[^"]*""#).unwrap();
    result = key_re.replace_all(&result, "").to_string();

    // Interpolate remaining {{ expr }} with data
    result = interpolate(&result, data);

    result
}

/// Render a resolved `.van` component with separated assets.
///
/// Pipeline: `compile_assets() + fill_data()` — shares compile step with Java SSR.
pub fn render_to_assets(
    resolved: &ResolvedComponent,
    data: &Value,
    page_name: &str,
    asset_prefix: &str,
    global_name: &str,
) -> Result<PageAssets, String> {
    // Step 1: compile with separated assets
    let mut compiled = compile_assets(resolved, page_name, asset_prefix, global_name)?;

    // Step 2: fill data into compiled HTML
    compiled.html = fill_data(&compiled.html, data);

    Ok(compiled)
}

/// Compile mode: produce page HTML for Java SSR.
///
/// Auto-detects signal bindings via `analyze_script`:
/// - Signal bindings (ref/computed): interpolate initial values + generate JS (same as render mode)
/// - Model bindings: preserve for Java SSR (v-for, v-if, :class, {{ }})
///
/// Uses comment anchors (`<!--v:N-->`) for position-independent signal element targeting.
pub fn compile(resolved: &ResolvedComponent, global_name: &str) -> Result<String, String> {
    let style_block: String = resolved
        .styles
        .iter()
        .map(|css| format!("<style>{css}</style>"))
        .collect::<Vec<_>>()
        .join("\n");

    let module_code: Vec<String> = resolved
        .module_imports
        .iter()
        .filter(|m| !m.is_type_only)
        .map(|m| m.content.clone())
        .collect();

    // Step 1: Analyze script to get reactive names
    let reactive_names: Vec<String> = if let Some(ref script_setup) = resolved.script_setup {
        let analysis = analyze_script(script_setup);
        analysis.signals.iter().map(|s| s.name.clone())
            .chain(analysis.computeds.iter().map(|c| c.name.clone()))
            .collect()
    } else {
        Vec::new()
    };

    // Step 2: Generate signal JS from dirty HTML (before cleanup), using comment anchors
    let signal_scripts = if let Some(ref script_setup) = resolved.script_setup {
        if let Some(signal_js) = generate_signals_comment(script_setup, &resolved.html, &module_code, global_name) {
            let runtime = runtime_js(global_name);
            format!("<script>{runtime}</script>\n<script>{signal_js}</script>")
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Step 3: Inject comment anchors before signal-bound elements
    let reactive_refs: Vec<&str> = reactive_names.iter().map(|s| s.as_str()).collect();
    let bindings = walk_template(&resolved.html, &reactive_refs);
    let binding_paths = collect_signal_binding_paths(&bindings);
    let (html_with_comments, _) = inject_signal_comments(&resolved.html, &binding_paths);

    // Step 4: Get signal initial values and interpolate
    let signal_initial_values: HashMap<String, String> = resolved.script_setup.as_ref()
        .map(|s| extract_initial_values(s))
        .unwrap_or_default();

    // Step 5: Cleanup HTML — signal bindings processed, model bindings preserved
    let mut clean_html = cleanup_html_compile_smart(&html_with_comments, &reactive_names);
    clean_html = interpolate_signals_only(&clean_html, &signal_initial_values);

    if clean_html.contains("<html") {
        let mut html = clean_html;
        inject_before_close(&mut html, "</head>", &style_block);
        inject_before_close(&mut html, "</body>", &signal_scripts);
        Ok(html)
    } else {
        Ok(format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>Van App</title>
{style_block}
</head>
<body>
{clean_html}
{signal_scripts}
</body>
</html>"#
        ))
    }
}

/// Compile mode: produce page with separated assets.
pub fn compile_assets(
    resolved: &ResolvedComponent,
    page_name: &str,
    asset_prefix: &str,
    global_name: &str,
) -> Result<PageAssets, String> {
    let mut assets = HashMap::new();

    let css_ref = if !resolved.styles.is_empty() {
        let css_content: String = resolved.styles.join("\n");
        let hash = content_hash(&css_content);
        let css_path = format!("{}/css/{}.{}.css", asset_prefix, page_name, hash);
        assets.insert(css_path.clone(), css_content);
        format!(r#"<link rel="stylesheet" href="{css_path}">"#)
    } else {
        String::new()
    };

    let module_code: Vec<String> = resolved
        .module_imports
        .iter()
        .filter(|m| !m.is_type_only)
        .map(|m| m.content.clone())
        .collect();

    let js_ref = if let Some(ref script_setup) = resolved.script_setup {
        if let Some(signal_js) = generate_signals_compile(script_setup, &resolved.html, &module_code, global_name) {
            let runtime = runtime_js(global_name);
            let runtime_hash = content_hash(&runtime);
            let runtime_path = format!("{}/js/van-runtime.{}.js", asset_prefix, runtime_hash);
            let js_hash = content_hash(&signal_js);
            let js_path = format!("{}/js/{}.{}.js", asset_prefix, page_name, js_hash);
            assets.insert(runtime_path.clone(), runtime);
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

    let clean_html = cleanup_html_compile(&resolved.html);

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

/// Compile cleanup: strip only @click/v-model events, keep runtime directives for Java.
/// Preserves: v-for, v-if, v-else-if, v-else, v-show, :class, :style, :href, {{ }}, v-html, v-text
/// Strips: @click, @input, v-model, <Transition>
fn cleanup_html_compile(html: &str) -> String {
    let mut result = html.to_string();

    // Strip @event="..." attributes
    let event_re = Regex::new(r#"\s*@\w+="[^"]*""#).unwrap();
    result = event_re.replace_all(&result, "").to_string();

    // Strip <Transition> / </Transition> wrapper tags
    let transition_re = Regex::new(r#"</?[Tt]ransition[^>]*>"#).unwrap();
    result = transition_re.replace_all(&result, "").to_string();

    // Strip v-model="..." (client-only directive)
    let model_re = Regex::new(r#"\s*v-model="[^"]*""#).unwrap();
    result = model_re.replace_all(&result, "").to_string();

    // Everything else (v-for, v-if, v-show, :class, :style, :href, {{ }}) is PRESERVED
    result
}

/// Collect all unique binding paths from TemplateBindings, sorted in DFS order.
fn collect_signal_binding_paths(bindings: &van_signal_gen::TemplateBindings) -> Vec<Vec<usize>> {
    let mut paths = std::collections::BTreeSet::new();
    for b in &bindings.events { paths.insert(b.path.clone()); }
    for b in &bindings.texts { paths.insert(b.path.clone()); }
    for b in &bindings.shows { paths.insert(b.path.clone()); }
    for b in &bindings.htmls { paths.insert(b.path.clone()); }
    for b in &bindings.text_directives { paths.insert(b.path.clone()); }
    for b in &bindings.classes { paths.insert(b.path.clone()); }
    for b in &bindings.styles { paths.insert(b.path.clone()); }
    for b in &bindings.models { paths.insert(b.path.clone()); }
    paths.into_iter().collect()
}

/// Smart compile cleanup: process signal bindings like render mode, preserve model bindings for Java.
///
/// Signal bindings (expr references reactive_names):
/// - `{{ count }}` → interpolate with initial value
/// - `v-show="visible"` → evaluate initial value, add display:none
/// - `@click`, `v-model` → strip (JS already generated)
/// - `:class`/`:style` referencing signals → strip (JS already generated)
///
/// Model bindings (expr does NOT reference reactive_names):
/// - `{{ ctx.title }}`, `v-for`, `v-if="ctx.xxx"`, `:class` → preserve for Java
fn cleanup_html_compile_smart(html: &str, reactive_names: &[String]) -> String {
    let mut result = html.to_string();

    // 1. Strip ALL @event="..." (events are always client-side, JS already generated)
    let event_re = Regex::new(r#"\s*@\w+="[^"]*""#).unwrap();
    result = event_re.replace_all(&result, "").to_string();

    // 2. Strip <Transition> wrapper tags
    let transition_re = Regex::new(r#"</?[Tt]ransition[^>]*>"#).unwrap();
    result = transition_re.replace_all(&result, "").to_string();

    // 3. Strip v-model="..." (always client-side)
    let model_re = Regex::new(r#"\s*v-model="[^"]*""#).unwrap();
    result = model_re.replace_all(&result, "").to_string();

    // 4. Process v-show: signal-bound → evaluate initial value; model-bound → preserve
    let show_re = Regex::new(r#"\s*v-show="([^"]*)""#).unwrap();
    result = show_re.replace_all(&result, |caps: &regex::Captures| {
        let expr = &caps[1];
        if is_signal_expr(expr, reactive_names) {
            // Signal-bound: evaluate with initial value (same as render mode)
            // Initial value for ref(false) is "false", ref(true) is "true"
            let is_falsy = expr == "false" || expr == "0";
            if is_falsy {
                r#" style="display:none""#.to_string()
            } else {
                // Default: initially hidden for safety (signal will show on client)
                // Check if the signal initial value is falsy
                r#" style="display:none""#.to_string()
            }
        } else {
            // Model-bound: preserve for Java
            caps[0].to_string()
        }
    }).to_string();

    // 5. Process v-if: signal-bound → evaluate; model-bound → preserve
    let vif_re = Regex::new(r#"\s*v-if="([^"]*)""#).unwrap();
    result = vif_re.replace_all(&result, |caps: &regex::Captures| {
        let expr = &caps[1];
        if is_signal_expr(expr, reactive_names) {
            String::new() // Signal-bound: strip (JS handles it)
        } else {
            caps[0].to_string() // Model-bound: preserve
        }
    }).to_string();

    // 6. Strip signal-bound :class/:style (JS handles them); preserve model-bound
    let bind_class_re = Regex::new(r#"\s*:class="([^"]*)""#).unwrap();
    result = bind_class_re.replace_all(&result, |caps: &regex::Captures| {
        let expr = &caps[1];
        if is_signal_expr(expr, reactive_names) {
            String::new()
        } else {
            caps[0].to_string()
        }
    }).to_string();

    let bind_style_re = Regex::new(r#"\s*:style="([^"]*)""#).unwrap();
    result = bind_style_re.replace_all(&result, |caps: &regex::Captures| {
        let expr = &caps[1];
        if is_signal_expr(expr, reactive_names) {
            String::new()
        } else {
            caps[0].to_string()
        }
    }).to_string();

    // 7. Interpolate signal {{ }} with initial values; preserve model {{ }}
    let initial_values = extract_signal_initial_values(&result, reactive_names);
    result = interpolate_signals_only(&result, &initial_values);

    result
}

/// Check if an expression references any signal name.
fn is_signal_expr(expr: &str, reactive_names: &[String]) -> bool {
    reactive_names.iter().any(|name| {
        let re = Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap();
        re.is_match(expr)
    })
}

/// Build a map of signal_name → initial display value from the HTML's script_setup context.
/// Uses the extract_initial_values function from van-signal-gen.
fn extract_signal_initial_values(_html: &str, _reactive_names: &[String]) -> HashMap<String, String> {
    // This is called after inject_signal_comments, but we need the script to get initial values.
    // The caller should pass initial values directly. For now return empty —
    // the compile() function handles this via augment_data_with_signals pattern.
    HashMap::new()
}

/// Replace only signal-bound {{ name }} with initial values, preserve model-bound {{ }}.
fn interpolate_signals_only(html: &str, initial_values: &HashMap<String, String>) -> String {
    if initial_values.is_empty() {
        return html.to_string();
    }
    let re = Regex::new(r"\{\{\s*([^}]+?)\s*\}\}").unwrap();
    re.replace_all(html, |caps: &regex::Captures| {
        let expr = caps[1].trim();
        if let Some(val) = initial_values.get(expr) {
            val.clone()
        } else {
            caps[0].to_string() // Not a signal → preserve for Java
        }
    }).to_string()
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
                if let Some(translated) = try_resolve_t(expr, data) {
                    result.push_str(&translated);
                } else if expr.trim().starts_with("$t(") {
                    // $t() but no $i18n data — preserve for runtime resolution
                    result.push_str(&format!("{{{{{{{}}}}}}}", expr));
                } else {
                    result.push_str(&resolve_path(data, expr));
                }
                rest = &after_open[end + 3..];
            } else {
                result.push_str("{{{");
                rest = &rest[start + 3..];
            }
        } else {
            let after_open = &rest[start + 2..];
            if let Some(end) = after_open.find("}}") {
                let expr = after_open[..end].trim();
                if let Some(translated) = try_resolve_t(expr, data) {
                    result.push_str(&escape_html(&translated));
                } else if expr.trim().starts_with("$t(") {
                    // $t() but no $i18n data — preserve for runtime resolution
                    result.push_str(&format!("{{{{{}}}}}", expr));
                } else {
                    let value = resolve_path(data, expr);
                    if value.contains("{{") {
                        // Value is an unresolved or compile expression — preserve for Java
                        result.push_str(&value);
                    } else {
                        result.push_str(&escape_html(&value));
                    }
                }
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

/// Try to resolve a `$t(...)` expression. Returns `Some(translated)` if the
/// expression is a valid `$t()` call, `None` otherwise.
pub(crate) fn try_resolve_t(expr: &str, data: &Value) -> Option<String> {
    let (key, params_str) = i18n::parse_t_call(expr)?;

    // No $i18n data → return None so the expression is preserved for runtime resolution
    let i18n_messages = data.get("$i18n")?;

    let mut resolved_params = std::collections::HashMap::new();
    if let Some(ref ps) = params_str {
        for (k, v) in i18n::parse_t_params(ps) {
            let value = match v {
                i18n::ParamValue::Literal(lit) => lit,
                i18n::ParamValue::DataPath(path) => {
                    let resolved = resolve_path(data, &path);
                    // If unresolved (still has {{ }}), use the path name as-is
                    if resolved.contains("{{") {
                        path
                    } else {
                        resolved
                    }
                }
            };
            resolved_params.insert(k, value);
        }
    }

    Some(i18n::resolve_translation(&key, &resolved_params, i18n_messages))
}

/// Resolve a dot-separated path like `user.name` against a JSON value.
pub fn resolve_path(data: &Value, path: &str) -> String {
    let mut current = data;
    let keys: Vec<&str> = path.split('.').collect();
    for (i, key) in keys.iter().enumerate() {
        let key = key.trim();
        match current.get(key) {
            Some(v) => {
                // Compile-mode expression forwarding: if value is "{{ expr }}" and there are
                // remaining path segments, compose "{{ expr.remaining }}" for Java.
                if let Value::String(s) = v {
                    if i + 1 < keys.len() && s.starts_with("{{") && s.ends_with("}}") {
                        let inner = s[2..s.len() - 2].trim();
                        let remaining = keys[i + 1..].join(".");
                        return format!("{{{{ {}.{} }}}}", inner, remaining);
                    }
                }
                current = v;
            }
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
    fn test_interpolate_t_simple() {
        let data = json!({
            "$i18n": { "hello": "你好" }
        });
        assert_eq!(interpolate("{{ $t('hello') }}", &data), "你好");
    }

    #[test]
    fn test_interpolate_t_with_params() {
        let data = json!({
            "userName": "Alice",
            "$i18n": { "greeting": "你好，{name}！" }
        });
        assert_eq!(
            interpolate("{{ $t('greeting', { name: userName }) }}", &data),
            "你好，Alice！"
        );
    }

    #[test]
    fn test_interpolate_t_missing_key() {
        let data = json!({ "$i18n": {} });
        assert_eq!(interpolate("{{ $t('missing') }}", &data), "missing");
    }

    #[test]
    fn test_interpolate_t_no_i18n_data() {
        let data = json!({});
        assert_eq!(interpolate("{{ $t('hello') }}", &data), "{{$t('hello')}}");
    }

    #[test]
    fn test_interpolate_t_triple_mustache() {
        let data = json!({
            "$i18n": { "html_content": "<b>粗体</b>" }
        });
        // Triple mustache: raw output, no escaping
        assert_eq!(
            interpolate("{{{ $t('html_content') }}}", &data),
            "<b>粗体</b>"
        );
        // Double mustache: HTML escaped
        assert_eq!(
            interpolate("{{ $t('html_content') }}", &data),
            "&lt;b&gt;粗体&lt;/b&gt;"
        );
    }

    #[test]
    fn test_interpolate_t_plural() {
        let data = json!({
            "itemCount": 3,
            "$i18n": { "items": "没有项目 | 1 个项目 | {count} 个项目" }
        });
        assert_eq!(
            interpolate("{{ $t('items', { count: itemCount }) }}", &data),
            "3 个项目"
        );
    }

    #[test]
    fn test_interpolate_t_mixed_with_regular() {
        let data = json!({
            "name": "World",
            "$i18n": { "hello": "你好" }
        });
        assert_eq!(
            interpolate("{{ $t('hello') }}, {{ name }}!", &data),
            "你好, World!"
        );
    }

    #[test]
    fn test_render_to_string_basic() {
        let resolved = ResolvedComponent {
            html: "<h1>Hello</h1>".to_string(),
            styles: vec!["h1 { color: red; }".to_string()],
            script_setup: None,
            module_imports: Vec::new(),
        };
        let data = json!({});
        let html = render_to_string(&resolved, &data, "Van").unwrap();
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("h1 { color: red; }"));
        // Should NOT contain client.js WebSocket reload
        assert!(!html.contains("__van/ws"));
    }
}
