/// Obsidian Bases renderer.
///
/// Parses a `.base` YAML file (or an inline `base` code block) and renders
/// a read-only HTML view for the web server.  Only `table` and `list` view
/// types are rendered; other types fall back to a notice.
///
/// What is supported
/// -----------------
/// Filters  : global + per-view; `and` / `or` / `not` trees; string
///            comparisons (==, !=, <, <=, >, >=); `file.hasTag()`,
///            `file.inFolder()`, `file.hasLink()`.
/// Views    : table (with columns from `properties`), list.
/// Order    : `order` list → sort by first matching property.
/// Limit    : `limit` rows.
/// GroupBy  : groups rows by a property value.
/// Properties: `displayName` used as column header.
///
/// What is NOT supported (server-side limitation)
/// -----------------------------------------------
/// Formulas, summaries, `cards` / `kanban` / `map` views, `this.*`.

use crate::vault::{NoteEntry, Vault};
use serde_yaml::Value;
use std::collections::HashMap;

// ── Public API ────────────────────────────────────────────────────────────────

/// Render a `.base` file's YAML content into HTML.
pub fn render_base(yaml: &str, vault: &Vault) -> String {
    match serde_yaml::from_str::<Value>(yaml) {
        Ok(doc) => render_doc(&doc, vault),
        Err(e) => format!(
            "<div class=\"bases-error\">⚠ Bases-Fehler: {}</div>",
            html_escape(&e.to_string())
        ),
    }
}

/// Replace all ` ```base ` code blocks in `content` with rendered HTML.
pub fn render_base_blocks(content: &str, vault: &Vault) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("```base") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 3..]; // skip ```
        let lang_end = after.find('\n').unwrap_or(after.len());
        let body_start = &after[lang_end..];

        if let Some(end) = body_start.find("\n```") {
            let yaml = body_start[..end].trim();
            result.push_str(&render_base(yaml, vault));
            rest = &body_start[end + 4..];
        } else {
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

// ── Document rendering ────────────────────────────────────────────────────────

fn render_doc(doc: &Value, vault: &Vault) -> String {
    let global_filter = doc.get("filters");
    let properties = parse_properties(doc.get("properties"));

    let views = match doc.get("views").and_then(|v| v.as_sequence()) {
        Some(seq) if !seq.is_empty() => seq.to_vec(),
        _ => {
            // No views defined — render a plain table of all (filtered) notes.
            let notes = collect_notes(vault, global_filter, None);
            return render_table_view(&notes, &[], &properties, None, None);
        }
    };

    let mut html = String::new();
    for view in &views {
        let view_type = view
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("table");
        let view_name = view.get("name").and_then(|v| v.as_str());
        let limit = view.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
        let view_filter = view.get("filters");
        let order = view
            .get("order")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            });
        let group_by = view.get("groupBy").and_then(|gb| {
            gb.get("property")
                .and_then(|p| p.as_str())
                .map(|p| {
                    let dir = gb
                        .get("direction")
                        .and_then(|d| d.as_str())
                        .unwrap_or("ASC");
                    (p.to_string(), dir.to_uppercase() == "DESC")
                })
        });

        let mut notes = collect_notes(vault, global_filter, view_filter);

        // Sort
        if let Some(ref ord) = order {
            if let Some(first) = ord.first() {
                let field = first.clone();
                notes.sort_by(|a, b| {
                    note_field(a, &field)
                        .to_lowercase()
                        .cmp(&note_field(b, &field).to_lowercase())
                });
            }
        } else {
            notes.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }

        // GroupBy sort (secondary)
        if let Some((ref gb_prop, desc)) = group_by {
            let field = gb_prop.clone();
            notes.sort_by(|a, b| {
                let va = note_field(a, &field).to_lowercase();
                let vb = note_field(b, &field).to_lowercase();
                if desc { vb.cmp(&va) } else { va.cmp(&vb) }
            });
        }

        // Limit
        if let Some(n) = limit {
            notes.truncate(n);
        }

        // Determine columns from `order` or `properties`
        let cols: Vec<String> = if let Some(ref ord) = order {
            ord.iter()
                .filter(|f| *f != "file.name" && *f != "file.link")
                .cloned()
                .collect()
        } else {
            properties.keys().cloned().collect()
        };

        let view_html = match view_type {
            "list" => render_list_view(&notes, view_name),
            "table" | _ => render_table_view(&notes, &cols, &properties, view_name, group_by.as_ref().map(|(p, _)| p.as_str())),
        };
        html.push_str(&view_html);
    }
    html
}

// ── Note collection + filtering ────────────────────────────────────────────────

fn collect_notes<'v>(
    vault: &'v Vault,
    global_filter: Option<&Value>,
    view_filter: Option<&Value>,
) -> Vec<&'v NoteEntry> {
    vault
        .notes
        .values()
        .filter(|n| n.kind == crate::vault::NoteKind::Markdown && n.is_published)
        .filter(|n| {
            let gf = global_filter.map(|f| eval_filter(f, n)).unwrap_or(true);
            let vf = view_filter.map(|f| eval_filter(f, n)).unwrap_or(true);
            gf && vf
        })
        .collect()
}

fn eval_filter(filter: &Value, note: &NoteEntry) -> bool {
    match filter {
        Value::String(s) => eval_expr(s.trim(), note),
        Value::Mapping(map) => {
            if let Some(and_list) = map.get("and").and_then(|v| v.as_sequence()) {
                return and_list.iter().all(|f| eval_filter(f, note));
            }
            if let Some(or_list) = map.get("or").and_then(|v| v.as_sequence()) {
                return or_list.iter().any(|f| eval_filter(f, note));
            }
            if let Some(not_list) = map.get("not").and_then(|v| v.as_sequence()) {
                return !not_list.iter().any(|f| eval_filter(f, note));
            }
            true
        }
        Value::Sequence(seq) => seq.iter().all(|f| eval_filter(f, note)),
        _ => true,
    }
}

fn eval_expr(expr: &str, note: &NoteEntry) -> bool {
    // file.hasTag("x")
    if let Some(inner) = expr.strip_prefix("file.hasTag(").and_then(|s| s.strip_suffix(')')) {
        let tag = inner.trim().trim_matches('"').trim_matches('\'').to_lowercase();
        return note.tags.iter().any(|t| t.to_lowercase() == tag)
            || note.frontmatter.get("tags").map(|t| t.to_lowercase().contains(&tag)).unwrap_or(false);
    }
    // file.inFolder("x")
    if let Some(inner) = expr.strip_prefix("file.inFolder(").and_then(|s| s.strip_suffix(')')) {
        let folder = inner.trim().trim_matches('"').trim_matches('\'');
        let folder = folder.trim_end_matches('/');
        return note.slug.starts_with(&format!("{}/", folder))
            || note.slug == folder;
    }
    // file.hasLink("x")
    if let Some(inner) = expr.strip_prefix("file.hasLink(").and_then(|s| s.strip_suffix(')')) {
        let target = inner.trim().trim_matches('"').trim_matches('\'').to_lowercase();
        return note.links_to.iter().any(|l| l.to_lowercase() == target);
    }

    // Comparison operators
    for op in &["!=", ">=", "<=", ">", "<", "==", "="] {
        if let Some(pos) = expr.find(op) {
            // Make sure we don't split inside a function call
            let lhs = expr[..pos].trim();
            let rhs = expr[pos + op.len()..].trim().trim_matches('"').trim_matches('\'');
            let lv = note_field(note, lhs).to_lowercase();
            let rv = rhs.to_lowercase();
            return match *op {
                "=" | "==" => lv == rv,
                "!=" => lv != rv,
                ">" => lv > rv,
                "<" => lv < rv,
                ">=" => lv >= rv,
                "<=" => lv <= rv,
                _ => false,
            };
        }
    }
    true
}

// ── Field access ──────────────────────────────────────────────────────────────

fn note_field(note: &NoteEntry, field: &str) -> String {
    let f = field.trim();
    match f.to_lowercase().as_str() {
        "file.name" | "file.title" | "file.link" => note.title.clone(),
        "file.path" => note.path.clone(),
        "file.slug" => note.slug.clone(),
        "file.ext" => std::path::Path::new(&note.path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string(),
        "file.folder" => std::path::Path::new(&note.path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .replace('\\', "/"),
        "file.tags" | "tags" => note.tags.join(", "),
        other => {
            // Strip note. prefix
            let key = other
                .strip_prefix("note.")
                .unwrap_or(other)
                .strip_prefix("formula.")
                .unwrap_or(other);
            note.frontmatter
                .get(key)
                .or_else(|| note.frontmatter.get(other))
                .cloned()
                .unwrap_or_default()
        }
    }
}

// ── Properties parsing ────────────────────────────────────────────────────────

fn parse_properties(val: Option<&Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(Value::Mapping(m)) = val else { return map };
    for (k, v) in m {
        let key = k.as_str().unwrap_or("").to_string();
        let display = v
            .get("displayName")
            .and_then(|d| d.as_str())
            .unwrap_or(&key)
            .to_string();
        if !key.is_empty() {
            map.insert(key, display);
        }
    }
    map
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render_table_view(
    notes: &[&NoteEntry],
    cols: &[String],
    properties: &HashMap<String, String>,
    name: Option<&str>,
    group_by: Option<&str>,
) -> String {
    let mut s = String::new();
    if let Some(n) = name {
        s.push_str(&format!("<h3 class=\"bases-view-name\">{}</h3>\n", html_escape(n)));
    }

    if notes.is_empty() {
        s.push_str("<p class=\"bases-empty\"><em>(Keine Ergebnisse)</em></p>\n");
        return s;
    }

    // Determine effective columns
    let effective_cols: Vec<(String, String)> = if cols.is_empty() {
        properties
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    } else {
        cols.iter()
            .map(|c| {
                let display = properties.get(c).cloned().unwrap_or_else(|| {
                    c.split('.').last().unwrap_or(c).to_string()
                });
                (c.clone(), display)
            })
            .collect()
    };

    s.push_str("<div class=\"bases-table-wrap\"><table class=\"bases-table\">\n");
    s.push_str("<thead><tr>\n  <th>Datei</th>\n");
    for (_, display) in &effective_cols {
        s.push_str(&format!("  <th>{}</th>\n", html_escape(display)));
    }
    s.push_str("</tr></thead>\n<tbody>\n");

    let mut current_group: Option<String> = None;

    for note in notes {
        // GroupBy separator row
        if let Some(gb) = group_by {
            let group_val = note_field(note, gb);
            if current_group.as_deref() != Some(&group_val) {
                current_group = Some(group_val.clone());
                let col_count = effective_cols.len() + 1;
                s.push_str(&format!(
                    "<tr class=\"bases-group-row\"><td colspan=\"{}\">{}</td></tr>\n",
                    col_count,
                    html_escape(if group_val.is_empty() { "—" } else { &group_val })
                ));
            }
        }

        s.push_str("<tr>\n");
        s.push_str(&format!(
            "  <td><a href=\"/note/{}\" class=\"internal\">{}</a></td>\n",
            html_escape(&note.slug),
            html_escape(&note.title),
        ));
        for (field, _) in &effective_cols {
            let val = note_field(note, field);
            s.push_str(&format!("  <td>{}</td>\n", html_escape(&val)));
        }
        s.push_str("</tr>\n");
    }

    s.push_str("</tbody></table></div>\n");
    s
}

fn render_list_view(notes: &[&NoteEntry], name: Option<&str>) -> String {
    let mut s = String::new();
    if let Some(n) = name {
        s.push_str(&format!("<h3 class=\"bases-view-name\">{}</h3>\n", html_escape(n)));
    }
    if notes.is_empty() {
        s.push_str("<p class=\"bases-empty\"><em>(Keine Ergebnisse)</em></p>\n");
        return s;
    }
    s.push_str("<ul class=\"bases-list\">\n");
    for note in notes {
        s.push_str(&format!(
            "  <li><a href=\"/note/{}\" class=\"internal\">{}</a></li>\n",
            html_escape(&note.slug),
            html_escape(&note.title),
        ));
    }
    s.push_str("</ul>\n");
    s
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
