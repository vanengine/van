use regex::Regex;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A non-component import from `<script setup>` (.ts/.js files).
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptImport {
    /// The full import statement as-is, e.g. `import { formatDate } from '../utils/format.ts'`
    pub raw: String,
    /// Whether this is a type-only import (`import type { ... }`)
    pub is_type_only: bool,
    /// The module path, e.g. `../utils/format.ts`
    pub path: String,
}

/// Parse non-.van imports from a script setup block.
/// Returns imports from .ts, .js, .tsx, .jsx files.
/// Supports both relative paths and scoped packages (`@scope/pkg/file.ts`).
/// Excludes: .van imports (handled by parse_imports), bare module imports like 'vue'.
pub fn parse_script_imports(script_setup: &str) -> Vec<ScriptImport> {
    let re = Regex::new(r#"(?m)^[ \t]*(import\s+(?:type\s+)?.*?\s+from\s+['"]([^'"]+\.(?:ts|js|tsx|jsx))['"].*)"#).unwrap();
    let type_re = Regex::new(r#"^import\s+type\s"#).unwrap();
    re.captures_iter(script_setup)
        .map(|cap| {
            let raw = cap[1].trim().to_string();
            let path = cap[2].to_string();
            let is_type_only = type_re.is_match(&raw);
            ScriptImport {
                raw,
                is_type_only,
                path,
            }
        })
        .collect()
}

/// Represents an import from a `<script setup>` block.
#[derive(Debug, Clone, PartialEq)]
pub struct VanImport {
    /// The imported identifier, e.g. `DefaultLayout`
    pub name: String,
    /// The kebab-case tag name, e.g. `default-layout`
    pub tag_name: String,
    /// The import path, e.g. `../layouts/default.van`
    pub path: String,
}

/// Parse `import X from './path.van'` statements from a script setup block.
/// Supports both relative paths (`./foo.van`, `../bar.van`) and scoped packages (`@scope/pkg/file.van`).
pub fn parse_imports(script_setup: &str) -> Vec<VanImport> {
    let re = Regex::new(r#"import\s+(\w+)\s+from\s+['"]([^'"]+\.van)['"]"#).unwrap();
    re.captures_iter(script_setup)
        .map(|cap| {
            let name = cap[1].to_string();
            let tag_name = pascal_to_kebab(&name);
            let path = cap[2].to_string();
            VanImport {
                name,
                tag_name,
                path,
            }
        })
        .collect()
}

/// Convert PascalCase to kebab-case: `DefaultLayout` → `default-layout`
pub fn pascal_to_kebab(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('-');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }
    result
}

/// A single prop declaration from `defineProps({ ... })`.
#[derive(Debug, Clone, PartialEq)]
pub struct PropDef {
    pub name: String,
    /// The declared type: "String", "Number", "Boolean", "Array", "Object", or None.
    pub prop_type: Option<String>,
    pub required: bool,
}

/// Represents the extracted blocks from a `.van` file.
#[derive(Debug, Default)]
pub struct VanBlock {
    pub template: Option<String>,
    pub script_setup: Option<String>,
    pub script_server: Option<String>,
    pub style: Option<String>,
    pub style_scoped: bool,
    pub props: Vec<PropDef>,
}

/// Extract blocks from a `.van` source file using simple tag matching.
///
/// This is a minimal implementation that finds `<template>`, `<script setup>`,
/// `<script lang="java">`, and `<style>` blocks by locating their opening and
/// closing tags.
pub fn parse_blocks(source: &str) -> VanBlock {
    let (style, style_scoped) = extract_style(source);
    let script_setup = extract_script_setup(source);
    let props = if let Some(ref script) = script_setup {
        parse_define_props(script)
    } else {
        Vec::new()
    };
    VanBlock {
        template: extract_block(source, "template"),
        script_setup,
        script_server: extract_script_server(source),
        style,
        style_scoped,
        props,
    }
}

/// Parse `defineProps({ ... })` from a script setup block.
///
/// Supports two forms per entry:
/// - Simple: `name: Type` → `PropDef { name, prop_type: Some("Type"), required: false }`
/// - Object: `name: { type: Type, required: true }` → extracts type and required flag
pub fn parse_define_props(script: &str) -> Vec<PropDef> {
    // Find `defineProps({` ... `})`
    let Some(start) = script.find("defineProps(") else {
        return Vec::new();
    };
    let after_paren = start + "defineProps(".len();
    let rest = &script[after_paren..];

    // Extract the balanced `{ ... }` content
    let Some(inner) = extract_balanced_braces(rest) else {
        return Vec::new();
    };

    if inner.trim().is_empty() {
        return Vec::new();
    }

    let mut props = Vec::new();

    // Split entries by comma, respecting nested `{ ... }`
    let entries = split_respecting_braces(inner);

    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }

        // Each entry: `name: value`
        let Some(colon_pos) = entry.find(':') else {
            continue;
        };
        let name = entry[..colon_pos].trim().trim_matches('\'').trim_matches('"').to_string();
        let value = entry[colon_pos + 1..].trim();

        if value.starts_with('{') {
            // Object form: `{ type: Type, required: true }`
            let obj_inner = value
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .unwrap_or(value)
                .trim();

            let mut prop_type = None;
            let mut required = false;

            for part in obj_inner.split(',') {
                let part = part.trim();
                if let Some(cp) = part.find(':') {
                    let key = part[..cp].trim();
                    let val = part[cp + 1..].trim();
                    if key == "type" {
                        prop_type = Some(val.to_string());
                    } else if key == "required" {
                        required = val == "true";
                    }
                }
            }

            props.push(PropDef {
                name,
                prop_type,
                required,
            });
        } else {
            // Simple form: `name: Type`
            props.push(PropDef {
                name,
                prop_type: Some(value.to_string()),
                required: false,
            });
        }
    }

    props
}

/// Extract the content between balanced `{` and `}` from the start of the string.
fn extract_balanced_braces(s: &str) -> Option<&str> {
    let s = s.trim();
    if !s.starts_with('{') {
        return None;
    }
    let mut depth = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split a string by commas, but respect nested `{ ... }` blocks.
fn split_respecting_braces(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
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

fn extract_block(source: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);

    let start_idx = source.find(&open)?;
    let after_open = &source[start_idx..];
    // Find the end of the opening tag (the '>')
    let tag_end = after_open.find('>')?;
    let content_start = start_idx + tag_end + 1;

    // Use rfind for the closing tag to handle nested <template #slot> blocks
    let end_idx = source.rfind(&close)?;
    if end_idx <= content_start {
        return None;
    }

    Some(source[content_start..end_idx].trim().to_string())
}

fn extract_script_setup(source: &str) -> Option<String> {
    // Look for <script setup or <script setup lang="ts">
    let marker = "<script setup";
    let close = "</script>";

    let start_idx = source.find(marker)?;
    let after_open = &source[start_idx..];
    let tag_end = after_open.find('>')?;
    let content_start = start_idx + tag_end + 1;

    // Find the closing </script> after this opening tag
    let remaining = &source[content_start..];
    let end_offset = remaining.find(close)?;
    let end_idx = content_start + end_offset;

    Some(source[content_start..end_idx].trim().to_string())
}

fn extract_script_server(source: &str) -> Option<String> {
    // Look for <script lang="java">
    let marker = "<script lang=\"java\">";
    let close = "</script>";

    let start_idx = source.find(marker)?;
    let content_start = start_idx + marker.len();

    // Find the closing </script> after this opening tag
    let remaining = &source[content_start..];
    let end_offset = remaining.find(close)?;
    let end_idx = content_start + end_offset;

    Some(source[content_start..end_idx].trim().to_string())
}

fn extract_style(source: &str) -> (Option<String>, bool) {
    let open = "<style";
    let close = "</style>";

    let Some(start_idx) = source.find(open) else {
        return (None, false);
    };
    let after_open = &source[start_idx..];
    let Some(tag_end) = after_open.find('>') else {
        return (None, false);
    };

    // Check if the opening tag attributes contain "scoped"
    let tag_attrs = &after_open[..tag_end];
    let is_scoped = tag_attrs.contains("scoped");

    let content_start = start_idx + tag_end + 1;
    let remaining = &source[content_start..];
    let Some(end_offset) = remaining.find(close) else {
        return (None, false);
    };
    let end_idx = content_start + end_offset;

    (Some(source[content_start..end_idx].trim().to_string()), is_scoped)
}

/// Generate a deterministic 8-hex-char scope ID from content (typically CSS).
///
/// Uses `DefaultHasher` with fixed seed (SipHash keys 0,0) so the same
/// content always produces the same ID, even across process restarts.
pub fn scope_id(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

/// Tags that should NOT receive a scope class.
/// - `slot` / `template`: virtual tags replaced during resolution
/// - structural/head tags: not rendered or not styleable targets
const SKIP_SCOPE_TAGS: &[&str] = &[
    "slot", "template",
    "html", "head", "body", "meta", "link", "title",
    "script", "style", "base", "noscript",
];

/// Add a scope class to every opening HTML tag in the fragment.
///
/// Skips closing tags, comments, and tags in [`SKIP_SCOPE_TAGS`].
/// Handles: existing class, no class, self-closing tags.
pub fn add_scope_class(html: &str, id: &str) -> String {
    let mut result = String::with_capacity(html.len() + id.len() * 10);
    let mut rest = html;

    while let Some(lt_pos) = rest.find('<') {
        result.push_str(&rest[..lt_pos]);
        rest = &rest[lt_pos..];

        // Skip closing tags and comments
        if rest.starts_with("</") || rest.starts_with("<!--") || rest.starts_with("<!") {
            if let Some(gt) = rest.find('>') {
                result.push_str(&rest[..=gt]);
                rest = &rest[gt + 1..];
            } else {
                result.push_str(rest);
                return result;
            }
            continue;
        }

        // Opening tag — find '>'
        let Some(gt) = rest.find('>') else {
            result.push_str(rest);
            return result;
        };

        // Extract tag name to check skip list
        let tag_name_end = rest[1..]
            .find(|c: char| !c.is_alphanumeric() && c != '-')
            .map(|p| p + 1)
            .unwrap_or(gt);
        let tag_name = &rest[1..tag_name_end];

        let should_skip = SKIP_SCOPE_TAGS.iter().any(|&t| t.eq_ignore_ascii_case(tag_name));

        if should_skip {
            result.push_str(&rest[..=gt]);
            rest = &rest[gt + 1..];
            continue;
        }

        let tag = &rest[..gt];
        let is_self_closing = tag.trim_end().ends_with('/');

        if let Some(class_idx) = tag.find("class=\"") {
            let after_quote = class_idx + 7;
            if let Some(end_quote) = tag[after_quote..].find('"') {
                let insert = after_quote + end_quote;
                result.push_str(&rest[..insert]);
                result.push(' ');
                result.push_str(id);
                result.push_str(&rest[insert..=gt]);
            } else {
                result.push_str(&rest[..=gt]);
            }
        } else if is_self_closing {
            let slash = tag.rfind('/').unwrap();
            result.push_str(&rest[..slash]);
            result.push_str("class=\"");
            result.push_str(id);
            result.push_str("\" ");
            result.push_str(&rest[slash..=gt]);
        } else {
            result.push_str(&rest[..gt]);
            result.push_str(" class=\"");
            result.push_str(id);
            result.push_str("\">");
        }

        rest = &rest[gt + 1..];
    }

    result.push_str(rest);
    result
}

/// Scope CSS by inserting `.{id}` before any pseudo-class/pseudo-element
/// on the last simple selector of each rule.
///
/// Input: `.card { border: 1px solid; }  a:hover { color: navy; }`
/// Output: `.card.a1b2c3d4 { border: 1px solid; }  a.a1b2c3d4:hover { color: navy; }`
pub fn scope_css(css: &str, id: &str) -> String {
    let suffix = format!(".{id}");
    let rule_re = Regex::new(r"([^{}]+)\{([^{}]*)\}").unwrap();

    rule_re.replace_all(css, |caps: &regex::Captures| {
        let selectors = caps[1].trim();
        let body = &caps[2];

        let scoped: Vec<String> = selectors
            .split(',')
            .map(|s| insert_scope_suffix(s.trim(), &suffix))
            .collect();

        format!("{} {{{}}}", scoped.join(", "), body)
    }).to_string()
}

/// Insert a scope class suffix before any pseudo-class/pseudo-element
/// at the end of a selector.
///
/// `.demo-list a:hover` → `.demo-list a.{suffix}:hover`
/// `.foo::before` → `.foo.{suffix}::before`
/// `.card h3` → `.card h3.{suffix}`
fn insert_scope_suffix(selector: &str, suffix: &str) -> String {
    // Find the last simple selector (after space or combinator)
    let last_start = selector
        .rfind(|c: char| c == ' ' || c == '>' || c == '+' || c == '~')
        .map(|p| p + 1)
        .unwrap_or(0);

    let last_part = &selector[last_start..];

    // Find the first `:` in the last part (pseudo-class or pseudo-element)
    if let Some(colon_pos) = last_part.find(':') {
        let insert_at = last_start + colon_pos;
        format!("{}{}{}", &selector[..insert_at], suffix, &selector[insert_at..])
    } else {
        format!("{}{}", selector, suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_blocks_basic() {
        let source = r#"
<script setup lang="ts">
import Hello from './hello.van'
</script>

<template>
  <div>Hello {{ name }}</div>
</template>

<style scoped>
.hello { color: red; }
</style>
"#;
        let blocks = parse_blocks(source);
        assert!(blocks.template.is_some());
        assert!(blocks.template.unwrap().contains("Hello {{ name }}"));
        assert!(blocks.script_setup.is_some());
        assert!(blocks.script_setup.unwrap().contains("import Hello"));
        assert!(blocks.style.is_some());
        assert!(blocks.style.unwrap().contains("color: red"));
        assert!(blocks.script_server.is_none());
    }

    #[test]
    fn test_parse_blocks_with_java_script() {
        let source = r#"
<template>
  <div></div>
</template>

<script setup lang="ts">
// ts code
</script>

<script lang="java">
// java code
</script>
"#;
        let blocks = parse_blocks(source);
        assert!(blocks.template.is_some());
        assert!(blocks.script_setup.is_some());
        assert!(blocks.script_server.is_some());
        assert!(blocks.script_server.unwrap().contains("java code"));
    }

    #[test]
    fn test_parse_blocks_empty() {
        let blocks = parse_blocks("");
        assert!(blocks.template.is_none());
        assert!(blocks.script_setup.is_none());
        assert!(blocks.script_server.is_none());
        assert!(blocks.style.is_none());
    }

    #[test]
    fn test_parse_blocks_nested_template_slots() {
        let source = r#"
<template>
  <default-layout>
    <template #title>{{ title }}</template>
    <h1>Welcome</h1>
  </default-layout>
</template>

<script setup lang="ts">
import DefaultLayout from '../layouts/default.van'
</script>
"#;
        let blocks = parse_blocks(source);
        let template = blocks.template.unwrap();
        assert!(template.contains("<default-layout>"), "Should contain opening tag");
        assert!(template.contains("</default-layout>"), "Should contain closing tag");
        assert!(template.contains("<template #title>"), "Should contain slot template");
        assert!(template.contains("<h1>Welcome</h1>"), "Should contain h1");
    }

    #[test]
    fn test_parse_imports() {
        let script = r#"
import DefaultLayout from '../layouts/default.van'
import Hello from '../components/hello.van'

defineProps({
  title: String
})
"#;
        let imports = parse_imports(script);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].name, "DefaultLayout");
        assert_eq!(imports[0].tag_name, "default-layout");
        assert_eq!(imports[0].path, "../layouts/default.van");
        assert_eq!(imports[1].name, "Hello");
        assert_eq!(imports[1].tag_name, "hello");
        assert_eq!(imports[1].path, "../components/hello.van");
    }

    #[test]
    fn test_parse_imports_double_quotes() {
        let script = r#"import Foo from "../components/foo.van""#;
        let imports = parse_imports(script);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].name, "Foo");
        assert_eq!(imports[0].path, "../components/foo.van");
    }

    #[test]
    fn test_parse_imports_no_van_files() {
        let script = r#"import { ref } from 'vue'"#;
        let imports = parse_imports(script);
        assert!(imports.is_empty());
    }

    #[test]
    fn test_pascal_to_kebab() {
        assert_eq!(pascal_to_kebab("DefaultLayout"), "default-layout");
        assert_eq!(pascal_to_kebab("Hello"), "hello");
        assert_eq!(pascal_to_kebab("MyComponent"), "my-component");
        assert_eq!(pascal_to_kebab("A"), "a");
    }

    // ─── Scoped style tests ──────────────────────────────────────────

    #[test]
    fn test_style_scoped_detection() {
        let scoped_source = r#"
<template><div>Hi</div></template>
<style scoped>
.card { color: red; }
</style>
"#;
        let blocks = parse_blocks(scoped_source);
        assert!(blocks.style_scoped);
        assert!(blocks.style.unwrap().contains("color: red"));

        let unscoped_source = r#"
<template><div>Hi</div></template>
<style>
.card { color: blue; }
</style>
"#;
        let blocks = parse_blocks(unscoped_source);
        assert!(!blocks.style_scoped);
        assert!(blocks.style.unwrap().contains("color: blue"));
    }

    #[test]
    fn test_style_scoped_with_lang() {
        let source = r#"
<template><div>Hi</div></template>
<style scoped lang="css">
h1 { font-size: 2rem; }
</style>
"#;
        let blocks = parse_blocks(source);
        assert!(blocks.style_scoped);
    }

    #[test]
    fn test_scope_id_deterministic() {
        let id1 = scope_id(".card { color: red; }");
        let id2 = scope_id(".card { color: red; }");
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 8);
        // Different content → different ID
        let id3 = scope_id("h1 { color: blue; }");
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_add_scope_class_all_elements() {
        let html = r#"<div class="card"><h1>Title</h1><p>Text</p></div>"#;
        let result = add_scope_class(html, "a1b2c3d4");
        assert_eq!(
            result,
            r#"<div class="card a1b2c3d4"><h1 class="a1b2c3d4">Title</h1><p class="a1b2c3d4">Text</p></div>"#
        );
    }

    #[test]
    fn test_add_scope_class_no_class() {
        let html = r#"<div><h1>Title</h1></div>"#;
        let result = add_scope_class(html, "a1b2c3d4");
        assert_eq!(result, r#"<div class="a1b2c3d4"><h1 class="a1b2c3d4">Title</h1></div>"#);
    }

    #[test]
    fn test_add_scope_class_self_closing() {
        let html = r#"<div><img src="x.png" /><br /></div>"#;
        let result = add_scope_class(html, "a1b2c3d4");
        assert_eq!(
            result,
            r#"<div class="a1b2c3d4"><img src="x.png" class="a1b2c3d4" /><br class="a1b2c3d4" /></div>"#
        );
    }

    #[test]
    fn test_add_scope_class_skips_comments() {
        let html = r#"<!-- comment --><div>Hi</div>"#;
        let result = add_scope_class(html, "a1b2c3d4");
        assert_eq!(result, r#"<!-- comment --><div class="a1b2c3d4">Hi</div>"#);
    }

    #[test]
    fn test_add_scope_class_skips_slot() {
        let html = r#"<div><slot /><slot name="x">fallback</slot></div>"#;
        let result = add_scope_class(html, "a1b2c3d4");
        assert_eq!(result, r#"<div class="a1b2c3d4"><slot /><slot name="x">fallback</slot></div>"#);
    }

    #[test]
    fn test_add_scope_class_skips_structural() {
        let html = r#"<html><head><meta charset="UTF-8" /></head><body><nav class="x">Hi</nav></body></html>"#;
        let result = add_scope_class(html, "a1b2c3d4");
        assert_eq!(
            result,
            r#"<html><head><meta charset="UTF-8" /></head><body><nav class="x a1b2c3d4">Hi</nav></body></html>"#
        );
    }

    #[test]
    fn test_scope_css_single_selector() {
        let css = ".card { border: 1px solid; }";
        let result = scope_css(css, "a1b2c3d4");
        assert_eq!(result, ".card.a1b2c3d4 { border: 1px solid; }");
    }

    #[test]
    fn test_scope_css_multiple_rules() {
        let css = ".card { border: 1px solid; }\nh1 { color: navy; }";
        let result = scope_css(css, "a1b2c3d4");
        assert!(result.contains(".card.a1b2c3d4 { border: 1px solid; }"));
        assert!(result.contains("h1.a1b2c3d4 { color: navy; }"));
    }

    #[test]
    fn test_scope_css_comma_selectors() {
        let css = ".card, .box { border: 1px solid; }";
        let result = scope_css(css, "a1b2c3d4");
        assert_eq!(result, ".card.a1b2c3d4, .box.a1b2c3d4 { border: 1px solid; }");
    }

    #[test]
    fn test_scope_css_descendant_selector() {
        let css = ".card h1 { color: navy; }";
        let result = scope_css(css, "a1b2c3d4");
        assert_eq!(result, ".card h1.a1b2c3d4 { color: navy; }");
    }

    #[test]
    fn test_scope_css_pseudo_class() {
        let css = ".demo-list a:hover { text-decoration: underline; }";
        let result = scope_css(css, "a1b2c3d4");
        assert_eq!(result, ".demo-list a.a1b2c3d4:hover { text-decoration: underline; }");
    }

    #[test]
    fn test_scope_css_pseudo_element() {
        let css = ".item::before { content: '-'; }";
        let result = scope_css(css, "a1b2c3d4");
        assert_eq!(result, ".item.a1b2c3d4::before { content: '-'; }");
    }

    #[test]
    fn test_scope_css_no_pseudo() {
        let css = "h1 { font-size: 2rem; }";
        let result = scope_css(css, "a1b2c3d4");
        assert_eq!(result, "h1.a1b2c3d4 { font-size: 2rem; }");
    }

    // ─── defineProps tests ──────────────────────────────────────────

    #[test]
    fn test_parse_define_props_simple() {
        let script = "defineProps({ title: String, count: Number })";
        let props = parse_define_props(script);
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].name, "title");
        assert_eq!(props[0].prop_type, Some("String".to_string()));
        assert!(!props[0].required);
        assert_eq!(props[1].name, "count");
        assert_eq!(props[1].prop_type, Some("Number".to_string()));
        assert!(!props[1].required);
    }

    #[test]
    fn test_parse_define_props_with_required() {
        let script = "defineProps({ user: { type: Object, required: true } })";
        let props = parse_define_props(script);
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].name, "user");
        assert_eq!(props[0].prop_type, Some("Object".to_string()));
        assert!(props[0].required);
    }

    #[test]
    fn test_parse_define_props_mixed() {
        let script = r#"defineProps({
  title: String,
  user: { type: Object, required: true },
  count: Number
})"#;
        let props = parse_define_props(script);
        assert_eq!(props.len(), 3);
        assert_eq!(props[0].name, "title");
        assert_eq!(props[0].prop_type, Some("String".to_string()));
        assert!(!props[0].required);
        assert_eq!(props[1].name, "user");
        assert_eq!(props[1].prop_type, Some("Object".to_string()));
        assert!(props[1].required);
        assert_eq!(props[2].name, "count");
        assert_eq!(props[2].prop_type, Some("Number".to_string()));
        assert!(!props[2].required);
    }

    #[test]
    fn test_parse_define_props_missing() {
        let script = "const count = ref(0)";
        let props = parse_define_props(script);
        assert!(props.is_empty());
    }

    #[test]
    fn test_parse_define_props_empty() {
        let script = "defineProps({})";
        let props = parse_define_props(script);
        assert!(props.is_empty());
    }

    #[test]
    fn test_parse_blocks_includes_props() {
        let source = r#"
<script setup lang="ts">
defineProps({ title: String, count: Number })
</script>

<template>
  <h1>{{ title }}</h1>
</template>
"#;
        let blocks = parse_blocks(source);
        assert_eq!(blocks.props.len(), 2);
        assert_eq!(blocks.props[0].name, "title");
        assert_eq!(blocks.props[1].name, "count");
    }

    // ─── parse_script_imports tests ───────────────────────────────────

    #[test]
    fn test_parse_script_imports_ts() {
        let script = r#"
import { formatDate } from '../utils/format.ts'
import DefaultLayout from '../layouts/default.van'
const count = ref(0)
"#;
        let imports = parse_script_imports(script);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].path, "../utils/format.ts");
        assert!(!imports[0].is_type_only);
        assert!(imports[0].raw.contains("formatDate"));
    }

    #[test]
    fn test_parse_script_imports_type_only() {
        let script = r#"
import type { User } from '../types/models.ts'
import { formatDate } from '../utils/format.ts'
"#;
        let imports = parse_script_imports(script);
        assert_eq!(imports.len(), 2);
        assert!(imports[0].is_type_only);
        assert_eq!(imports[0].path, "../types/models.ts");
        assert!(!imports[1].is_type_only);
        assert_eq!(imports[1].path, "../utils/format.ts");
    }

    #[test]
    fn test_parse_script_imports_js() {
        let script = r#"import foo from '../utils/helper.js'"#;
        let imports = parse_script_imports(script);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].path, "../utils/helper.js");
        assert!(!imports[0].is_type_only);
    }

    #[test]
    fn test_parse_script_imports_ignores_van() {
        let script = r#"
import Hello from './hello.van'
import Foo from '../foo.van'
"#;
        let imports = parse_script_imports(script);
        assert!(imports.is_empty());
    }

    #[test]
    fn test_parse_script_imports_ignores_bare() {
        let script = r#"import { ref } from 'vue'"#;
        let imports = parse_script_imports(script);
        assert!(imports.is_empty());
    }

    #[test]
    fn test_parse_script_imports_mixed_type() {
        // `import { type User, formatDate } from ...` is NOT type-only
        let script = r#"import { type User, formatDate } from '../utils.ts'"#;
        let imports = parse_script_imports(script);
        assert_eq!(imports.len(), 1);
        assert!(!imports[0].is_type_only);
    }

    #[test]
    fn test_parse_imports_scoped_package() {
        let script = r#"
import VanButton from '@van-ui/button/button.van'
import DefaultLayout from '../layouts/default.van'
"#;
        let imports = parse_imports(script);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].name, "VanButton");
        assert_eq!(imports[0].tag_name, "van-button");
        assert_eq!(imports[0].path, "@van-ui/button/button.van");
        assert_eq!(imports[1].name, "DefaultLayout");
        assert_eq!(imports[1].path, "../layouts/default.van");
    }

    #[test]
    fn test_parse_script_imports_scoped_package() {
        let script = r#"
import { formatDate } from '@van-ui/utils/format.ts'
import { helper } from '../utils/helper.ts'
"#;
        let imports = parse_script_imports(script);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].path, "@van-ui/utils/format.ts");
        assert_eq!(imports[1].path, "../utils/helper.ts");
    }

    #[test]
    fn test_parse_script_imports_tsx_jsx() {
        let script = r#"
import { render } from '../lib/render.tsx'
import { helper } from '../lib/helper.jsx'
"#;
        let imports = parse_script_imports(script);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].path, "../lib/render.tsx");
        assert_eq!(imports[1].path, "../lib/helper.jsx");
    }
}
