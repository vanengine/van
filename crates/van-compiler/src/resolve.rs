use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use van_parser::{add_scope_class, parse_blocks, parse_imports, parse_script_imports, scope_css, scope_id, VanImport};

use crate::render::{interpolate, resolve_path as resolve_json_path};

const MAX_DEPTH: usize = 10;

/// A resolved non-component module import (.ts/.js file).
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// Resolved virtual path.
    pub path: String,
    /// File content from the files map.
    pub content: String,
    /// Whether this is a type-only import (should be erased).
    pub is_type_only: bool,
}

/// The result of resolving a `.van` file (with or without imports).
#[derive(Debug)]
pub struct ResolvedComponent {
    /// The fully rendered HTML content.
    pub html: String,
    /// Collected CSS styles from this component and all descendants.
    pub styles: Vec<String>,
    /// The `<script setup>` content (for signal generation).
    pub script_setup: Option<String>,
    /// Resolved non-component module imports (.ts/.js files).
    pub module_imports: Vec<ResolvedModule>,
}

// ─── Multi-file resolve (HashMap-based, no FS) ─────────────────────────

/// Resolve a `.van` entry file with all its component imports from an in-memory file map.
///
/// This is the main API for multi-file compilation (playground, WASM, tests).
/// Files are keyed by virtual path (e.g. `"index.van"`, `"hello.van"`).
pub fn resolve_with_files(
    entry_path: &str,
    files: &HashMap<String, String>,
    data: &Value,
) -> Result<ResolvedComponent, String> {
    resolve_with_files_inner(entry_path, files, data, false, &HashMap::new())
}

/// Like `resolve_with_files`, but with debug HTML comments showing component/slot boundaries.
///
/// `file_origins` maps each file path to its theme name (e.g. `"components/header.van" → "van1"`).
/// When present, debug comments include the theme: `<!-- START: [van1] components/header.van -->`.
pub fn resolve_with_files_debug(
    entry_path: &str,
    files: &HashMap<String, String>,
    data: &Value,
    file_origins: &HashMap<String, String>,
) -> Result<ResolvedComponent, String> {
    resolve_with_files_inner(entry_path, files, data, true, file_origins)
}

fn resolve_with_files_inner(
    entry_path: &str,
    files: &HashMap<String, String>,
    data: &Value,
    debug: bool,
    file_origins: &HashMap<String, String>,
) -> Result<ResolvedComponent, String> {
    let source = files
        .get(entry_path)
        .ok_or_else(|| format!("Entry file not found: {entry_path}"))?;

    let blocks = parse_blocks(source);
    let reactive_names = if let Some(ref script) = blocks.script_setup {
        extract_reactive_names(script)
    } else {
        Vec::new()
    };

    resolve_recursive(source, data, entry_path, files, 0, &reactive_names, debug, file_origins)
}

/// Recursively resolve component tags in a `.van` source using in-memory files.
fn resolve_recursive(
    source: &str,
    data: &Value,
    current_path: &str,
    files: &HashMap<String, String>,
    depth: usize,
    reactive_names: &[String],
    debug: bool,
    file_origins: &HashMap<String, String>,
) -> Result<ResolvedComponent, String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "Component nesting exceeded maximum depth of {MAX_DEPTH}"
        ));
    }

    let blocks = parse_blocks(source);
    let mut template = blocks
        .template
        .unwrap_or_else(|| "<p>No template block found.</p>".to_string());

    let mut styles: Vec<String> = Vec::new();
    if let Some(css) = &blocks.style {
        if blocks.style_scoped {
            let id = scope_id(css);
            template = add_scope_class(&template, &id);
            styles.push(scope_css(css, &id));
        } else {
            styles.push(css.clone());
        }
    }

    // Parse imports from script setup to build tag -> import mapping
    let imports = if let Some(ref script) = blocks.script_setup {
        parse_imports(script)
    } else {
        Vec::new()
    };

    let import_map: HashMap<String, &VanImport> = imports
        .iter()
        .map(|imp| (imp.tag_name.clone(), imp))
        .collect();

    // Expand v-for directives before component resolution
    template = expand_v_for(&template, data);

    // Repeatedly find and replace component tags until none remain
    loop {
        let tag_match = find_component_tag(&template, &import_map);
        let Some(tag_info) = tag_match else {
            break;
        };

        let imp = &import_map[&tag_info.tag_name];

        // Resolve the component .van file via virtual path
        let resolved_key = resolve_virtual_path(current_path, &imp.path);
        let component_source = files
            .get(&resolved_key)
            .ok_or_else(|| format!("Component not found: {} (resolved from '{}')", resolved_key, imp.path))?;

        // Parse props from the tag and build child data context
        let child_data = parse_props(&tag_info.attrs, data);

        // Parse slot content from children (using parent data + parent import_map)
        let slot_result = parse_slot_content(
            &tag_info.children,
            data,
            &imports,
            current_path,
            files,
            depth,
            reactive_names,
            debug,
            file_origins,
        )?;

        // Recursively resolve the child component
        let child_resolved = resolve_recursive(
            component_source,
            &child_data,
            &resolved_key,
            files,
            depth + 1,
            reactive_names,
            debug,
            file_origins,
        )?;

        // Distribute slots into the child's rendered HTML
        // Build per-slot theme map: check slot-specific origin first, then file-level origin
        let file_theme = file_origins.get(current_path).cloned().unwrap_or_default();
        let mut slot_themes: HashMap<String, String> = HashMap::new();
        for slot_name in slot_result.slots.keys() {
            let slot_key = format!("{}#{}", current_path, slot_name);
            let theme = file_origins.get(&slot_key).unwrap_or(&file_theme);
            slot_themes.insert(slot_name.clone(), theme.clone());
        }
        let with_slots = distribute_slots(&child_resolved.html, &slot_result.slots, debug, &slot_themes);

        // Replace the component tag with the resolved content
        let replacement = if debug {
            let theme_prefix = file_origins.get(&resolved_key)
                .map(|t| format!("[{t}] "))
                .unwrap_or_default();
            format!("<!-- START: {theme_prefix}{resolved_key} -->{with_slots}<!-- END: {theme_prefix}{resolved_key} -->")
        } else {
            with_slots
        };

        template = format!(
            "{}{}{}",
            &template[..tag_info.start],
            replacement,
            &template[tag_info.end..],
        );

        // Collect child styles and slot component styles
        styles.extend(child_resolved.styles);
        styles.extend(slot_result.styles);
    }

    // Reactive-aware interpolation: leave reactive {{ expr }} as-is for
    // signal gen to find via tree walking; interpolate non-reactive ones.
    let html = if !reactive_names.is_empty() {
        interpolate_skip_reactive(&template, data, reactive_names)
    } else {
        interpolate(&template, data)
    };

    // Capture script_setup and resolve module imports only at top level
    let script_setup = if depth == 0 {
        blocks.script_setup.clone()
    } else {
        None
    };

    let module_imports = if depth == 0 {
        if let Some(ref script) = blocks.script_setup {
            let script_imports = parse_script_imports(script);
            script_imports
                .into_iter()
                .filter_map(|imp| {
                    if imp.is_type_only {
                        return None; // type-only imports are erased
                    }
                    let resolved_key = resolve_virtual_path(current_path, &imp.path);
                    let content = files.get(&resolved_key)?;
                    Some(ResolvedModule {
                        path: resolved_key,
                        content: content.clone(),
                        is_type_only: false,
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    Ok(ResolvedComponent {
        html,
        styles,
        script_setup,
        module_imports,
    })
}

// ─── Single-file resolve (no imports, no FS) ────────────────────────────

/// Resolve a single `.van` source into HTML + styles (no import resolution).
///
/// This is the backward-compatible entry point for single-file usage.
pub fn resolve_single(source: &str, data: &Value) -> Result<ResolvedComponent, String> {
    resolve_single_with_path(source, data, "")
}

/// Like `resolve_single`, but kept for API compatibility.
pub fn resolve_single_with_path(source: &str, data: &Value, _path: &str) -> Result<ResolvedComponent, String> {
    let blocks = parse_blocks(source);

    let mut template = blocks
        .template
        .unwrap_or_else(|| "<p>No template block found.</p>".to_string());

    let mut styles: Vec<String> = Vec::new();
    if let Some(css) = &blocks.style {
        if blocks.style_scoped {
            let id = scope_id(css);
            template = add_scope_class(&template, &id);
            styles.push(scope_css(css, &id));
        } else {
            styles.push(css.clone());
        }
    }

    // Extract reactive names from script setup
    let reactive_names = if let Some(ref script) = blocks.script_setup {
        extract_reactive_names(script)
    } else {
        Vec::new()
    };

    // Reactive-aware interpolation
    let html = if !reactive_names.is_empty() {
        interpolate_skip_reactive(&template, data, &reactive_names)
    } else {
        interpolate(&template, data)
    };

    Ok(ResolvedComponent {
        html,
        styles,
        script_setup: blocks.script_setup.clone(),
        module_imports: Vec::new(),
    })
}

// ─── Virtual path resolution ────────────────────────────────────────────

/// Resolve a relative import path against a current file's virtual path.
///
/// ```text
/// current_file="index.van", import="./hello.van" → "hello.van"
/// current_file="pages/index.van", import="../components/hello.van" → "components/hello.van"
/// current_file="pages/index.van", import="./sub.van" → "pages/sub.van"
/// import="@van-ui/button/button.van" → "@van-ui/button/button.van" (scoped package, returned as-is)
/// ```
fn resolve_virtual_path(current_file: &str, import_path: &str) -> String {
    // @scope/pkg paths are absolute references into node_modules — return as-is
    if import_path.starts_with('@') {
        return import_path.to_string();
    }

    // Get the directory of the current file
    let dir = if let Some(pos) = current_file.rfind('/') {
        &current_file[..pos]
    } else {
        "" // root level
    };

    let combined = if dir.is_empty() {
        import_path.to_string()
    } else {
        format!("{}/{}", dir, import_path)
    };

    normalize_virtual_path(&combined)
}

/// Normalize a virtual path by resolving `.` and `..` segments.
fn normalize_virtual_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

// ─── Shared helpers ─────────────────────────────────────────────────────

/// Extract reactive signal names from script setup (ref/computed declarations).
pub fn extract_reactive_names(script: &str) -> Vec<String> {
    let ref_re = Regex::new(r#"const\s+(\w+)\s*=\s*ref\("#).unwrap();
    let computed_re = Regex::new(r#"const\s+(\w+)\s*=\s*computed\("#).unwrap();
    let mut names = Vec::new();
    for cap in ref_re.captures_iter(script) {
        names.push(cap[1].to_string());
    }
    for cap in computed_re.captures_iter(script) {
        names.push(cap[1].to_string());
    }
    names
}

/// Interpolate `{{ expr }}` but leave reactive expressions as-is.
fn interpolate_skip_reactive(template: &str, data: &Value, reactive_names: &[String]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];

        if let Some(end) = after_open.find("}}") {
            let expr = after_open[..end].trim();

            // Check if expression references any reactive signal
            let is_reactive = reactive_names.iter().any(|name| {
                let bytes = expr.as_bytes();
                let name_bytes = name.as_bytes();
                let name_len = name.len();
                let mut i = 0;
                let mut found = false;
                while i + name_len <= bytes.len() {
                    if &bytes[i..i + name_len] == name_bytes {
                        let before_ok = i == 0 || !(bytes[i - 1] as char).is_alphanumeric();
                        let after_ok = i + name_len == bytes.len()
                            || !(bytes[i + name_len] as char).is_alphanumeric();
                        if before_ok && after_ok {
                            found = true;
                            break;
                        }
                    }
                    i += 1;
                }
                found
            });

            if is_reactive {
                result.push_str(&format!("{{{{ {expr} }}}}"));
            } else {
                let value = resolve_json_path(data, expr);
                result.push_str(&value);
            }
            rest = &after_open[end + 2..];
        } else {
            result.push_str("{{");
            rest = after_open;
        }
    }
    result.push_str(rest);
    result
}

// ─── Component tag matching ─────────────────────────────────────────────

/// Information about a matched component tag in the template.
struct TagInfo {
    tag_name: String,
    attrs: String,
    children: String,
    start: usize,
    end: usize,
}

/// Find the first component tag in the template that matches an import.
fn find_component_tag(template: &str, import_map: &HashMap<String, &VanImport>) -> Option<TagInfo> {
    for tag_name in import_map.keys() {
        if let Some(info) = extract_component_tag(template, tag_name) {
            return Some(info);
        }
    }
    None
}

/// Extract a component tag (self-closing or paired) from the template.
fn extract_component_tag(template: &str, tag_name: &str) -> Option<TagInfo> {
    let open_pattern = format!("<{}", tag_name);

    let start = template.find(&open_pattern)?;

    // Verify it's a complete tag name (next char must be space, /, or >)
    let after_tag = start + open_pattern.len();
    if after_tag < template.len() {
        let next_ch = template.as_bytes()[after_tag] as char;
        if next_ch != ' '
            && next_ch != '/'
            && next_ch != '>'
            && next_ch != '\n'
            && next_ch != '\r'
            && next_ch != '\t'
        {
            return None;
        }
    }

    // Find the end of the opening tag '>'
    let rest = &template[start..];
    let gt_pos = rest.find('>')?;

    // Check for self-closing: ends with />
    let is_self_closing = rest[..gt_pos].ends_with('/');

    if is_self_closing {
        let attr_start = open_pattern.len();
        let attr_end = gt_pos;
        let attrs_str = &rest[attr_start..attr_end].trim_end_matches('/').trim();

        return Some(TagInfo {
            tag_name: tag_name.to_string(),
            attrs: attrs_str.to_string(),
            children: String::new(),
            start,
            end: start + gt_pos + 1,
        });
    }

    // Paired tag: find matching closing tag </tag-name>
    let content_start = start + gt_pos + 1;
    let close_tag = format!("</{}>", tag_name);

    let remaining = &template[content_start..];
    let close_pos = remaining.find(&close_tag)?;

    let attrs_raw = &rest[tag_name.len() + 1..gt_pos];
    let children = remaining[..close_pos].to_string();

    Some(TagInfo {
        tag_name: tag_name.to_string(),
        attrs: attrs_raw.trim().to_string(),
        children,
        start,
        end: content_start + close_pos + close_tag.len(),
    })
}

// ─── Props ──────────────────────────────────────────────────────────────

/// Parse `:prop="expr"` attributes and resolve them against parent data.
fn parse_props(attrs: &str, parent_data: &Value) -> Value {
    let re = Regex::new(r#":(\w+)="([^"]*)""#).unwrap();
    let mut map = serde_json::Map::new();

    for cap in re.captures_iter(attrs) {
        let key = &cap[1];
        let expr = &cap[2];
        let value_str = resolve_json_path(parent_data, expr);
        map.insert(key.to_string(), Value::String(value_str));
    }

    Value::Object(map)
}

// ─── Slots ──────────────────────────────────────────────────────────────

/// Parsed slot content keyed by slot name ("default" for unnamed).
type SlotMap = HashMap<String, String>;

/// Result of parsing slot content, including collected styles from resolved components.
struct SlotResult {
    slots: SlotMap,
    styles: Vec<String>,
}

/// Parse `<template #name>...</template>` blocks and default content from children.
fn parse_slot_content(
    children: &str,
    parent_data: &Value,
    parent_imports: &[VanImport],
    current_path: &str,
    files: &HashMap<String, String>,
    depth: usize,
    reactive_names: &[String],
    debug: bool,
    file_origins: &HashMap<String, String>,
) -> Result<SlotResult, String> {
    let mut slots = SlotMap::new();
    let mut styles: Vec<String> = Vec::new();
    let mut default_parts: Vec<String> = Vec::new();
    let mut rest = children;

    let named_slot_re = Regex::new(r#"<template\s+#(\w+)\s*>"#).unwrap();

    loop {
        let Some(cap) = named_slot_re.captures(rest) else {
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                default_parts.push(trimmed.to_string());
            }
            break;
        };

        let full_match = cap.get(0).unwrap();
        let slot_name = cap[1].to_string();

        // Content before this named slot is default content
        let before = rest[..full_match.start()].trim();
        if !before.is_empty() {
            default_parts.push(before.to_string());
        }

        // Find closing </template>
        let after_open = &rest[full_match.end()..];
        let close_pos = after_open.find("</template>");
        let slot_content = if let Some(pos) = close_pos {
            let content = after_open[..pos].trim().to_string();
            rest = &after_open[pos + "</template>".len()..];
            content
        } else {
            let content = after_open.trim().to_string();
            rest = "";
            content
        };

        // Interpolate named slot content with parent data
        let interpolated = if !reactive_names.is_empty() {
            interpolate_skip_reactive(&slot_content, parent_data, reactive_names)
        } else {
            interpolate(&slot_content, parent_data)
        };
        slots.insert(slot_name, interpolated);
    }

    // Process default slot content: resolve any child components using parent's import context
    if !default_parts.is_empty() {
        let default_content = default_parts.join("\n");

        let parent_import_map: HashMap<String, &VanImport> = parent_imports
            .iter()
            .map(|imp| (imp.tag_name.clone(), imp))
            .collect();

        let resolved = resolve_slot_components(
            &default_content,
            parent_data,
            &parent_import_map,
            current_path,
            files,
            depth,
            reactive_names,
            debug,
            file_origins,
        )?;

        slots.insert("default".to_string(), resolved.html);
        styles.extend(resolved.styles);
    }

    Ok(SlotResult { slots, styles })
}

/// Resolve component tags within slot content using the parent's import context.
fn resolve_slot_components(
    content: &str,
    data: &Value,
    import_map: &HashMap<String, &VanImport>,
    current_path: &str,
    files: &HashMap<String, String>,
    depth: usize,
    reactive_names: &[String],
    debug: bool,
    file_origins: &HashMap<String, String>,
) -> Result<ResolvedComponent, String> {
    let mut result = content.to_string();
    let mut styles: Vec<String> = Vec::new();

    loop {
        let tag_match = find_component_tag(&result, import_map);
        let Some(tag_info) = tag_match else {
            break;
        };

        let imp = &import_map[&tag_info.tag_name];
        let resolved_key = resolve_virtual_path(current_path, &imp.path);
        let component_source = files
            .get(&resolved_key)
            .ok_or_else(|| format!("Component not found: {} (resolved from '{}')", resolved_key, imp.path))?;

        let child_data = parse_props(&tag_info.attrs, data);

        let child_resolved = resolve_recursive(
            component_source,
            &child_data,
            &resolved_key,
            files,
            depth + 1,
            reactive_names,
            debug,
            file_origins,
        )?;

        let with_slots = distribute_slots(&child_resolved.html, &HashMap::new(), debug, &HashMap::new());
        styles.extend(child_resolved.styles);

        let replacement = if debug {
            let theme_prefix = file_origins.get(&resolved_key)
                .map(|t| format!("[{t}] "))
                .unwrap_or_default();
            format!("<!-- START: {theme_prefix}{resolved_key} -->{with_slots}<!-- END: {theme_prefix}{resolved_key} -->")
        } else {
            with_slots
        };

        result = format!(
            "{}{}{}",
            &result[..tag_info.start],
            replacement,
            &result[tag_info.end..],
        );
    }

    // Interpolate remaining {{ }} with parent data (reactive-aware)
    let html = if !reactive_names.is_empty() {
        interpolate_skip_reactive(&result, data, reactive_names)
    } else {
        interpolate(&result, data)
    };

    Ok(ResolvedComponent {
        html,
        styles,
        script_setup: None,
        module_imports: Vec::new(),
    })
}

/// Replace `<slot />` and `<slot name="x">fallback</slot>` with provided content.
///
/// `slot_themes` maps slot_name → theme_name for debug comments.
/// Only shown for explicitly provided slots, not for fallback defaults.
fn distribute_slots(html: &str, slots: &SlotMap, debug: bool, slot_themes: &HashMap<String, String>) -> String {
    let mut result = html.to_string();

    // Helper: build theme prefix for a given slot
    let tp = |name: &str| -> String {
        slot_themes.get(name)
            .filter(|t| !t.is_empty())
            .map(|t| format!("[{t}] "))
            .unwrap_or_default()
    };

    // Handle named slots: <slot name="x">fallback</slot>
    let named_re = Regex::new(r#"<slot\s+name="(\w+)">([\s\S]*?)</slot>"#).unwrap();
    result = named_re
        .replace_all(&result, |caps: &regex::Captures| {
            let name = &caps[1];
            let fallback = &caps[2];
            let provided = slots.get(name);
            let content = provided
                .cloned()
                .unwrap_or_else(|| fallback.trim().to_string());
            if debug {
                let p = if provided.is_some() { tp(name) } else { String::new() };
                format!("<!-- START: {p}#{name} -->{content}<!-- END: {p}#{name} -->")
            } else {
                content
            }
        })
        .to_string();

    // Handle named self-closing slots: <slot name="x" />
    let named_sc_re = Regex::new(r#"<slot\s+name="(\w+)"\s*/>"#).unwrap();
    result = named_sc_re
        .replace_all(&result, |caps: &regex::Captures| {
            let name = &caps[1];
            let provided = slots.get(name);
            let content = provided.cloned().unwrap_or_default();
            if debug {
                let p = if provided.is_some() { tp(name) } else { String::new() };
                format!("<!-- START: {p}#{name} -->{content}<!-- END: {p}#{name} -->")
            } else {
                content
            }
        })
        .to_string();

    // Handle default slot: <slot /> (self-closing)
    let default_sc_re = Regex::new(r#"<slot\s*/>"#).unwrap();
    result = default_sc_re
        .replace_all(&result, |_: &regex::Captures| {
            let provided = slots.get("default");
            let content = provided.cloned().unwrap_or_default();
            if debug {
                let p = if provided.is_some() { tp("default") } else { String::new() };
                format!("<!-- START: {p}#default -->{content}<!-- END: {p}#default -->")
            } else {
                content
            }
        })
        .to_string();

    // Handle default slot with fallback: <slot>fallback</slot>
    let default_re = Regex::new(r#"<slot>([\s\S]*?)</slot>"#).unwrap();
    result = default_re
        .replace_all(&result, |caps: &regex::Captures| {
            let fallback = &caps[1];
            let provided = slots.get("default");
            let content = provided
                .cloned()
                .unwrap_or_else(|| fallback.trim().to_string());
            if debug {
                let p = if provided.is_some() { tp("default") } else { String::new() };
                format!("<!-- START: {p}#default -->{content}<!-- END: {p}#default -->")
            } else {
                content
            }
        })
        .to_string();

    result
}

/// Resolve a dot-separated path and return the raw JSON Value.
fn resolve_path_value<'a>(data: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = data;
    for key in path.split('.') {
        let key = key.trim();
        match current.get(key) {
            Some(v) => current = v,
            None => return None,
        }
    }
    Some(current)
}

/// Expand `v-for` directives by repeating elements for each array item.
fn expand_v_for(template: &str, data: &Value) -> String {
    let vfor_re = Regex::new(r#"<(\w[\w-]*)([^>]*)\sv-for="([^"]*)"([^>]*)>"#).unwrap();
    let mut result = template.to_string();

    for _ in 0..20 {
        let Some(cap) = vfor_re.captures(&result) else {
            break;
        };

        let full_match = cap.get(0).unwrap();
        let tag_name = &cap[1];
        let attrs_before = &cap[2];
        let vfor_expr = &cap[3];
        let attrs_after = &cap[4];

        let (item_var, index_var, array_expr) = parse_vfor_expr(vfor_expr);
        let open_tag_no_vfor = format!("<{}{}{}>", tag_name, attrs_before, attrs_after);
        let match_start = full_match.start();
        let after_open = full_match.end();
        let is_self_closing = result[match_start..after_open].trim_end_matches('>').ends_with('/');

        if is_self_closing {
            let sc_tag = format!("<{}{}{} />", tag_name, attrs_before, attrs_after);
            let array = resolve_path_value(data, &array_expr);
            let items = array.and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let mut expanded = String::new();
            for (idx, item) in items.iter().enumerate() {
                let mut item_data = data.clone();
                if let Value::Object(ref mut map) = item_data {
                    map.insert(item_var.clone(), item.clone());
                    if let Some(ref idx_var) = index_var {
                        map.insert(idx_var.clone(), Value::Number(idx.into()));
                    }
                }
                expanded.push_str(&interpolate(&sc_tag, &item_data));
            }
            result = format!("{}{}{}", &result[..match_start], expanded, &result[after_open..]);
            continue;
        }

        let close_tag = format!("</{}>", tag_name);
        let remaining = &result[after_open..];
        let close_pos = find_matching_close_tag(remaining, tag_name);
        let inner_content = remaining[..close_pos].to_string();
        let element_end = after_open + close_pos + close_tag.len();

        let array = resolve_path_value(data, &array_expr);
        let items = array.and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut expanded = String::new();
        for (idx, item) in items.iter().enumerate() {
            let mut item_data = data.clone();
            if let Value::Object(ref mut map) = item_data {
                map.insert(item_var.clone(), item.clone());
                if let Some(ref idx_var) = index_var {
                    map.insert(idx_var.clone(), Value::Number(idx.into()));
                }
            }
            let tag_interpolated = interpolate(&open_tag_no_vfor, &item_data);
            let inner_interpolated = interpolate(&inner_content, &item_data);
            expanded.push_str(&format!("{}{}</{}>", tag_interpolated, inner_interpolated, tag_name));
        }

        result = format!("{}{}{}", &result[..match_start], expanded, &result[element_end..]);
    }

    result
}

fn parse_vfor_expr(expr: &str) -> (String, Option<String>, String) {
    let parts: Vec<&str> = expr.splitn(2, " in ").collect();
    if parts.len() != 2 {
        return (expr.to_string(), None, String::new());
    }
    let lhs = parts[0].trim();
    let array_expr = parts[1].trim().to_string();
    if lhs.starts_with('(') && lhs.ends_with(')') {
        let inner = &lhs[1..lhs.len() - 1];
        let vars: Vec<&str> = inner.split(',').collect();
        let item_var = vars[0].trim().to_string();
        let index_var = vars.get(1).map(|v| v.trim().to_string());
        (item_var, index_var, array_expr)
    } else {
        (lhs.to_string(), None, array_expr)
    }
}

fn find_matching_close_tag(html: &str, tag_name: &str) -> usize {
    let open = format!("<{}", tag_name);
    let close = format!("</{}>", tag_name);
    let mut depth = 0;
    let mut pos = 0;
    while pos < html.len() {
        if html[pos..].starts_with(&close) {
            if depth == 0 {
                return pos;
            }
            depth -= 1;
            pos += close.len();
        } else if html[pos..].starts_with(&open) {
            let after = pos + open.len();
            if after < html.len() {
                let ch = html.as_bytes()[after] as char;
                if ch == ' ' || ch == '>' || ch == '/' || ch == '\n' || ch == '\t' {
                    depth += 1;
                }
            }
            pos += open.len();
        } else {
            pos += 1;
        }
    }
    html.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_reactive_names() {
        let script = r#"
const count = ref(0)
const doubled = computed(() => count * 2)
"#;
        let names = extract_reactive_names(script);
        assert_eq!(names, vec!["count", "doubled"]);
    }

    #[test]
    fn test_resolve_single_basic() {
        let source = r#"
<template>
  <h1>{{ title }}</h1>
</template>
"#;
        let data = json!({"title": "Hello"});
        let resolved = resolve_single(source, &data).unwrap();
        assert!(resolved.html.contains("<h1>Hello</h1>"));
        assert!(resolved.styles.is_empty());
        assert!(resolved.script_setup.is_none());
    }

    #[test]
    fn test_resolve_single_with_style() {
        let source = r#"
<template>
  <h1>Hello</h1>
</template>

<style scoped>
h1 { color: red; }
</style>
"#;
        let data = json!({});
        let resolved = resolve_single(source, &data).unwrap();
        assert_eq!(resolved.styles.len(), 1);
        assert!(resolved.styles[0].contains("color: red"));
    }

    #[test]
    fn test_resolve_single_reactive() {
        let source = r#"
<template>
  <p>Count: {{ count }}</p>
</template>

<script setup>
const count = ref(0)
</script>
"#;
        let data = json!({});
        let resolved = resolve_single(source, &data).unwrap();
        assert!(resolved.html.contains("{{ count }}"));
        assert!(resolved.script_setup.is_some());
    }

    // ─── Virtual path tests ─────────────────────────────────────────

    #[test]
    fn test_resolve_virtual_path_same_dir() {
        assert_eq!(
            resolve_virtual_path("index.van", "./hello.van"),
            "hello.van"
        );
    }

    #[test]
    fn test_resolve_virtual_path_parent_dir() {
        assert_eq!(
            resolve_virtual_path("pages/index.van", "../components/hello.van"),
            "components/hello.van"
        );
    }

    #[test]
    fn test_resolve_virtual_path_subdir() {
        assert_eq!(
            resolve_virtual_path("pages/index.van", "./sub.van"),
            "pages/sub.van"
        );
    }

    #[test]
    fn test_normalize_virtual_path() {
        assert_eq!(normalize_virtual_path("./hello.van"), "hello.van");
        assert_eq!(
            normalize_virtual_path("pages/../components/hello.van"),
            "components/hello.van"
        );
        assert_eq!(normalize_virtual_path("a/b/./c"), "a/b/c");
    }

    #[test]
    fn test_resolve_virtual_path_scoped_package() {
        // @scope/pkg paths should be returned as-is regardless of current file
        assert_eq!(
            resolve_virtual_path("pages/index.van", "@van-ui/button/button.van"),
            "@van-ui/button/button.van"
        );
        assert_eq!(
            resolve_virtual_path("index.van", "@van-ui/utils/format.ts"),
            "@van-ui/utils/format.ts"
        );
    }

    #[test]
    fn test_resolve_with_files_scoped_import() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <van-button :label="title" />
</template>

<script setup>
import VanButton from '@van-ui/button/button.van'
</script>
"#
            .to_string(),
        );
        // In-memory file map: key is "@van-ui/button/button.van"
        files.insert(
            "@van-ui/button/button.van".to_string(),
            r#"
<template>
  <button>{{ label }}</button>
</template>
"#
            .to_string(),
        );

        let data = json!({"title": "Click me"});
        let resolved = resolve_with_files("index.van", &files, &data).unwrap();
        assert!(resolved.html.contains("<button>Click me</button>"));
    }

    // ─── Multi-file resolve tests ───────────────────────────────────

    #[test]
    fn test_resolve_with_files_basic_import() {
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
"#
            .to_string(),
        );

        let data = json!({"title": "World"});
        let resolved = resolve_with_files("index.van", &files, &data).unwrap();
        assert!(resolved.html.contains("<h1>Hello, World!</h1>"));
    }

    #[test]
    fn test_resolve_with_files_missing_component() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <hello />
</template>

<script setup>
import Hello from './hello.van'
</script>
"#
            .to_string(),
        );

        let data = json!({});
        let result = resolve_with_files("index.van", &files, &data);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Component not found"));
    }

    #[test]
    fn test_resolve_with_files_slots() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <wrapper>
    <p>Default slot content</p>
  </wrapper>
</template>

<script setup>
import Wrapper from './wrapper.van'
</script>
"#
            .to_string(),
        );
        files.insert(
            "wrapper.van".to_string(),
            r#"
<template>
  <div class="wrapper"><slot /></div>
</template>
"#
            .to_string(),
        );

        let data = json!({});
        let resolved = resolve_with_files("index.van", &files, &data).unwrap();
        assert!(resolved.html.contains("<div class=\"wrapper\">"));
        assert!(resolved.html.contains("<p>Default slot content</p>"));
    }

    #[test]
    fn test_resolve_with_files_styles_collected() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <hello />
</template>

<script setup>
import Hello from './hello.van'
</script>

<style>
.app { color: blue; }
</style>
"#
            .to_string(),
        );
        files.insert(
            "hello.van".to_string(),
            r#"
<template>
  <h1>Hello</h1>
</template>

<style>
h1 { color: red; }
</style>
"#
            .to_string(),
        );

        let data = json!({});
        let resolved = resolve_with_files("index.van", &files, &data).unwrap();
        assert_eq!(resolved.styles.len(), 2);
        assert!(resolved.styles[0].contains("color: blue"));
        assert!(resolved.styles[1].contains("color: red"));
    }

    #[test]
    fn test_resolve_with_files_reactive_preserved() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <div>
    <p>Count: {{ count }}</p>
    <hello :name="title" />
  </div>
</template>

<script setup>
import Hello from './hello.van'
const count = ref(0)
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
"#
            .to_string(),
        );

        let data = json!({"title": "World"});
        let resolved = resolve_with_files("index.van", &files, &data).unwrap();
        // Reactive expression should be preserved
        assert!(resolved.html.contains("{{ count }}"));
        // Non-reactive prop should be interpolated
        assert!(resolved.html.contains("<h1>Hello, World!</h1>"));
        assert!(resolved.script_setup.is_some());
    }

    // ─── Component tag extraction tests ─────────────────────────────

    #[test]
    fn test_extract_self_closing_tag() {
        let template = r#"<div><hello :name="title" /></div>"#;
        let info = extract_component_tag(template, "hello").unwrap();
        assert_eq!(info.tag_name, "hello");
        assert_eq!(info.attrs, r#":name="title""#);
        assert!(info.children.is_empty());
    }

    #[test]
    fn test_extract_paired_tag() {
        let template = r#"<default-layout><h1>Content</h1></default-layout>"#;
        let info = extract_component_tag(template, "default-layout").unwrap();
        assert_eq!(info.tag_name, "default-layout");
        assert_eq!(info.children, "<h1>Content</h1>");
    }

    #[test]
    fn test_extract_no_match() {
        let template = r#"<div>no components here</div>"#;
        assert!(extract_component_tag(template, "hello").is_none());
    }

    #[test]
    fn test_parse_props() {
        let data = json!({"title": "World", "count": 42});
        let attrs = r#":name="title" :num="count""#;
        let result = parse_props(attrs, &data);
        assert_eq!(result["name"], "World");
        assert_eq!(result["num"], "42");
    }

    #[test]
    fn test_distribute_slots_default() {
        let html = r#"<div><slot /></div>"#;
        let mut slots = HashMap::new();
        slots.insert("default".to_string(), "Hello World".to_string());
        let result = distribute_slots(html, &slots, false, &HashMap::new());
        assert_eq!(result, "<div>Hello World</div>");
    }

    #[test]
    fn test_distribute_slots_named() {
        let html =
            r#"<title><slot name="title">Fallback</slot></title><div><slot /></div>"#;
        let mut slots = HashMap::new();
        slots.insert("title".to_string(), "My Title".to_string());
        slots.insert("default".to_string(), "Body".to_string());
        let result = distribute_slots(html, &slots, false, &HashMap::new());
        assert_eq!(result, "<title>My Title</title><div>Body</div>");
    }

    #[test]
    fn test_distribute_slots_fallback() {
        let html = r#"<title><slot name="title">Fallback Title</slot></title>"#;
        let slots = HashMap::new();
        let result = distribute_slots(html, &slots, false, &HashMap::new());
        assert_eq!(result, "<title>Fallback Title</title>");
    }

    #[test]
    fn test_expand_v_for_basic() {
        let data = json!({"items": ["Alice", "Bob", "Charlie"]});
        let template = r#"<ul><li v-for="item in items">{{ item }}</li></ul>"#;
        let result = expand_v_for(template, &data);
        assert!(result.contains("<li>Alice</li>"));
        assert!(result.contains("<li>Bob</li>"));
        assert!(result.contains("<li>Charlie</li>"));
        assert!(!result.contains("v-for"));
    }

    #[test]
    fn test_expand_v_for_with_index() {
        let data = json!({"items": ["A", "B"]});
        let template = r#"<ul><li v-for="(item, index) in items">{{ index }}: {{ item }}</li></ul>"#;
        let result = expand_v_for(template, &data);
        assert!(result.contains("0: A"));
        assert!(result.contains("1: B"));
    }

    #[test]
    fn test_expand_v_for_nested_path() {
        let data = json!({"user": {"hobbies": ["coding", "reading"]}});
        let template = r#"<span v-for="h in user.hobbies">{{ h }}</span>"#;
        let result = expand_v_for(template, &data);
        assert!(result.contains("<span>coding</span>"));
        assert!(result.contains("<span>reading</span>"));
    }

    // ─── Scoped style tests ──────────────────────────────────────────

    #[test]
    fn test_resolve_scoped_style_single() {
        let source = r#"
<template>
  <div class="card"><h1>{{ title }}</h1></div>
</template>

<style scoped>
.card { border: 1px solid; }
h1 { color: navy; }
</style>
"#;
        let data = json!({"title": "Hello"});
        let css = ".card { border: 1px solid; }\nh1 { color: navy; }";
        let id = van_parser::scope_id(css);
        let resolved = resolve_single_with_path(source, &data, "components/card.van").unwrap();
        // All elements should have scope class
        assert!(resolved.html.contains(&format!("class=\"card {id}\"")), "Root should have scope class appended");
        assert!(resolved.html.contains(&format!("class=\"{id}\"")), "Child h1 should have scope class");
        // CSS selectors should have .{id} appended
        assert_eq!(resolved.styles.len(), 1);
        assert!(resolved.styles[0].contains(&format!(".card.{id}")));
        assert!(resolved.styles[0].contains(&format!("h1.{id}")));
    }

    #[test]
    fn test_resolve_scoped_style_multi_file() {
        let mut files = HashMap::new();
        files.insert(
            "index.van".to_string(),
            r#"
<template>
  <card :title="title" />
</template>

<script setup>
import Card from './card.van'
</script>
"#.to_string(),
        );
        files.insert(
            "card.van".to_string(),
            r#"
<template>
  <div class="card"><h1>{{ title }}</h1></div>
</template>

<style scoped>
.card { border: 1px solid; }
</style>
"#.to_string(),
        );

        let data = json!({"title": "Test"});
        let id = van_parser::scope_id(".card { border: 1px solid; }");
        let resolved = resolve_with_files("index.van", &files, &data).unwrap();
        // Child component HTML should have scope class on all elements
        assert!(resolved.html.contains(&format!("card {id}")), "Should contain scope class");
        // CSS selectors should have .{id} appended
        assert_eq!(resolved.styles.len(), 1);
        assert!(resolved.styles[0].contains(&format!(".card.{id}")));
    }

    #[test]
    fn test_resolve_unscoped_style_unchanged() {
        let source = r#"
<template>
  <div class="app"><p>Hello</p></div>
</template>

<style>
.app { margin: 0; }
</style>
"#;
        let data = json!({});
        let resolved = resolve_single(source, &data).unwrap();
        // HTML should be unchanged — no extra scope classes
        assert_eq!(resolved.html.matches("class=").count(), 1, "Only the original class attr");
        assert!(resolved.html.contains("class=\"app\""), "Original class preserved");
        assert_eq!(resolved.styles[0], ".app { margin: 0; }");
    }
}
