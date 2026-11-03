use std::collections::HashMap;

/// Evaluates a group query string against a note's metadata.
///
/// Query format: one condition per line, all conditions must match (AND).
/// Blank lines and lines starting with `#` are ignored.
///
/// Condition syntax: `<field> <operator> <value>`
///   - field:    any frontmatter key (case-insensitive), or special: `tag`, `path`, `title`
///   - operator: `=`, `!=`, `contains`, `starts_with`, `ends_with`
///   - value:    bare word or quoted string (single or double quotes)
///
/// Special field `tag` with `=` or `contains` checks the tags array.
/// An empty query string matches all notes.
pub fn evaluate_query(
    query: &str,
    frontmatter: &HashMap<String, String>,
    tags: &[String],
    slug: &str,
    title: &str,
) -> bool {
    let conditions: Vec<&str> = query
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    if conditions.is_empty() {
        return true;
    }

    conditions
        .iter()
        .all(|cond| eval_cond(cond, frontmatter, tags, slug, title))
}

fn eval_cond(
    cond: &str,
    fm: &HashMap<String, String>,
    tags: &[String],
    slug: &str,
    title: &str,
) -> bool {
    let Some((field, op, value)) = parse_cond(cond) else {
        return false;
    };

    // Tag field is special: check array membership
    if field == "tag" || field == "tags" {
        return match op.as_str() {
            "=" | "contains" => tags.iter().any(|t| t.to_lowercase() == value),
            "!=" => !tags.iter().any(|t| t.to_lowercase() == value),
            _ => false,
        };
    }

    // Resolve field value (case-insensitive key lookup)
    let field_val = match field.as_str() {
        "path" | "slug" => slug.to_lowercase(),
        "title" => title.to_lowercase(),
        f => fm
            .iter()
            .find(|(k, _)| k.to_lowercase() == f)
            .map(|(_, v)| v.to_lowercase())
            .unwrap_or_default(),
    };

    match op.as_str() {
        "=" => field_val == value,
        "!=" => field_val != value,
        "contains" => field_val.contains(&value),
        "starts_with" => field_val.starts_with(&value),
        "ends_with" => field_val.ends_with(&value),
        _ => false,
    }
}

fn parse_cond(s: &str) -> Option<(String, String, String)> {
    // Try keyword operators first (with word boundaries), then symbol operators
    let word_ops: &[&str] = &["starts_with", "ends_with", "contains"];
    let sym_ops: &[&str] = &["!=", "="];

    for &op in word_ops {
        if let Some(pos) = find_word_op(s, op) {
            let field = s[..pos].trim().to_lowercase();
            let raw = s[pos + op.len()..].trim();
            let value = strip_quotes(raw).to_lowercase();
            if !field.is_empty() {
                return Some((field, op.to_string(), value));
            }
        }
    }
    for &op in sym_ops {
        if let Some(pos) = s.find(op) {
            let field = s[..pos].trim().to_lowercase();
            let raw = s[pos + op.len()..].trim();
            let value = strip_quotes(raw).to_lowercase();
            if !field.is_empty() {
                return Some((field, op.to_string(), value));
            }
        }
    }
    None
}

fn find_word_op(s: &str, op: &str) -> Option<usize> {
    let s_lc = s.to_lowercase();
    let op_lc = op.to_lowercase();
    let mut start = 0;
    while let Some(pos) = s_lc[start..].find(&op_lc) {
        let abs = start + pos;
        let before_ok = abs == 0 || s_lc.as_bytes()[abs - 1] == b' ';
        let after_pos = abs + op.len();
        let after_ok = after_pos >= s_lc.len() || s_lc.as_bytes()[after_pos] == b' ';
        if before_ok && after_ok {
            return Some(abs);
        }
        start = abs + 1;
    }
    None
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"'))
            || (s.starts_with('\'') && s.ends_with('\'')))
    {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fm(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn test_empty_query() {
        assert!(evaluate_query("", &fm(&[]), &[], "slug", "title"));
        assert!(evaluate_query("  \n  ", &fm(&[]), &[], "slug", "title"));
    }

    #[test]
    fn test_eq() {
        let m = fm(&[("published", "true"), ("category", "research")]);
        assert!(evaluate_query("published = true", &m, &[], "s", "t"));
        assert!(evaluate_query("category = research", &m, &[], "s", "t"));
        assert!(!evaluate_query("category = other", &m, &[], "s", "t"));
    }

    #[test]
    fn test_neq() {
        let m = fm(&[("status", "draft")]);
        assert!(evaluate_query("status != published", &m, &[], "s", "t"));
        assert!(!evaluate_query("status != draft", &m, &[], "s", "t"));
    }

    #[test]
    fn test_contains() {
        let m = fm(&[("title", "My Research Note")]);
        assert!(evaluate_query("title contains research", &m, &[], "s", "t"));
        assert!(!evaluate_query("title contains premium", &m, &[], "s", "t"));
    }

    #[test]
    fn test_tag() {
        let tags = vec!["premium".to_string(), "research".to_string()];
        assert!(evaluate_query("tag = premium", &fm(&[]), &tags, "s", "t"));
        assert!(evaluate_query("tag contains research", &fm(&[]), &tags, "s", "t"));
        assert!(!evaluate_query("tag = vip", &fm(&[]), &tags, "s", "t"));
    }

    #[test]
    fn test_path() {
        assert!(evaluate_query("path starts_with Projekt/", &fm(&[]), &[], "Projekt/Notiz", "t"));
        assert!(!evaluate_query("path starts_with Archiv/", &fm(&[]), &[], "Projekt/Notiz", "t"));
    }

    #[test]
    fn test_and() {
        let m = fm(&[("published", "true"), ("category", "research")]);
        assert!(evaluate_query("published = true\ncategory = research", &m, &[], "s", "t"));
        assert!(!evaluate_query("published = true\ncategory = other", &m, &[], "s", "t"));
    }

    #[test]
    fn test_quoted_value() {
        let m = fm(&[("group", "my group")]);
        assert!(evaluate_query("group = \"my group\"", &m, &[], "s", "t"));
        assert!(evaluate_query("group = 'my group'", &m, &[], "s", "t"));
    }
}
