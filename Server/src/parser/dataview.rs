use crate::vault::{NoteEntry, Vault};

/// Replace all ```dataview and ```dataviewjs blocks in `content` with rendered HTML.
pub fn render_dataview_blocks(content: &str, vault: &Vault) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("```dataview") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 3..]; // skip ```

        let is_js = after.starts_with("dataviewjs");
        let lang_end = after.find('\n').unwrap_or(after.len());
        let body_start = &after[lang_end..];

        if let Some(end) = body_start.find("\n```") {
            let query = body_start[..end].trim();
            let rendered = if is_js {
                dataviewjs_placeholder()
            } else {
                execute_query(query, vault)
            };
            result.push_str(&rendered);
            rest = &body_start[end + 4..]; // skip \n```
        } else {
            // No closing fence — emit as-is and stop
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

fn dataviewjs_placeholder() -> String {
    r#"<div class="dataview-notice">⚠ DataviewJS wird auf dem Server nicht unterstützt.</div>"#.to_string()
}

// ── Query executor ────────────────────────────────────────────────────────────

fn execute_query(query: &str, vault: &Vault) -> String {
    let upper = query.to_uppercase();

    let qtype = if upper.starts_with("TABLE") {
        QueryType::Table
    } else if upper.starts_with("TASK") {
        return r#"<div class="dataview-notice">⚠ Dataview TASK wird nicht unterstützt.</div>"#.to_string();
    } else {
        QueryType::List
    };

    // Strip the query type keyword
    let after_type = match qtype {
        QueryType::Table => query[5..].trim(),
        QueryType::List  => query[4..].trim(),
    };

    // Split off FROM clause
    let (fields_str, after_fields) = split_at_keyword(after_type, "FROM");
    let (from_str, after_from) = split_at_keyword(after_fields, "WHERE");
    let (where_str, after_where) = split_at_keyword(after_from, "SORT");
    let (sort_str, after_sort) = split_at_keyword(after_where, "LIMIT");
    let limit_str = after_sort.trim();

    // Collect candidates
    let mut notes: Vec<&NoteEntry> = vault
        .notes
        .values()
        .filter(|n| n.kind == crate::vault::NoteKind::Markdown)
        .collect();

    // FROM filter
    let from = from_str.trim();
    if !from.is_empty() {
        if let Some(tag) = from.strip_prefix('#') {
            let tag = tag.trim().to_lowercase();
            notes.retain(|n| n.tags.iter().any(|t| t.to_lowercase() == tag));
        } else {
            let folder = from.trim_matches('"').trim_matches('\'');
            notes.retain(|n| n.slug.starts_with(folder) || n.path.starts_with(folder));
        }
    }

    // WHERE filter
    let where_clause = where_str.trim();
    if !where_clause.is_empty() {
        notes.retain(|n| eval_where(where_clause, n));
    }

    // SORT
    let sort = sort_str.trim();
    if !sort.is_empty() {
        let (field, desc) = parse_sort(sort);
        notes.sort_by(|a, b| {
            let va = field_value(a, &field);
            let vb = field_value(b, &field);
            let ord = va.cmp(&vb);
            if desc { ord.reverse() } else { ord }
        });
    } else {
        notes.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    }

    // LIMIT
    if let Ok(n) = limit_str.parse::<usize>() {
        notes.truncate(n);
    }

    match qtype {
        QueryType::List => render_list(&notes),
        QueryType::Table => render_table(&notes, fields_str.trim()),
    }
}

enum QueryType { List, Table }

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render_list(notes: &[&NoteEntry]) -> String {
    if notes.is_empty() {
        return "<p><em>(Keine Ergebnisse)</em></p>".to_string();
    }
    let mut s = String::from("<ul class=\"dataview-list\">\n");
    for n in notes {
        s.push_str(&format!(
            "  <li><a href=\"/note/{}\">{}</a></li>\n",
            html_escape(&n.slug),
            html_escape(&n.title),
        ));
    }
    s.push_str("</ul>\n");
    s
}

fn render_table(notes: &[&NoteEntry], fields_str: &str) -> String {
    // Parse column definitions: "field1, field2 AS alias, ..."
    let cols: Vec<(String, String)> = if fields_str.is_empty() || fields_str.to_uppercase() == "WITHOUT ID" {
        vec![("file.link".to_string(), "Datei".to_string())]
    } else {
        fields_str.split(',').map(|col| {
            let col = col.trim();
            let upper = col.to_uppercase();
            if let Some(idx) = upper.find(" AS ") {
                let field = col[..idx].trim().to_string();
                let alias = col[idx + 4..].trim().trim_matches('"').trim_matches('\'').to_string();
                (field, alias)
            } else {
                let alias = col.split('.').last().unwrap_or(col).to_string();
                (col.to_string(), alias)
            }
        }).collect()
    };

    if notes.is_empty() {
        return "<p><em>(Keine Ergebnisse)</em></p>".to_string();
    }

    let mut s = String::from("<table class=\"dataview-table\">\n<thead><tr>\n  <th>Datei</th>\n");
    for (_, alias) in &cols {
        s.push_str(&format!("  <th>{}</th>\n", html_escape(alias)));
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    for n in notes {
        s.push_str("<tr>\n");
        s.push_str(&format!(
            "  <td><a href=\"/note/{}\">{}</a></td>\n",
            html_escape(&n.slug),
            html_escape(&n.title),
        ));
        for (field, _) in &cols {
            let val = field_value(n, field);
            s.push_str(&format!("  <td>{}</td>\n", html_escape(&val)));
        }
        s.push_str("</tr>\n");
    }
    s.push_str("</tbody></table>\n");
    s
}

// ── Field access ──────────────────────────────────────────────────────────────

fn field_value(note: &NoteEntry, field: &str) -> String {
    match field.to_lowercase().as_str() {
        "file.name" | "file.title" => note.title.clone(),
        "file.path" => note.path.clone(),
        "file.link" => format!("[[{}]]", note.title),
        "file.slug" => note.slug.clone(),
        "file.tags" | "tags" => note.tags.join(", "),
        other => note.frontmatter.get(other)
            .or_else(|| note.frontmatter.get(&other.replace("file.", "")))
            .cloned()
            .unwrap_or_default(),
    }
}

// ── WHERE evaluation ──────────────────────────────────────────────────────────

fn eval_where(clause: &str, note: &NoteEntry) -> bool {
    // Support AND / OR (left-to-right, no precedence)
    let upper = clause.to_uppercase();
    if let Some(pos) = find_keyword_pos(&upper, " AND ") {
        return eval_where(&clause[..pos], note) && eval_where(&clause[pos + 5..], note);
    }
    if let Some(pos) = find_keyword_pos(&upper, " OR ") {
        return eval_where(&clause[..pos], note) || eval_where(&clause[pos + 4..], note);
    }

    // contains(field, "value")
    if upper.starts_with("CONTAINS(") {
        if let Some(inner) = clause[9..].strip_suffix(')') {
            let parts: Vec<&str> = inner.splitn(2, ',').collect();
            if parts.len() == 2 {
                let field = parts[0].trim();
                let val = parts[1].trim().trim_matches('"').trim_matches('\'').to_lowercase();
                return field_value(note, field).to_lowercase().contains(&val);
            }
        }
        return false;
    }

    // field op value
    for op in &["!=", ">=", "<=", ">", "<", "="] {
        if let Some(pos) = clause.find(op) {
            let field = clause[..pos].trim();
            let val = clause[pos + op.len()..].trim().trim_matches('"').trim_matches('\'');
            let fv = field_value(note, field).to_lowercase();
            let val_lc = val.to_lowercase();
            return match *op {
                "="  => fv == val_lc,
                "!=" => fv != val_lc,
                ">"  => fv > val_lc,
                "<"  => fv < val_lc,
                ">=" => fv >= val_lc,
                "<=" => fv <= val_lc,
                _    => false,
            };
        }
    }
    true
}

fn find_keyword_pos(haystack: &str, kw: &str) -> Option<usize> {
    haystack.find(kw)
}

// ── SORT parsing ──────────────────────────────────────────────────────────────

fn parse_sort(sort: &str) -> (String, bool) {
    let upper = sort.to_uppercase();
    if let Some(field) = upper.strip_suffix(" DESC") {
        (sort[..field.len()].trim().to_string(), true)
    } else if let Some(field) = upper.strip_suffix(" ASC") {
        (sort[..field.len()].trim().to_string(), false)
    } else {
        (sort.to_string(), false)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Split `text` at the first occurrence of `keyword` (case-insensitive, word-boundary).
/// Returns (before, after_keyword).
fn split_at_keyword<'a>(text: &'a str, keyword: &str) -> (&'a str, &'a str) {
    let upper = text.to_uppercase();
    let kw_upper = format!(" {} ", keyword);
    if let Some(pos) = upper.find(&kw_upper) {
        (&text[..pos], &text[pos + kw_upper.len()..])
    } else if upper.ends_with(&format!(" {}", keyword)) {
        let pos = text.len() - keyword.len() - 1;
        (&text[..pos], "")
    } else {
        (text, "")
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}
