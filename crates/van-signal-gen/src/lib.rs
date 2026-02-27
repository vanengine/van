use std::collections::HashMap;
use regex::Regex;

/// The embedded signal runtime JS (~1KB).
pub const RUNTIME_JS: &str = include_str!("runtime.js");

/// Extract initial values of `ref()` signals from a `<script setup>` block.
///
/// Returns a map of `signal_name → display_value` suitable for HTML interpolation.
/// JS literals are converted to plain strings: `0` → `"0"`, `'hello'` → `"hello"`.
pub fn extract_initial_values(script: &str) -> HashMap<String, String> {
    let analysis = analyze_script(script);
    let mut values = HashMap::new();
    for signal in &analysis.signals {
        values.insert(
            signal.name.clone(),
            js_literal_to_display(&signal.initial_value),
        );
    }
    values
}

/// Convert a JS literal to a display string for SSR.
fn js_literal_to_display(literal: &str) -> String {
    let s = literal.trim();
    match s {
        "true" => "true".to_string(),
        "false" => "false".to_string(),
        "null" | "undefined" | "" => String::new(),
        _ if (s.starts_with('\'') && s.ends_with('\''))
            || (s.starts_with('"') && s.ends_with('"')) =>
        {
            s[1..s.len() - 1].to_string()
        }
        _ => s.to_string(), // numbers and other literals as-is
    }
}

// ── Stage A: Script Analysis ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct SignalDecl {
    pub name: String,
    pub initial_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedDecl {
    pub name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub params: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptAnalysis {
    pub signals: Vec<SignalDecl>,
    pub computeds: Vec<ComputedDecl>,
    pub functions: Vec<FunctionDecl>,
    pub watches: Vec<WatchDecl>,
}

/// Parse `<script setup>` content to extract reactive declarations.
pub fn analyze_script(script: &str) -> ScriptAnalysis {
    let mut signals = Vec::new();
    let mut computeds = Vec::new();
    let mut functions = Vec::new();

    // Match: const x = ref(value)
    let ref_re = Regex::new(r#"(?m)const\s+(\w+)\s*=\s*ref\(([^)]*)\)"#).unwrap();
    for cap in ref_re.captures_iter(script) {
        signals.push(SignalDecl {
            name: cap[1].to_string(),
            initial_value: cap[2].trim().to_string(),
        });
    }

    // Match: const x = computed(() => expr)
    let computed_re =
        Regex::new(r#"(?m)const\s+(\w+)\s*=\s*computed\(\s*\(\)\s*=>\s*(.+?)\s*\)"#).unwrap();
    for cap in computed_re.captures_iter(script) {
        computeds.push(ComputedDecl {
            name: cap[1].to_string(),
            body: cap[2].trim().to_string(),
        });
    }

    // Match: function name(args) { body }
    let func_re =
        Regex::new(r#"(?m)function\s+(\w+)\s*\(([^)]*)\)\s*\{([^}]*)\}"#).unwrap();
    for cap in func_re.captures_iter(script) {
        functions.push(FunctionDecl {
            name: cap[1].to_string(),
            params: cap[2].trim().to_string(),
            body: cap[3].trim().to_string(),
        });
    }

    // Match: watch(source, function(params) { body }) or watch(source, (params) => { body })
    let mut watches = Vec::new();
    let watch_fn_re = Regex::new(
        r#"(?m)watch\(\s*(\w+)\s*,\s*function\s*\(([^)]*)\)\s*\{([^}]*)\}\s*\)"#,
    )
    .unwrap();
    for cap in watch_fn_re.captures_iter(script) {
        watches.push(WatchDecl {
            source: cap[1].to_string(),
            params: cap[2].trim().to_string(),
            body: cap[3].trim().to_string(),
        });
    }
    let watch_arrow_re = Regex::new(
        r#"(?m)watch\(\s*(\w+)\s*,\s*\(([^)]*)\)\s*=>\s*\{([^}]*)\}\s*\)"#,
    )
    .unwrap();
    for cap in watch_arrow_re.captures_iter(script) {
        watches.push(WatchDecl {
            source: cap[1].to_string(),
            params: cap[2].trim().to_string(),
            body: cap[3].trim().to_string(),
        });
    }

    ScriptAnalysis {
        signals,
        computeds,
        functions,
        watches,
    }
}

// ── Stage B: HTML Tree Walker ───────────────────────────────────────────────
//
// Builds a minimal element tree from resolved HTML (which still contains
// @event, v-show/v-if, and {{ reactive }} expressions). Walks the tree
// collecting bindings with positional paths (Vec<usize> of element-child
// indices, like .children[0].children[2]).

#[derive(Debug, Clone)]
struct HtmlElement {
    tag: String,
    attrs: Vec<(String, String)>,
    children: Vec<HtmlNode>,
}

#[derive(Debug, Clone)]
enum HtmlNode {
    Element(HtmlElement),
    Text(String),
}

/// A binding for `@event="handler"` with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct EventBinding {
    pub path: Vec<usize>,
    pub event: String,
    pub handler: String,
}

/// A binding for `{{ reactiveExpr }}` text content with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBinding {
    pub path: Vec<usize>,
    pub template: String,
}

/// A binding for `v-show="expr"` or `v-if="expr"` with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowBinding {
    pub path: Vec<usize>,
    pub expr: String,
    pub transition: Option<String>,
}

/// A binding for `v-html="expr"` with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlDirectiveBinding {
    pub path: Vec<usize>,
    pub expr: String,
}

/// A binding for `v-text="expr"` with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct TextDirectiveBinding {
    pub path: Vec<usize>,
    pub expr: String,
}

/// A binding for `:class="{ ... }"` with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassBinding {
    pub path: Vec<usize>,
    pub expr: String,
}

/// A binding for `:style="{ ... }"` with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleBinding {
    pub path: Vec<usize>,
    pub expr: String,
}

/// A binding for `v-model="signalName"` with its positional path.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelBinding {
    pub path: Vec<usize>,
    pub signal_name: String,
}

/// A watch() declaration from script setup.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchDecl {
    pub source: String,
    pub params: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateBindings {
    pub events: Vec<EventBinding>,
    pub texts: Vec<TextBinding>,
    pub shows: Vec<ShowBinding>,
    pub htmls: Vec<HtmlDirectiveBinding>,
    pub text_directives: Vec<TextDirectiveBinding>,
    pub classes: Vec<ClassBinding>,
    pub styles: Vec<StyleBinding>,
    pub models: Vec<ModelBinding>,
}

/// Parse HTML string into a list of HtmlNode.
fn parse_html(html: &str) -> Vec<HtmlNode> {
    let mut nodes = Vec::new();
    let mut pos = 0;
    let bytes = html.as_bytes();

    while pos < bytes.len() {
        if bytes[pos] == b'<' {
            // Check for closing tag, comment, or doctype — skip as non-element
            if pos + 1 < bytes.len() && (bytes[pos + 1] == b'/' || bytes[pos + 1] == b'!') {
                if bytes[pos + 1] == b'!' {
                    // Skip comment/doctype — find closing >
                    if let Some(end) = html[pos..].find('>') {
                        pos = pos + end + 1;
                    } else {
                        pos = bytes.len();
                    }
                } else {
                    // Closing tag — shouldn't appear at top level if well-formed
                    // but just skip it
                    if let Some(end) = html[pos..].find('>') {
                        pos = pos + end + 1;
                    } else {
                        pos = bytes.len();
                    }
                }
                continue;
            }

            // Opening tag — parse element
            if let Some((elem, end_pos)) = parse_element(html, pos) {
                nodes.push(HtmlNode::Element(elem));
                pos = end_pos;
            } else {
                // Failed to parse as element — treat as text
                let text_end = html[pos + 1..].find('<').map(|p| pos + 1 + p).unwrap_or(bytes.len());
                let text = &html[pos..text_end];
                if !text.trim().is_empty() {
                    nodes.push(HtmlNode::Text(text.to_string()));
                }
                pos = text_end;
            }
        } else {
            // Text node — collect until next '<'
            let text_end = html[pos..].find('<').map(|p| pos + p).unwrap_or(bytes.len());
            let text = &html[pos..text_end];
            if !text.trim().is_empty() {
                nodes.push(HtmlNode::Text(text.to_string()));
            }
            pos = text_end;
        }
    }

    nodes
}

/// Void elements that never have closing tags.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input",
    "link", "meta", "param", "source", "track", "wbr",
];

/// Parse a single element starting at `pos` (which points to '<').
/// Returns the element and the position after its closing tag.
fn parse_element(html: &str, pos: usize) -> Option<(HtmlElement, usize)> {
    let rest = &html[pos..];
    if !rest.starts_with('<') {
        return None;
    }

    // Find end of opening tag
    let gt_pos = rest.find('>')?;
    let tag_content = &rest[1..gt_pos];

    // Self-closing?
    let self_closing = tag_content.ends_with('/');
    let tag_content = if self_closing {
        &tag_content[..tag_content.len() - 1]
    } else {
        tag_content
    };

    // Extract tag name
    let tag_end = tag_content.find(|c: char| c.is_whitespace()).unwrap_or(tag_content.len());
    let tag_name = tag_content[..tag_end].to_lowercase();

    if tag_name.is_empty() {
        return None;
    }

    // Parse attributes
    let attrs = parse_attrs(&tag_content[tag_end..]);

    let after_open = pos + gt_pos + 1;

    // Void elements and self-closing tags have no children
    if self_closing || VOID_ELEMENTS.contains(&tag_name.as_str()) {
        return Some((
            HtmlElement {
                tag: tag_name,
                attrs,
                children: Vec::new(),
            },
            after_open,
        ));
    }

    // Parse children until we find the closing tag
    let close_tag = format!("</{}>", tag_name);
    let children = parse_children_until(html, after_open, &close_tag);
    let (child_nodes, end_pos) = children;

    Some((
        HtmlElement {
            tag: tag_name,
            attrs,
            children: child_nodes,
        },
        end_pos,
    ))
}

/// Parse children from `start` until we encounter `close_tag`.
/// Returns the children and position after the close tag.
fn parse_children_until(html: &str, start: usize, close_tag: &str) -> (Vec<HtmlNode>, usize) {
    let mut nodes = Vec::new();
    let mut pos = start;
    let bytes = html.as_bytes();

    while pos < bytes.len() {
        // Check if we've reached the closing tag
        if html[pos..].starts_with(close_tag) {
            return (nodes, pos + close_tag.len());
        }

        if bytes[pos] == b'<' {
            // Comment or doctype
            if pos + 1 < bytes.len() && bytes[pos + 1] == b'!' {
                if let Some(end) = html[pos..].find('>') {
                    pos = pos + end + 1;
                } else {
                    pos = bytes.len();
                }
                continue;
            }

            // Closing tag for something else? Skip it.
            if pos + 1 < bytes.len() && bytes[pos + 1] == b'/' {
                if let Some(end) = html[pos..].find('>') {
                    pos = pos + end + 1;
                } else {
                    pos = bytes.len();
                }
                continue;
            }

            // Opening tag — try to parse as child element
            if let Some((elem, end_pos)) = parse_element(html, pos) {
                nodes.push(HtmlNode::Element(elem));
                pos = end_pos;
            } else {
                // Can't parse — treat as text
                let text_end = html[pos + 1..].find('<').map(|p| pos + 1 + p).unwrap_or(bytes.len());
                let text = &html[pos..text_end];
                if !text.trim().is_empty() {
                    nodes.push(HtmlNode::Text(text.to_string()));
                }
                pos = text_end;
            }
        } else {
            // Text node
            let text_end = html[pos..].find('<').map(|p| pos + p).unwrap_or(bytes.len());
            let text = &html[pos..text_end];
            if !text.trim().is_empty() {
                nodes.push(HtmlNode::Text(text.to_string()));
            }
            pos = text_end;
        }
    }

    // Ran out of input without finding close tag — return what we have
    (nodes, pos)
}

/// Parse attributes from a tag's attribute string.
/// Handles: `key="value"`, `key='value'`, `key`, and directives like `@click="handler"`.
fn parse_attrs(attr_str: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let s = attr_str.trim();
    if s.is_empty() {
        return attrs;
    }

    let mut pos = 0;
    let bytes = s.as_bytes();

    while pos < bytes.len() {
        // Skip whitespace
        while pos < bytes.len() && (bytes[pos] as char).is_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }

        // Read attribute name (may include @, v-, :, etc.)
        let name_start = pos;
        while pos < bytes.len() && bytes[pos] != b'=' && !(bytes[pos] as char).is_whitespace() && bytes[pos] != b'>' {
            pos += 1;
        }
        let name = s[name_start..pos].to_string();
        if name.is_empty() {
            pos += 1;
            continue;
        }

        // Check for = (attribute value)
        if pos < bytes.len() && bytes[pos] == b'=' {
            pos += 1; // skip '='
            if pos < bytes.len() && (bytes[pos] == b'"' || bytes[pos] == b'\'') {
                let quote = bytes[pos];
                pos += 1;
                let val_start = pos;
                while pos < bytes.len() && bytes[pos] != quote {
                    pos += 1;
                }
                let val = s[val_start..pos].to_string();
                if pos < bytes.len() {
                    pos += 1; // skip closing quote
                }
                attrs.push((name, val));
            } else {
                // Unquoted value
                let val_start = pos;
                while pos < bytes.len() && !(bytes[pos] as char).is_whitespace() {
                    pos += 1;
                }
                attrs.push((name, s[val_start..pos].to_string()));
            }
        } else {
            // Boolean attribute (no value)
            attrs.push((name, String::new()));
        }
    }

    attrs
}

/// Walk the HTML tree and collect bindings with positional paths.
/// If the HTML contains `<body>`, paths are relative to body's children.
pub fn walk_template(html: &str, reactive_names: &[&str]) -> TemplateBindings {
    let nodes = parse_html(html);

    let mut bindings = TemplateBindings {
        events: Vec::new(),
        texts: Vec::new(),
        shows: Vec::new(),
        htmls: Vec::new(),
        text_directives: Vec::new(),
        classes: Vec::new(),
        styles: Vec::new(),
        models: Vec::new(),
    };

    // Check if there's a <body> element — if so, walk its children
    if let Some(body) = find_body(&nodes) {
        walk_children(&body.children, &[], reactive_names, &mut bindings);
    } else {
        // No body — treat top-level nodes as body children
        walk_children(&nodes, &[], reactive_names, &mut bindings);
    }

    bindings
}

/// Find the <body> element in the tree (may be nested in <html>).
fn find_body(nodes: &[HtmlNode]) -> Option<&HtmlElement> {
    for node in nodes {
        if let HtmlNode::Element(elem) = node {
            if elem.tag == "body" {
                return Some(elem);
            }
            // Recurse into <html>
            if elem.tag == "html" {
                if let Some(body) = find_body(&elem.children) {
                    return Some(body);
                }
            }
        }
    }
    None
}

/// Walk child nodes, tracking element-child-index at each level.
/// `path` is the current path of element indices from the root.
fn walk_children(
    children: &[HtmlNode],
    path: &[usize],
    reactive_names: &[&str],
    bindings: &mut TemplateBindings,
) {
    let mut element_index = 0;
    walk_nodes(children, path, reactive_names, bindings, &mut element_index, None);
}

/// Internal walker that shares a mutable element index counter.
/// `transition` carries the `name` attribute from a parent `<Transition>` wrapper.
/// When inside a `<Transition>`, child elements inherit the parent index counter
/// and path — the `<Transition>` tag itself does NOT count as a DOM element.
fn walk_nodes(
    children: &[HtmlNode],
    path: &[usize],
    reactive_names: &[&str],
    bindings: &mut TemplateBindings,
    element_index: &mut usize,
    transition: Option<&str>,
) {
    for node in children {
        match node {
            HtmlNode::Element(elem) => {
                if elem.tag == "transition" {
                    // <Transition> is not a real DOM element — skip it in the path.
                    // Extract the `name` attribute to pass to children; default to "v".
                    let name = elem.attrs.iter()
                        .find(|(k, _)| k == "name")
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("v");
                    // Recurse into children, sharing the same index counter and path
                    walk_nodes(&elem.children, path, reactive_names, bindings, element_index, Some(name));
                    continue;
                }

                let mut current_path = path.to_vec();
                current_path.push(*element_index);

                // Check for @event attributes
                for (name, value) in &elem.attrs {
                    if let Some(event) = name.strip_prefix('@') {
                        bindings.events.push(EventBinding {
                            path: current_path.clone(),
                            event: event.to_string(),
                            handler: value.clone(),
                        });
                    }
                    if name == "v-show" || name == "v-if" || name == "v-else-if" {
                        bindings.shows.push(ShowBinding {
                            path: current_path.clone(),
                            expr: value.clone(),
                            transition: transition.map(|s| s.to_string()),
                        });
                    }
                    if name == "v-else" {
                        bindings.shows.push(ShowBinding {
                            path: current_path.clone(),
                            expr: "true".to_string(),
                            transition: transition.map(|s| s.to_string()),
                        });
                    }
                    if name == "v-html" {
                        bindings.htmls.push(HtmlDirectiveBinding {
                            path: current_path.clone(),
                            expr: value.clone(),
                        });
                    }
                    if name == "v-text" {
                        bindings.text_directives.push(TextDirectiveBinding {
                            path: current_path.clone(),
                            expr: value.clone(),
                        });
                    }
                    if name == ":class" {
                        bindings.classes.push(ClassBinding {
                            path: current_path.clone(),
                            expr: value.clone(),
                        });
                    }
                    if name == ":style" {
                        bindings.styles.push(StyleBinding {
                            path: current_path.clone(),
                            expr: value.clone(),
                        });
                    }
                    if name == "v-model" {
                        bindings.models.push(ModelBinding {
                            path: current_path.clone(),
                            signal_name: value.clone(),
                        });
                    }
                }

                // Check if this element has text children with reactive {{ expr }}
                check_text_bindings(elem, &current_path, reactive_names, bindings);

                // Recurse into children (reset index for a new level)
                walk_children(&elem.children, &current_path, reactive_names, bindings);

                *element_index += 1;
            }
            HtmlNode::Text(_) => {
                // Text nodes don't count as element children for .children[N]
            }
        }
    }
}

/// Check if an element's direct text content contains reactive {{ expr }}.
/// If so, record a TextBinding for the element.
fn check_text_bindings(
    elem: &HtmlElement,
    path: &[usize],
    reactive_names: &[&str],
    bindings: &mut TemplateBindings,
) {
    // Collect all text content of this element's direct children
    let mut full_text = String::new();
    let mut has_only_text = true;

    for child in &elem.children {
        match child {
            HtmlNode::Text(text) => {
                full_text.push_str(text);
            }
            HtmlNode::Element(_) => {
                has_only_text = false;
            }
        }
    }

    // Only process if the element contains text-only content with {{ }}
    if !has_only_text || !full_text.contains("{{") {
        return;
    }

    // Check if any {{ expr }} contains a reactive name
    let re = Regex::new(r"\{\{\s*([^}]+?)\s*\}\}").unwrap();
    let has_reactive = re.captures_iter(&full_text).any(|cap| {
        let expr = cap[1].trim();
        is_reactive_expr(expr, reactive_names)
    });

    if has_reactive {
        bindings.texts.push(TextBinding {
            path: path.to_vec(),
            template: full_text.trim().to_string(),
        });
    }
}

/// Check if an expression references any reactive name.
fn is_reactive_expr(expr: &str, reactive_names: &[&str]) -> bool {
    reactive_names.iter().any(|name| {
        let bytes = expr.as_bytes();
        let name_bytes = name.as_bytes();
        let name_len = name.len();
        let mut i = 0;
        while i + name_len <= bytes.len() {
            if &bytes[i..i + name_len] == name_bytes {
                let before_ok = i == 0 || !(bytes[i - 1] as char).is_alphanumeric() && bytes[i - 1] != b'_';
                let after_ok = i + name_len == bytes.len()
                    || !(bytes[i + name_len] as char).is_alphanumeric() && bytes[i + name_len] != b'_';
                if before_ok && after_ok {
                    return true;
                }
            }
            i += 1;
        }
        false
    })
}

// ── Stage C: Positional JS Code Generation ──────────────────────────────────

/// Transform a script expression from Vue-style to signal JS.
///
/// Converts `x` → `x.value` and `x.value` stays as-is for reactive names.
fn transform_expr(expr: &str, reactive_names: &[&str]) -> String {
    let mut result = expr.to_string();

    for name in reactive_names {
        // Replace `name` with `name.value` but not if already `name.value`
        let dot_value = format!("{}.value", name);
        // First, temporarily replace existing .value references
        let placeholder = format!("__PLACEHOLDER_{name}__");
        result = result.replace(&dot_value, &placeholder);

        // Now replace bare name references (word boundary)
        let re = Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap();
        result = re.replace_all(&result, &dot_value).to_string();

        // Restore the placeholders (they would have become name.value.value)
        let double_value = format!("{}.value.value", name);
        result = result.replace(&double_value, &dot_value);
        result = result.replace(&placeholder, &dot_value);
    }

    result
}

/// Convert a text template like `"Count: {{ count }}"` to a JS expression
/// like `'Count: ' + count.value`.
fn template_to_js_expr(template: &str, reactive_names: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let before = &rest[..start];
        if !before.is_empty() {
            // Escape single quotes in literal text
            let escaped = before.replace('\\', "\\\\").replace('\'', "\\'");
            parts.push(format!("'{}'", escaped));
        }
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find("}}") {
            let expr = after_open[..end].trim();
            let transformed = transform_expr(expr, reactive_names);
            parts.push(transformed);
            rest = &after_open[end + 2..];
        } else {
            // No closing }} — treat rest as literal
            let escaped = rest.replace('\\', "\\\\").replace('\'', "\\'");
            parts.push(format!("'{}'", escaped));
            rest = "";
            break;
        }
    }

    if !rest.is_empty() {
        let escaped = rest.replace('\\', "\\\\").replace('\'', "\\'");
        parts.push(format!("'{}'", escaped));
    }

    if parts.is_empty() {
        "''".to_string()
    } else {
        parts.join(" + ")
    }
}

/// A single `:class` binding item: either a conditional toggle or a static class name.
#[derive(Debug, Clone, PartialEq)]
enum ClassItem {
    /// `{ active: isActive }` → toggle class based on condition
    Toggle(String, String),
    /// `'static-class'` → unconditionally add class
    Static(String),
}

/// Parse a `:class` expression, dispatching based on syntax:
/// - `{ ... }` → object syntax (existing)
/// - `[{ ... }, 'static']` → array syntax (new)
fn parse_class_expr(expr: &str) -> Vec<ClassItem> {
    let trimmed = expr.trim();
    if trimmed.starts_with('[') {
        parse_class_array(trimmed)
    } else if trimmed.starts_with('{') {
        parse_class_object(trimmed)
            .into_iter()
            .map(|(name, cond)| ClassItem::Toggle(name, cond))
            .collect()
    } else {
        Vec::new()
    }
}

/// Parse a `:class` array expression like `[{ active: isActive }, 'bold']`.
fn parse_class_array(expr: &str) -> Vec<ClassItem> {
    let trimmed = expr.trim();
    let inner = match trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(s) => s,
        None => return Vec::new(),
    };

    let mut items = Vec::new();
    let parts = split_respecting_nesting(inner);

    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part.starts_with('{') {
            // Object item → delegate to parse_class_object
            for (name, cond) in parse_class_object(part) {
                items.push(ClassItem::Toggle(name, cond));
            }
        } else {
            // String literal → static class
            let class_name = part.trim_matches('\'').trim_matches('"');
            if !class_name.is_empty() {
                items.push(ClassItem::Static(class_name.to_string()));
            }
        }
    }

    items
}

/// Parse a `:class` object expression like `{ active: isActive, 'text-bold': isBold }`.
/// Returns a list of (class_name, condition_expr) pairs.
fn parse_class_object(expr: &str) -> Vec<(String, String)> {
    let trimmed = expr.trim();
    let inner = if trimmed.starts_with('{') && trimmed.ends_with('}') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        return Vec::new();
    };

    let mut pairs = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon_pos) = part.find(':') {
            let key = part[..colon_pos].trim().trim_matches('\'').trim_matches('"');
            let val = part[colon_pos + 1..].trim();
            pairs.push((key.to_string(), val.to_string()));
        }
    }
    pairs
}

/// Parse a `:style` expression, dispatching based on syntax:
/// - `{ ... }` → object syntax (existing)
/// - `[{ ... }, { ... }]` → array syntax (new) — flattens all pairs
fn parse_style_expr(expr: &str) -> Vec<(String, String)> {
    let trimmed = expr.trim();
    if trimmed.starts_with('[') {
        parse_style_array(trimmed)
    } else if trimmed.starts_with('{') {
        parse_style_object(trimmed)
    } else {
        Vec::new()
    }
}

/// Parse a `:style` array expression like `[{ color: c }, { fontSize: s }]`.
fn parse_style_array(expr: &str) -> Vec<(String, String)> {
    let trimmed = expr.trim();
    let inner = match trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        Some(s) => s,
        None => return Vec::new(),
    };

    let mut pairs = Vec::new();
    let parts = split_respecting_nesting(inner);

    for part in parts {
        let part = part.trim();
        if part.starts_with('{') {
            pairs.extend(parse_style_object(part));
        }
    }

    pairs
}

/// Split a string by commas, respecting nested `{}` and `[]` blocks.
fn split_respecting_nesting(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = &s[start..];
    if !tail.trim().is_empty() {
        result.push(tail);
    }
    result
}

/// Parse a `:style` object expression like `{ color: textColor, fontSize: size }`.
/// Returns a list of (css_property, value_expr) pairs. camelCase keys are kept as-is
/// for `element.style.propName` assignment.
fn parse_style_object(expr: &str) -> Vec<(String, String)> {
    let trimmed = expr.trim();
    let inner = if trimmed.starts_with('{') && trimmed.ends_with('}') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        return Vec::new();
    };

    let mut pairs = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon_pos) = part.find(':') {
            let key = part[..colon_pos].trim().trim_matches('\'').trim_matches('"');
            let val = part[colon_pos + 1..].trim();
            // Keep camelCase for element.style.propName
            pairs.push((key.to_string(), val.to_string()));
        }
    }
    pairs
}

/// Collect all unique path prefixes that need JS variables.
/// Returns a sorted, deduplicated list of paths that are either:
/// - Direct binding targets (have an event, text, or show binding)
/// - Ancestors on the way to a binding target
fn collect_required_paths(bindings: &TemplateBindings) -> Vec<Vec<usize>> {
    let mut paths = std::collections::BTreeSet::new();

    let all_binding_paths: Vec<&Vec<usize>> = bindings
        .events
        .iter()
        .map(|b| &b.path)
        .chain(bindings.texts.iter().map(|b| &b.path))
        .chain(bindings.shows.iter().map(|b| &b.path))
        .chain(bindings.htmls.iter().map(|b| &b.path))
        .chain(bindings.text_directives.iter().map(|b| &b.path))
        .chain(bindings.classes.iter().map(|b| &b.path))
        .chain(bindings.styles.iter().map(|b| &b.path))
        .chain(bindings.models.iter().map(|b| &b.path))
        .collect();

    for path in &all_binding_paths {
        // Add the full path and all ancestor prefixes
        for i in 1..=path.len() {
            paths.insert(path[..i].to_vec());
        }
    }

    paths.into_iter().collect()
}

/// Strip import lines from script setup so they don't interfere with regex matching.
fn strip_imports(script: &str) -> String {
    let import_re = Regex::new(r#"(?m)^[ \t]*import\s+.*$"#).unwrap();
    import_re.replace_all(script, "").to_string()
}

/// Generate the signal JS for a page. Returns `None` if no reactive code found.
///
/// `module_code` contains resolved .ts/.js content (already transpiled to JS) to be
/// inlined before signal declarations. Each entry is wrapped in an IIFE.
pub fn generate_signals(script_setup: &str, template_html: &str, module_code: &[String]) -> Option<String> {
    let clean_script = strip_imports(script_setup);
    let analysis = analyze_script(&clean_script);

    // If nothing reactive, skip
    if analysis.signals.is_empty() && analysis.computeds.is_empty() {
        return None;
    }

    let reactive_names: Vec<&str> = analysis
        .signals
        .iter()
        .map(|s| s.name.as_str())
        .chain(analysis.computeds.iter().map(|c| c.name.as_str()))
        .collect();

    let bindings = walk_template(template_html, &reactive_names);

    // If no bindings found, still emit signals/functions but no DOM code
    let required_paths = collect_required_paths(&bindings);

    let mut js = String::new();
    js.push_str("(function() {\n");
    js.push_str("  var V = Van;\n");

    // Inlined module code
    for (i, code) in module_code.iter().enumerate() {
        js.push_str(&format!(
            "  var __mod_{} = (function() {{ {} }})();\n",
            i,
            code.trim()
        ));
    }

    // Signals
    for s in &analysis.signals {
        js.push_str(&format!(
            "  var {} = V.signal({});\n",
            s.name, s.initial_value
        ));
    }

    // Computeds
    for c in &analysis.computeds {
        let body = transform_expr(&c.body, &reactive_names);
        js.push_str(&format!(
            "  var {} = V.computed(function() {{ return {}; }});\n",
            c.name, body
        ));
    }

    // Functions
    for f in &analysis.functions {
        let body = transform_expr(&f.body, &reactive_names);
        js.push_str(&format!(
            "  function {}({}) {{ {} }}\n",
            f.name, f.params, body
        ));
    }

    // Watch declarations
    for w in &analysis.watches {
        let body = transform_expr(&w.body, &reactive_names);
        js.push_str(&format!(
            "  V.watch({}, function({}) {{ {} }});\n",
            w.source, w.params, body
        ));
    }

    // Positional DOM element variables
    if !required_paths.is_empty() {
        js.push_str("\n");
        // Build a map of path → variable name
        let mut path_vars: std::collections::HashMap<Vec<usize>, String> = std::collections::HashMap::new();
        let mut var_counter = 0;

        // Root is document.body
        js.push_str("  var _r = document.body;\n");

        for path in &required_paths {
            let var_name = format!("_e{}", var_counter);
            var_counter += 1;

            // Parent variable
            let parent_var = if path.len() == 1 {
                "_r".to_string()
            } else {
                let parent_path = &path[..path.len() - 1];
                path_vars.get(parent_path).cloned().unwrap_or_else(|| "_r".to_string())
            };

            let index = path[path.len() - 1];
            js.push_str(&format!(
                "  var {} = {}.children[{}];\n",
                var_name, parent_var, index
            ));

            path_vars.insert(path.clone(), var_name);
        }

        // Event bindings
        for binding in &bindings.events {
            let var = path_vars.get(&binding.path).unwrap();
            let handler_ref = if analysis.functions.iter().any(|f| f.name == binding.handler) {
                binding.handler.clone()
            } else {
                let body = transform_expr(&binding.handler, &reactive_names);
                format!("function() {{ {} }}", body)
            };
            js.push_str(&format!(
                "  {}.addEventListener('{}', {});\n",
                var, binding.event, handler_ref
            ));
        }

        // Text bindings (reactive text content)
        for binding in &bindings.texts {
            let var = path_vars.get(&binding.path).unwrap();
            let js_expr = template_to_js_expr(&binding.template, &reactive_names);
            js.push_str(&format!(
                "  V.effect(function() {{ {}.textContent = {}; }});\n",
                var, js_expr
            ));
        }

        // Show bindings
        for binding in &bindings.shows {
            let var = path_vars.get(&binding.path).unwrap();
            let transformed = transform_expr(&binding.expr, &reactive_names);
            if let Some(ref name) = binding.transition {
                js.push_str(&format!(
                    "  V.effect(function() {{ V.transition({}, {}, '{}'); }});\n",
                    var, transformed, name
                ));
            } else {
                js.push_str(&format!(
                    "  V.effect(function() {{ {}.style.display = {} ? '' : 'none'; }});\n",
                    var, transformed
                ));
            }
        }

        // v-html bindings
        for binding in &bindings.htmls {
            let var = path_vars.get(&binding.path).unwrap();
            let transformed = transform_expr(&binding.expr, &reactive_names);
            js.push_str(&format!(
                "  V.effect(function() {{ {}.innerHTML = {}; }});\n",
                var, transformed
            ));
        }

        // v-text bindings
        for binding in &bindings.text_directives {
            let var = path_vars.get(&binding.path).unwrap();
            let transformed = transform_expr(&binding.expr, &reactive_names);
            js.push_str(&format!(
                "  V.effect(function() {{ {}.textContent = {}; }});\n",
                var, transformed
            ));
        }

        // :class bindings (object + array syntax)
        for binding in &bindings.classes {
            let var = path_vars.get(&binding.path).unwrap();
            let items = parse_class_expr(&binding.expr);
            for item in &items {
                match item {
                    ClassItem::Toggle(class_name, cond_expr) => {
                        let transformed = transform_expr(cond_expr, &reactive_names);
                        js.push_str(&format!(
                            "  V.effect(function() {{ {}.classList.toggle('{}', !!{}); }});\n",
                            var, class_name, transformed
                        ));
                    }
                    ClassItem::Static(class_name) => {
                        js.push_str(&format!(
                            "  {}.classList.add('{}');\n",
                            var, class_name
                        ));
                    }
                }
            }
        }

        // :style bindings (object + array syntax)
        for binding in &bindings.styles {
            let var = path_vars.get(&binding.path).unwrap();
            let pairs = parse_style_expr(&binding.expr);
            for (prop, val_expr) in &pairs {
                let transformed = transform_expr(val_expr, &reactive_names);
                js.push_str(&format!(
                    "  V.effect(function() {{ {}.style.{} = {}; }});\n",
                    var, prop, transformed
                ));
            }
        }

        // v-model bindings
        for binding in &bindings.models {
            let var = path_vars.get(&binding.path).unwrap();
            let signal = &binding.signal_name;
            js.push_str(&format!(
                "  V.effect(function() {{ {}.value = {}.value; }});\n",
                var, signal
            ));
            js.push_str(&format!(
                "  {}.addEventListener('input', function(e) {{ {}.value = e.target.value; }});\n",
                var, signal
            ));
        }
    }

    js.push_str("})();\n");

    Some(js)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_script_ref() {
        let script = r#"
const count = ref(0)
const name = ref('hello')
"#;
        let analysis = analyze_script(script);
        assert_eq!(analysis.signals.len(), 2);
        assert_eq!(analysis.signals[0].name, "count");
        assert_eq!(analysis.signals[0].initial_value, "0");
        assert_eq!(analysis.signals[1].name, "name");
        assert_eq!(analysis.signals[1].initial_value, "'hello'");
    }

    #[test]
    fn test_analyze_script_computed() {
        let script = "const doubled = computed(() => count * 2)";
        let analysis = analyze_script(script);
        assert_eq!(analysis.computeds.len(), 1);
        assert_eq!(analysis.computeds[0].name, "doubled");
        assert_eq!(analysis.computeds[0].body, "count * 2");
    }

    #[test]
    fn test_analyze_script_functions() {
        let script = "function increment() { count.value++ }";
        let analysis = analyze_script(script);
        assert_eq!(analysis.functions.len(), 1);
        assert_eq!(analysis.functions[0].name, "increment");
        assert_eq!(analysis.functions[0].body, "count.value++");
    }

    #[test]
    fn test_analyze_script_full() {
        let script = r#"
import DefaultLayout from '../layouts/default.van'
import Hello from '../components/hello.van'

defineProps({ title: String })

const count = ref(0)
function increment() { count.value++ }
function decrement() { count.value-- }
"#;
        let analysis = analyze_script(script);
        assert_eq!(analysis.signals.len(), 1);
        assert_eq!(analysis.signals[0].name, "count");
        assert_eq!(analysis.functions.len(), 2);
        assert_eq!(analysis.functions[0].name, "increment");
        assert_eq!(analysis.functions[1].name, "decrement");
    }

    #[test]
    fn test_parse_html_simple() {
        let html = "<div><p>Hello</p></div>";
        let nodes = parse_html(html);
        assert_eq!(nodes.len(), 1);
        if let HtmlNode::Element(elem) = &nodes[0] {
            assert_eq!(elem.tag, "div");
            // p is an element child
            let element_children: Vec<_> = elem.children.iter().filter(|n| matches!(n, HtmlNode::Element(_))).collect();
            assert_eq!(element_children.len(), 1);
        } else {
            panic!("Expected element");
        }
    }

    #[test]
    fn test_parse_attrs() {
        let attrs = parse_attrs(r#" class="foo" @click="handler" v-show="visible""#);
        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs[0], ("class".to_string(), "foo".to_string()));
        assert_eq!(attrs[1], ("@click".to_string(), "handler".to_string()));
        assert_eq!(attrs[2], ("v-show".to_string(), "visible".to_string()));
    }

    #[test]
    fn test_walk_template_events() {
        let html = r#"<div><button @click="increment">+1</button></div>"#;
        let bindings = walk_template(html, &["count"]);
        assert_eq!(bindings.events.len(), 1);
        assert_eq!(bindings.events[0].event, "click");
        assert_eq!(bindings.events[0].handler, "increment");
        assert_eq!(bindings.events[0].path, vec![0, 0]); // div.children[0] = button
    }

    #[test]
    fn test_walk_template_text_binding() {
        let html = r#"<div><p>Count: {{ count }}</p></div>"#;
        let bindings = walk_template(html, &["count"]);
        assert_eq!(bindings.texts.len(), 1);
        assert_eq!(bindings.texts[0].template, "Count: {{ count }}");
        assert_eq!(bindings.texts[0].path, vec![0, 0]); // div.children[0] = p
    }

    #[test]
    fn test_walk_template_show() {
        let html = r#"<div><p v-show="visible">Hello</p></div>"#;
        let bindings = walk_template(html, &["visible"]);
        assert_eq!(bindings.shows.len(), 1);
        assert_eq!(bindings.shows[0].expr, "visible");
        assert_eq!(bindings.shows[0].path, vec![0, 0]); // div.children[0] = p
        assert_eq!(bindings.shows[0].transition, None);
    }

    #[test]
    fn test_walk_template_transition_skips_path() {
        // <Transition> should NOT count as a DOM element — path should skip it
        let html = r#"<div><p>Before</p><Transition name="slide"><div v-show="open">Drawer</div></Transition><p>After</p></div>"#;
        let bindings = walk_template(html, &["open"]);
        assert_eq!(bindings.shows.len(), 1);
        // div.children: [0]=p, [1]=div(drawer), [2]=p — Transition skipped
        assert_eq!(bindings.shows[0].path, vec![0, 1]);
        assert_eq!(bindings.shows[0].expr, "open");
        assert_eq!(bindings.shows[0].transition, Some("slide".to_string()));
    }

    #[test]
    fn test_walk_template_transition_no_name() {
        // <Transition> without name attribute should still work (default "v" prefix)
        let html = r#"<div><Transition><p v-show="visible">Hi</p></Transition></div>"#;
        let bindings = walk_template(html, &["visible"]);
        assert_eq!(bindings.shows.len(), 1);
        assert_eq!(bindings.shows[0].path, vec![0, 0]); // div.children[0] = p
        // No name attr → defaults to "v"
        assert_eq!(bindings.shows[0].transition, Some("v".to_string()));
    }

    #[test]
    fn test_walk_template_body() {
        // When HTML contains <body>, paths are relative to body
        let html = r#"<html><head><title>Test</title></head><body><nav>nav</nav><main><p>Count: {{ count }}</p><button @click="inc">+</button></main></body></html>"#;
        let bindings = walk_template(html, &["count"]);
        assert_eq!(bindings.texts.len(), 1);
        assert_eq!(bindings.texts[0].path, vec![1, 0]); // body.children[1]=main, main.children[0]=p
        assert_eq!(bindings.events.len(), 1);
        assert_eq!(bindings.events[0].path, vec![1, 1]); // body.children[1]=main, main.children[1]=button
    }

    #[test]
    fn test_walk_template_no_reactive_text() {
        let html = r#"<div><p>Hello {{ name }}</p></div>"#;
        // "name" is not reactive
        let bindings = walk_template(html, &["count"]);
        assert_eq!(bindings.texts.len(), 0);
    }

    #[test]
    fn test_template_to_js_expr() {
        let names = vec!["count"];
        assert_eq!(
            template_to_js_expr("Count: {{ count }}", &names),
            "'Count: ' + count.value"
        );
    }

    #[test]
    fn test_template_to_js_expr_only_reactive() {
        let names = vec!["count"];
        assert_eq!(
            template_to_js_expr("{{ count }}", &names),
            "count.value"
        );
    }

    #[test]
    fn test_generate_signals_positional() {
        let script = r#"
const count = ref(0)
function increment() { count.value++ }
function decrement() { count.value-- }
"#;
        // Simulate resolved body content
        let html = r#"<body><nav>nav</nav><main><h1>Title</h1><div class="counter"><p>Count: {{ count }}</p><button @click="increment">+1</button><button @click="decrement">-1</button></div></main></body>"#;

        let js = generate_signals(script, html, &[]).unwrap();

        // Should use positional paths, NOT querySelectorAll
        assert!(!js.contains("querySelectorAll"));
        assert!(!js.contains("data-van-"));

        // Should have document.body root
        assert!(js.contains("document.body"));

        // Should have children[N] paths
        assert!(js.contains(".children["));

        // Should have event listeners
        assert!(js.contains("addEventListener('click'"));

        // Should have effect for text binding
        assert!(js.contains("V.effect("));
        assert!(js.contains("textContent"));
        assert!(js.contains("count.value"));
    }

    #[test]
    fn test_generate_signals_none_for_static() {
        let script = r#"
defineProps({ title: String })
"#;
        let html = r#"<div><h1>Hello</h1></div>"#;
        assert!(generate_signals(script, html, &[]).is_none());
    }

    #[test]
    fn test_transform_expr() {
        let names = vec!["count"];
        assert_eq!(transform_expr("count", &names), "count.value");
        assert_eq!(
            transform_expr("count.value++", &names),
            "count.value++"
        );
        assert_eq!(
            transform_expr("'Count: ' + count", &names),
            "'Count: ' + count.value"
        );
    }

    #[test]
    fn test_runtime_js_included() {
        assert!(RUNTIME_JS.contains("Van"));
        assert!(RUNTIME_JS.contains("signal"));
        assert!(RUNTIME_JS.contains("effect"));
        assert!(RUNTIME_JS.contains("computed"));
    }

    #[test]
    fn test_collect_required_paths_dedup() {
        let bindings = TemplateBindings {
            events: vec![
                EventBinding { path: vec![1, 2, 0], event: "click".into(), handler: "inc".into() },
                EventBinding { path: vec![1, 2, 1], event: "click".into(), handler: "dec".into() },
            ],
            texts: vec![
                TextBinding { path: vec![1, 2], template: "{{ count }}".into() },
            ],
            shows: vec![],
            htmls: vec![],
            text_directives: vec![],
            classes: vec![],
            styles: vec![],
            models: vec![],
        };
        let paths = collect_required_paths(&bindings);
        // Should have: [1], [1,2], [1,2,0], [1,2,1]
        assert_eq!(paths.len(), 4);
        assert_eq!(paths[0], vec![1]);
        assert_eq!(paths[1], vec![1, 2]);
        assert_eq!(paths[2], vec![1, 2, 0]);
        assert_eq!(paths[3], vec![1, 2, 1]);
    }

    #[test]
    fn test_generate_signals_with_transition() {
        let script = r#"
const open = ref(false)
function toggle() { open.value = !open.value }
"#;
        let html = r#"<div><button @click="toggle">Toggle</button><Transition name="fade"><div v-show="open">Content</div></Transition></div>"#;
        let js = generate_signals(script, html, &[]).unwrap();
        // Should use V.transition() instead of style.display
        assert!(js.contains("V.transition("));
        assert!(js.contains("'fade'"));
        // Should NOT have style.display for the transitioned element
        assert!(!js.contains("style.display"));
    }

    #[test]
    fn test_runtime_js_has_transition() {
        assert!(RUNTIME_JS.contains("transition"));
        assert!(RUNTIME_JS.contains("__van_t"));
        assert!(RUNTIME_JS.contains("enter-from"));
        assert!(RUNTIME_JS.contains("leave-to"));
    }

    #[test]
    fn test_parse_self_closing() {
        let html = r#"<div><br /><p>text</p><img src="test.png" /></div>"#;
        let nodes = parse_html(html);
        assert_eq!(nodes.len(), 1);
        if let HtmlNode::Element(div) = &nodes[0] {
            let elements: Vec<_> = div.children.iter().filter(|n| matches!(n, HtmlNode::Element(_))).collect();
            assert_eq!(elements.len(), 3); // br, p, img
        }
    }

    // ─── :class/:style array syntax tests ───────────────────────────

    #[test]
    fn test_parse_class_array() {
        let items = parse_class_array("[{ active: isActive }, 'bold']");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], ClassItem::Toggle("active".into(), "isActive".into()));
        assert_eq!(items[1], ClassItem::Static("bold".into()));
    }

    #[test]
    fn test_parse_class_expr_object() {
        let items = parse_class_expr("{ active: isActive }");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0], ClassItem::Toggle("active".into(), "isActive".into()));
    }

    #[test]
    fn test_parse_class_expr_array() {
        let items = parse_class_expr("[{ active: isActive, highlight: isHighlighted }, 'static-cls']");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], ClassItem::Toggle("active".into(), "isActive".into()));
        assert_eq!(items[1], ClassItem::Toggle("highlight".into(), "isHighlighted".into()));
        assert_eq!(items[2], ClassItem::Static("static-cls".into()));
    }

    #[test]
    fn test_parse_style_array() {
        let pairs = parse_style_array("[{ color: textColor }, { fontSize: size }]");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("color".into(), "textColor".into()));
        assert_eq!(pairs[1], ("fontSize".into(), "size".into()));
    }

    #[test]
    fn test_parse_style_expr_object() {
        let pairs = parse_style_expr("{ color: textColor }");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("color".into(), "textColor".into()));
    }

    #[test]
    fn test_parse_style_expr_array() {
        let pairs = parse_style_expr("[{ color: c }, { fontSize: s }]");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], ("color".into(), "c".into()));
        assert_eq!(pairs[1], ("fontSize".into(), "s".into()));
    }

    #[test]
    fn test_generate_signals_class_binding() {
        let script = r#"
const isActive = ref(true)
"#;
        let html = r#"<div :class="[{ active: isActive }, 'base']"><p>Hello</p></div>"#;
        let js = generate_signals(script, html, &[]).unwrap();
        // Should have classList.toggle for object item
        assert!(js.contains("classList.toggle('active'"));
        // Should have classList.add for static item
        assert!(js.contains("classList.add('base')"));
    }

    #[test]
    fn test_generate_signals_style_binding() {
        let script = r#"
const textColor = ref('red')
const size = ref('16px')
"#;
        let html = r#"<div :style="[{ color: textColor }, { fontSize: size }]">Hello</div>"#;
        let js = generate_signals(script, html, &[]).unwrap();
        assert!(js.contains("style.color"));
        assert!(js.contains("style.fontSize"));
        assert!(js.contains("textColor.value"));
        assert!(js.contains("size.value"));
    }

    #[test]
    fn test_generate_signals_with_module_code() {
        let script = r#"
import { formatDate } from '../utils/format.ts'
const count = ref(0)
function increment() { count.value++ }
"#;
        let html = r#"<body><div><p>Count: {{ count }}</p><button @click="increment">+1</button></div></body>"#;
        let modules = vec![
            "function formatDate(d) { return d.toISOString(); }\nreturn { formatDate: formatDate };".to_string(),
        ];
        let js = generate_signals(script, html, &modules).unwrap();
        // Should have module IIFE
        assert!(js.contains("var __mod_0 = (function()"));
        assert!(js.contains("formatDate"));
        // Should still have signal code
        assert!(js.contains("V.signal(0)"));
        // Import line should be stripped — not cause issues
        assert!(!js.contains("from '../utils/format.ts'"));
    }

    #[test]
    fn test_generate_signals_imports_stripped() {
        let script = r#"
import { formatDate } from '../utils/format.ts'
import type { User } from '../types.ts'
const count = ref(0)
"#;
        let html = r#"<div><p>{{ count }}</p></div>"#;
        let js = generate_signals(script, html, &[]).unwrap();
        assert!(js.contains("V.signal(0)"));
    }
}
