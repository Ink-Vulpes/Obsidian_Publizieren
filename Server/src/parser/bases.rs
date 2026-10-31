/// Obsidian Bases renderer.
///
/// Parses a `.base` YAML file (or an inline `base` code block) and renders
/// a read-only HTML view for the web server.
///
/// Supported:
/// - Filters: global + per-view; `and` / `or` / `not` trees; string/number
///   comparisons (==, !=, <, <=, >, >=); `file.hasTag()`, `file.inFolder()`,
///   `file.hasLink()`; `.toString().contains()` / `.includes()`.
/// - Views: table (with columns from `order`) and list.
/// - Sort: `sort` list of `{property, direction}` objects.
/// - Limit: `limit` rows.
/// - GroupBy: groups rows by a property value.
/// - `order`: column list for table views.
/// - `properties`: `displayName` for column headers.
///
/// Not supported: formulas, summaries, `cards` / `kanban` / `map` views,
/// `this.*`, `columnSize`.

use crate::vault::{NoteEntry, Vault};
use serde_yaml::Value;
use std::collections::HashMap;

// ── Public API ────────────────────────────────────────────────────────────────

/// Render a `.base` file's YAML content into HTML.
/// `sees_all`: if true, unpublished notes are also included (admin / sees_all users).
pub fn render_base(yaml: &str, vault: &Vault, sees_all: bool) -> String {
    match serde_yaml::from_str::<Value>(yaml) {
        Ok(doc) => render_doc(&doc, vault, sees_all),
        Err(e) => format!(
            "<div class=\"bases-error\">⚠ Bases-Fehler: {}</div>",
            html_escape(&e.to_string())
        ),
    }
}

/// Replace all ` ```base ` code blocks in `content` with rendered HTML.
pub fn render_base_blocks(content: &str, vault: &Vault, sees_all: bool) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("```base") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 3..]; // skip ```
        let lang_end = after.find('\n').unwrap_or(after.len());
        let body_start = &after[lang_end..];

        if let Some(end) = body_start.find("\n```") {
            let yaml = body_start[..end].trim();
            result.push_str(&render_base(yaml, vault, sees_all));
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

fn render_doc(doc: &Value, vault: &Vault, sees_all: bool) -> String {
    let global_filter = doc.get("filters");
    let properties = parse_properties(doc.get("properties"));

    let views = match doc.get("views").and_then(|v| v.as_sequence()) {
        Some(seq) if !seq.is_empty() => seq.to_vec(),
        _ => {
            let notes = collect_notes(vault, global_filter, None, sees_all);
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

        // `order` = column list (which properties to display as columns)
        let order_cols: Option<Vec<String>> = view
            .get("order")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            });

        // `sort` = sort specification [{property, direction}, ...]
        let sort_spec: Vec<(String, bool)> = view
            .get("sort")
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|entry| {
                        let prop = entry.get("property").and_then(|p| p.as_str())?;
                        let desc = entry
                            .get("direction")
                            .and_then(|d| d.as_str())
                            .map(|d| d.to_uppercase() == "DESC")
                            .unwrap_or(false);
                        Some((prop.to_string(), desc))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let group_by: Option<(String, bool)> = view.get("groupBy").and_then(|gb| {
            let prop = gb.get("property").and_then(|p| p.as_str())?;
            let desc = gb
                .get("direction")
                .and_then(|d| d.as_str())
                .map(|d| d.to_uppercase() == "DESC")
                .unwrap_or(false);
            Some((prop.to_string(), desc))
        });

        let mut notes = collect_notes(vault, global_filter, view_filter, sees_all);

        // Primary sort by `sort` spec
        if !sort_spec.is_empty() {
            notes.sort_by(|a, b| {
                for (field, desc) in &sort_spec {
                    let va = note_field(a, field).to_lowercase();
                    let vb = note_field(b, field).to_lowercase();
                    let ord = va.cmp(&vb);
                    if ord != std::cmp::Ordering::Equal {
                        return if *desc { ord.reverse() } else { ord };
                    }
                }
                std::cmp::Ordering::Equal
            });
        } else {
            // Default: sort by title ascending
            notes.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }

        // Secondary sort for groupBy — must be stable relative to primary sort
        if let Some((ref gb_prop, desc)) = group_by {
            let field = gb_prop.clone();
            notes.sort_by(|a, b| {
                let va = note_field(a, &field).to_lowercase();
                let vb = note_field(b, &field).to_lowercase();
                let ord = va.cmp(&vb);
                if desc { ord.reverse() } else { ord }
            });
        }

        // Limit
        if let Some(n) = limit {
            notes.truncate(n);
        }

        // Determine display columns from `order`, filtering out file.* special names
        // that aren't real property columns (file.name is always shown as the link).
        let cols: Vec<String> = if let Some(ref ord) = order_cols {
            ord.iter()
                .filter(|f| {
                    let lower = f.to_lowercase();
                    lower != "file.name"
                        && lower != "file.link"
                        && lower != "file.title"
                        && lower != "file.path"
                        && lower != "file.slug"
                        && lower != "file.folder"
                        && lower != "file.ext"
                        && lower != "file.mtime"
                        && lower != "file.ctime"
                })
                .cloned()
                .collect()
        } else {
            properties.keys().cloned().collect()
        };

        let view_html = match view_type {
            "list" => render_list_view(&notes, view_name),
            _ => render_table_view(
                &notes,
                &cols,
                &properties,
                view_name,
                group_by.as_ref().map(|(p, _)| p.as_str()),
            ),
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
    sees_all: bool,
) -> Vec<&'v NoteEntry> {
    vault
        .notes
        .values()
        .filter(|n| n.kind == crate::vault::NoteKind::Markdown && (sees_all || n.is_published))
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
            || note
                .frontmatter
                .get("tags")
                .map(|t| t.to_lowercase().contains(&tag))
                .unwrap_or(false);
    }
    // file.inFolder("x") — matches note in the folder or any sub-folder
    if let Some(inner) = expr.strip_prefix("file.inFolder(").and_then(|s| s.strip_suffix(')')) {
        let folder = inner.trim().trim_matches('"').trim_matches('\'');
        let folder = folder.trim_end_matches('/');
        return note.slug.starts_with(&format!("{}/", folder)) || note.slug == folder;
    }
    // file.hasLink("x")
    if let Some(inner) = expr.strip_prefix("file.hasLink(").and_then(|s| s.strip_suffix(')')) {
        let target = inner.trim().trim_matches('"').trim_matches('\'').to_lowercase();
        return note.links_to.iter().any(|l| l.to_lowercase() == target);
    }

    // property.toString().contains("value") — case-insensitive substring match
    if let Some(pos) = expr.find(".toString().contains(") {
        let field = &expr[..pos];
        let rest = &expr[pos + ".toString().contains(".len()..];
        if let Some(rest2) = rest.strip_suffix(')') {
            let needle = rest2.trim().trim_matches('"').trim_matches('\'').to_lowercase();
            let val = note_field(note, field).to_lowercase();
            return val.contains(&needle);
        }
    }
    // property.includes("value") — same as contains
    if let Some(pos) = expr.find(".includes(") {
        let field = &expr[..pos];
        let rest = &expr[pos + ".includes(".len()..];
        if let Some(rest2) = rest.strip_suffix(')') {
            let needle = rest2.trim().trim_matches('"').trim_matches('\'').to_lowercase();
            let val = note_field(note, field).to_lowercase();
            return val.contains(&needle);
        }
    }

    // Comparison operators — longest first to avoid ">=" being split as ">"
    for op in &["!=", ">=", "<=", ">", "<", "==", "="] {
        if let Some(pos) = find_op(expr, op) {
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

/// Find operator position, ignoring occurrences inside parentheses or quotes.
fn find_op(expr: &str, op: &str) -> Option<usize> {
    let bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    let mut depth = 0usize;
    let mut in_quote = false;
    let mut quote_char = b'"';
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_quote {
            if b == quote_char {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_quote = true;
            quote_char = b;
            i += 1;
            continue;
        }
        if b == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if b == b')' {
            depth = depth.saturating_sub(1);
            i += 1;
            continue;
        }
        if depth == 0 && bytes[i..].starts_with(op_bytes) {
            // For ">" make sure it's not part of ">="
            if op == ">" && bytes.get(i + 1) == Some(&b'=') {
                i += 1;
                continue;
            }
            if op == "<" && bytes.get(i + 1) == Some(&b'=') {
                i += 1;
                continue;
            }
            // For "=" make sure it's not part of "==" or "!=" or ">=" or "<="
            if op == "=" {
                if i > 0 && (bytes[i - 1] == b'!' || bytes[i - 1] == b'>' || bytes[i - 1] == b'<') {
                    i += 1;
                    continue;
                }
                if bytes.get(i + 1) == Some(&b'=') {
                    i += 1;
                    continue;
                }
            }
            return Some(i);
        }
        i += 1;
    }
    None
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
        "file.folder" => {
            // Return the direct parent folder path (not just the last segment)
            let p = note.path.replace('\\', "/");
            let normalized = p.trim_end_matches('/');
            if let Some(slash) = normalized.rfind('/') {
                normalized[..slash].to_string()
            } else {
                String::new()
            }
        }
        "file.tags" | "tags" => note.tags.join(", "),
        "file.mtime" | "file.ctime" => String::new(), // not available server-side
        _ => {
            // Strip note. prefix so "note.Kategorien" → "Kategorien"
            // Use ORIGINAL case (f, not lowercased) for frontmatter lookups
            let key = f.strip_prefix("note.").unwrap_or(f);
            // Try original case first, then lowercase fallback
            note.frontmatter
                .get(key)
                .or_else(|| note.frontmatter.get(&key.to_lowercase()))
                .cloned()
                .unwrap_or_default()
        }
    }
}

// ── Properties parsing ────────────────────────────────────────────────────────

fn parse_properties(val: Option<&Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(Value::Mapping(m)) = val else {
        return map;
    };
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
        s.push_str(&format!(
            "<h3 class=\"bases-view-name\">{}</h3>\n",
            html_escape(n)
        ));
    }

    if notes.is_empty() {
        s.push_str("<p class=\"bases-empty\"><em>(Keine Ergebnisse)</em></p>\n");
        return s;
    }

    // Effective columns: use `cols` if provided, else fall back to properties map
    let effective_cols: Vec<(String, String)> = if cols.is_empty() {
        let mut v: Vec<(String, String)> = properties
            .iter()
            .map(|(k, display)| (k.clone(), display.clone()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    } else {
        cols.iter()
            .map(|c| {
                let display = properties.get(c).cloned().unwrap_or_else(|| {
                    // Strip note. prefix for display
                    let raw = c.strip_prefix("note.").unwrap_or(c);
                    raw.split('.').last().unwrap_or(raw).to_string()
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
            // Render multi-value fields (comma-separated) as tags
            let cell = if val.contains(',') {
                let tags: Vec<&str> = val.split(',').map(|t| t.trim()).filter(|t| !t.is_empty()).collect();
                if tags.len() > 1 {
                    let inner: String = tags
                        .iter()
                        .map(|t| format!("<span class=\"bases-tag\">{}</span>", html_escape(t)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    format!("<div class=\"bases-tags\">{}</div>", inner)
                } else {
                    html_escape(&val)
                }
            } else {
                html_escape(&val)
            };
            s.push_str(&format!("  <td>{}</td>\n", cell));
        }
        s.push_str("</tr>\n");
    }

    s.push_str("</tbody></table></div>\n");
    s
}

fn render_list_view(notes: &[&NoteEntry], name: Option<&str>) -> String {
    let mut s = String::new();
    if let Some(n) = name {
        s.push_str(&format!(
            "<h3 class=\"bases-view-name\">{}</h3>\n",
            html_escape(n)
        ));
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
