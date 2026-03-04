use std::collections::HashMap;
use serde_json::Value;

/// Distinguishes between literal values and data paths in $t() parameters.
#[derive(Debug, PartialEq)]
pub(crate) enum ParamValue {
    /// A string or numeric literal, e.g. `'Alice'` or `5`
    Literal(String),
    /// A data path to resolve, e.g. `userName` or `user.name`
    DataPath(String),
}

/// Check if an expression is a `$t(...)` call.
/// Returns `(key, optional_params_str)` on match.
///
/// Supported forms:
/// - `$t('key')`
/// - `$t("key")`
/// - `$t('key', { name: value, ... })`
pub(crate) fn parse_t_call(expr: &str) -> Option<(String, Option<String>)> {
    let trimmed = expr.trim();
    if !trimmed.starts_with("$t(") || !trimmed.ends_with(')') {
        return None;
    }

    // Extract inner content between $t( and the final )
    let inner = trimmed[3..trimmed.len() - 1].trim();

    // Parse the key (single or double quoted string)
    let (key, rest) = parse_quoted_string(inner)?;

    let rest = rest.trim();
    if rest.is_empty() {
        return Some((key, None));
    }

    // Expect a comma followed by { ... }
    if !rest.starts_with(',') {
        return None;
    }
    let params_str = rest[1..].trim();
    if params_str.starts_with('{') && params_str.ends_with('}') {
        let inner_params = params_str[1..params_str.len() - 1].trim();
        Some((key, Some(inner_params.to_string())))
    } else {
        None
    }
}

/// Parse a single or double quoted string, returning (content, remaining).
fn parse_quoted_string(s: &str) -> Option<(String, &str)> {
    let quote = s.as_bytes().first()?;
    if *quote != b'\'' && *quote != b'"' {
        return None;
    }
    let quote_char = *quote as char;
    let rest = &s[1..];
    let end = rest.find(quote_char)?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

/// Parse a params string like `name: userName, count: 5` into key-value pairs.
/// Values can be:
/// - Quoted strings → `ParamValue::Literal`
/// - Numbers → `ParamValue::Literal`
/// - Identifiers/paths → `ParamValue::DataPath`
pub(crate) fn parse_t_params(params_str: &str) -> Vec<(String, ParamValue)> {
    let mut result = Vec::new();
    // Split by comma, but be careful with nested quotes
    for part in split_params(params_str) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon_pos) = part.find(':') {
            let key = part[..colon_pos].trim().to_string();
            let val_str = part[colon_pos + 1..].trim();
            let value = parse_param_value(val_str);
            result.push((key, value));
        }
    }
    result
}

/// Split params by commas, respecting quoted strings.
fn split_params(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in s.chars() {
        match in_quote {
            Some(q) if ch == q => {
                current.push(ch);
                in_quote = None;
            }
            Some(_) => {
                current.push(ch);
            }
            None => {
                if ch == '\'' || ch == '"' {
                    current.push(ch);
                    in_quote = Some(ch);
                } else if ch == ',' {
                    parts.push(current.clone());
                    current.clear();
                } else {
                    current.push(ch);
                }
            }
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Parse a single parameter value.
fn parse_param_value(val: &str) -> ParamValue {
    let trimmed = val.trim();

    // Quoted string → Literal
    if (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('"') && trimmed.ends_with('"'))
    {
        return ParamValue::Literal(trimmed[1..trimmed.len() - 1].to_string());
    }

    // Numeric literal
    if trimmed.parse::<f64>().is_ok() {
        return ParamValue::Literal(trimmed.to_string());
    }

    // Boolean literals
    if trimmed == "true" || trimmed == "false" {
        return ParamValue::Literal(trimmed.to_string());
    }

    // Otherwise treat as a data path
    ParamValue::DataPath(trimmed.to_string())
}

/// Look up a translation key in the i18n messages, substitute `{param}` placeholders,
/// and handle plural forms.
///
/// `params` contains already-resolved key→value mappings.
/// If the key is not found, returns the key itself as a fallback.
pub(crate) fn resolve_translation(
    key: &str,
    params: &HashMap<String, String>,
    i18n_messages: &Value,
) -> String {
    // Look up the message by key (supports dot-separated keys)
    let message = lookup_message(key, i18n_messages);
    let Some(message) = message else {
        return key.to_string();
    };

    // Handle plural forms if `count` param is present
    let message = if let Some(count_str) = params.get("count") {
        if let Ok(count) = count_str.parse::<i64>() {
            resolve_plural(&message, count)
        } else {
            // If count is not a valid integer, try the first non-plural form
            resolve_plural(&message, -1)
        }
    } else {
        // If message contains pipe but no count param, use first form
        if message.contains('|') {
            resolve_plural(&message, -1)
        } else {
            message
        }
    };

    // Replace {param} placeholders
    let mut result = message;
    for (k, v) in params {
        result = result.replace(&format!("{{{}}}", k), v);
    }
    result
}

/// Look up a message by key, supporting dot-separated paths.
fn lookup_message(key: &str, messages: &Value) -> Option<String> {
    let mut current = messages;
    for part in key.split('.') {
        current = current.get(part)?;
    }
    current.as_str().map(|s| s.to_string())
}

/// Resolve plural forms separated by `|`.
///
/// Rules (vue-i18n compatible):
/// - 2 forms: `"singular | plural"` → 0,1 = first; 2+ = second
/// - 3 forms: `"zero | singular | plural"` → 0 = first; 1 = second; 2+ = third
/// - If count is negative or invalid, returns the first form
fn resolve_plural(message: &str, count: i64) -> String {
    let forms: Vec<&str> = message.split('|').map(|s| s.trim()).collect();
    if forms.len() == 1 {
        return forms[0].to_string();
    }

    if count < 0 {
        return forms[0].to_string();
    }

    let idx = match forms.len() {
        2 => {
            if count == 0 || count == 1 { 0 } else { 1 }
        }
        3 => {
            if count == 0 {
                0
            } else if count == 1 {
                1
            } else {
                2
            }
        }
        n => {
            // For n > 3 forms, clamp to the last index
            let i = count as usize;
            if i >= n { n - 1 } else { i }
        }
    };
    forms[idx].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_t_simple() {
        let result = parse_t_call("$t('hello')");
        assert_eq!(result, Some(("hello".to_string(), None)));
    }

    #[test]
    fn test_parse_t_double_quotes() {
        let result = parse_t_call("$t(\"hello\")");
        assert_eq!(result, Some(("hello".to_string(), None)));
    }

    #[test]
    fn test_parse_t_with_params() {
        let result = parse_t_call("$t('greeting', { name: userName })");
        assert_eq!(
            result,
            Some(("greeting".to_string(), Some("name: userName".to_string())))
        );
    }

    #[test]
    fn test_parse_t_with_multiple_params() {
        let result = parse_t_call("$t('msg', { name: userName, count: 5 })");
        assert_eq!(
            result,
            Some(("msg".to_string(), Some("name: userName, count: 5".to_string())))
        );
    }

    #[test]
    fn test_parse_t_dotted_key() {
        let result = parse_t_call("$t('home.title')");
        assert_eq!(result, Some(("home.title".to_string(), None)));
    }

    #[test]
    fn test_parse_t_not_t_call() {
        assert_eq!(parse_t_call("userName"), None);
        assert_eq!(parse_t_call("$t"), None);
        assert_eq!(parse_t_call("$t()"), None);
    }

    #[test]
    fn test_parse_t_with_spaces() {
        let result = parse_t_call("  $t( 'hello' )  ");
        assert_eq!(result, Some(("hello".to_string(), None)));
    }

    #[test]
    fn test_parse_t_params_basic() {
        let params = parse_t_params("name: userName");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].0, "name");
        assert_eq!(params[0].1, ParamValue::DataPath("userName".to_string()));
    }

    #[test]
    fn test_parse_t_params_literal_string() {
        let params = parse_t_params("name: 'Alice'");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].1, ParamValue::Literal("Alice".to_string()));
    }

    #[test]
    fn test_parse_t_params_literal_number() {
        let params = parse_t_params("count: 5");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].1, ParamValue::Literal("5".to_string()));
    }

    #[test]
    fn test_parse_t_params_multiple() {
        let params = parse_t_params("name: userName, count: 3");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], ("name".to_string(), ParamValue::DataPath("userName".to_string())));
        assert_eq!(params[1], ("count".to_string(), ParamValue::Literal("3".to_string())));
    }

    #[test]
    fn test_resolve_translation_simple() {
        let messages = json!({"hello": "你好"});
        let result = resolve_translation("hello", &HashMap::new(), &messages);
        assert_eq!(result, "你好");
    }

    #[test]
    fn test_resolve_translation_dotted_key() {
        let messages = json!({"home": {"title": "欢迎"}});
        let result = resolve_translation("home.title", &HashMap::new(), &messages);
        assert_eq!(result, "欢迎");
    }

    #[test]
    fn test_resolve_translation_with_params() {
        let messages = json!({"greeting": "你好，{name}！"});
        let mut params = HashMap::new();
        params.insert("name".to_string(), "Alice".to_string());
        let result = resolve_translation("greeting", &params, &messages);
        assert_eq!(result, "你好，Alice！");
    }

    #[test]
    fn test_resolve_plural_two_forms() {
        let messages = json!({"items": "1 个项目 | {count} 个项目"});
        let mut params = HashMap::new();
        params.insert("count".to_string(), "1".to_string());
        assert_eq!(resolve_translation("items", &params, &messages), "1 个项目");

        params.insert("count".to_string(), "5".to_string());
        assert_eq!(resolve_translation("items", &params, &messages), "5 个项目");
    }

    #[test]
    fn test_resolve_plural_three_forms() {
        let messages = json!({"items": "没有项目 | 1 个项目 | {count} 个项目"});
        let mut params = HashMap::new();

        params.insert("count".to_string(), "0".to_string());
        assert_eq!(resolve_translation("items", &params, &messages), "没有项目");

        params.insert("count".to_string(), "1".to_string());
        assert_eq!(resolve_translation("items", &params, &messages), "1 个项目");

        params.insert("count".to_string(), "5".to_string());
        assert_eq!(resolve_translation("items", &params, &messages), "5 个项目");
    }

    #[test]
    fn test_missing_key_fallback() {
        let messages = json!({});
        let result = resolve_translation("nonexistent.key", &HashMap::new(), &messages);
        assert_eq!(result, "nonexistent.key");
    }

    #[test]
    fn test_resolve_translation_multiple_params() {
        let messages = json!({"welcome": "{greeting}, {name}!"});
        let mut params = HashMap::new();
        params.insert("greeting".to_string(), "Hello".to_string());
        params.insert("name".to_string(), "Bob".to_string());
        let result = resolve_translation("welcome", &params, &messages);
        assert_eq!(result, "Hello, Bob!");
    }
}
