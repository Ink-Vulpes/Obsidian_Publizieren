use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;

use crate::vault::{NoteKind, Vault};

static WIKILINK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[\[([^\]|#\n]+?)(?:#([^\]|]*))?(?:\|([^\]]*))?\]\]").unwrap()
});
static EMBED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"!\[\[([^\]]+)\]\]").unwrap());
static HEADING_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)<h([1-6])>(.*?)</h[1-6]>").unwrap());
static TAG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static CALLOUT_RE: Lazy<Regex> = Lazy::new(|| {
    // Group 1: type  2: fold(+/-)  3: title (rest of first line)  4: body-in-first-p  5: remaining blockquote
    Regex::new(r"(?s)<blockquote>\s*<p>\[!(\w+)\]([+-])?[ \t]*([^\n]*)\n?(.*?)</p>(.*?)</blockquote>")
        .unwrap()
});
// Math: $$ block first, then $ inline (order matters)
static MATH_BLOCK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)\$\$(.+?)\$\$").unwrap());
static MATH_INLINE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\$([^\$\n]+?)\$").unwrap());

#[derive(Debug, Clone, Serialize)]
pub struct TocEntry {
    pub level: u8,
    pub title: String,
    pub anchor: String,
}

pub struct ParsedNote {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub html: String,
    pub toc: Vec<TocEntry>,
}

pub fn parse_markdown(content: &str, vault: &Vault, sees_all: bool) -> ParsedNote {
    let (fm, body) = split_frontmatter(content);
    let frontmatter = parse_frontmatter_map(fm);

    let title = frontmatter
        .get("title")
        .cloned()
        .unwrap_or_default();
    let description = frontmatter
        .get("description")
        .or_else(|| frontmatter.get("summary"))
        .cloned()
        .unwrap_or_default();
    let tags = crate::vault::extract_tags(content);

    // Render dataview and base blocks before other processing
    let body = &crate::parser::dataview::render_dataview_blocks(body, vault);
    let body = &crate::parser::bases::render_base_blocks(body, vault, sees_all);
    // Protect math expressions BEFORE comrak so _ * < > are not mangled
    let (body_no_math, math_slots) = protect_math(body);
    let processed = preprocess_embeds(&body_no_math, vault, sees_all);
    let processed = preprocess_wikilinks(&processed, vault);

    let mut opts = comrak::Options::default();
    opts.extension.strikethrough = true;
    opts.extension.table = true;
    opts.extension.autolink = true;
    opts.extension.tasklist = true;
    opts.extension.footnotes = true;
    opts.render.unsafe_ = true;

    let raw_html = comrak::markdown_to_html(&processed, &opts);
    // Restore math AFTER comrak, before further post-processing
    let raw_html = restore_math(raw_html, &math_slots);
    let html = postprocess_callouts(&raw_html);
    let (html, toc) = inject_heading_ids(html);

    ParsedNote {
        title,
        description,
        tags,
        html,
        toc,
    }
}

fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let c = content.trim_start_matches('\u{feff}');
    if !c.starts_with("---") {
        return (None, c);
    }
    let after = &c[3..];
    if let Some(pos) = after.find("\n---") {
        (Some(&after[..pos]), after[pos + 4..].trim_start_matches('\n'))
    } else {
        (None, c)
    }
}

fn parse_frontmatter_map(fm: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let fm = match fm {
        Some(s) => s,
        None => return map,
    };
    for line in fm.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
            if !key.is_empty() && !key.starts_with('#') {
                map.insert(key, val);
            }
        }
    }
    map
}

fn preprocess_wikilinks(content: &str, vault: &Vault) -> String {
    // Protect code blocks and inline code from wikilink substitution
    static CODE_BLOCK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?s)```[^`]*?```|`[^`\n]+`").unwrap());

    let mut slots: Vec<String> = Vec::new();
    let protected = CODE_BLOCK_RE.replace_all(content, |caps: &regex::Captures| {
        let idx = slots.len();
        slots.push(caps[0].to_string());
        format!("\x00CODEBLOCK{}\x00", idx)
    });

    let replaced = WIKILINK_RE
        .replace_all(&protected, |caps: &regex::Captures| {
            let target = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            let heading = caps.get(2).map(|m| m.as_str());
            let alias = caps.get(3).map(|m| m.as_str().trim());

            let display = alias.unwrap_or(target);
            let base_href = vault
                .resolve_link(target)
                .unwrap_or_else(|| format!("/note/{}", urlencoding::encode(target)));

            let href = match heading {
                Some(h) if !h.is_empty() => format!(
                    "{}#{}",
                    base_href,
                    h.to_lowercase().replace(' ', "-")
                ),
                _ => base_href,
            };

            // Determine if this is an asset link (PDF, image, …)
            let ext = std::path::Path::new(target)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let is_asset = matches!(
                ext.to_lowercase().as_str(),
                "pdf" | "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "avif"
                | "mp4" | "webm" | "mp3" | "ogg" | "wav"
                | "zip" | "docx" | "xlsx" | "pptx"
            );

            let broken = if is_asset {
                if vault.resolve_asset(target).is_some() { "" } else { " broken" }
            } else if vault.find_by_title(target).is_some() {
                ""
            } else {
                " broken"
            };

            let extra = if is_asset { r#" target="_blank" rel="noopener""# } else { "" };

            format!(
                r#"<a href="{}" class="internal{}"{} >{}</a>"#,
                href, broken, extra, display
            )
        })
        .into_owned();

    // Restore protected code sections
    let mut result = replaced;
    for (i, slot) in slots.iter().enumerate() {
        result = result.replace(&format!("\x00CODEBLOCK{}\x00", i), slot);
    }
    result
}

fn preprocess_embeds(content: &str, vault: &Vault, sees_all: bool) -> String {
    EMBED_RE
        .replace_all(content, |caps: &regex::Captures| {
            let raw = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            // Strip optional |size or |alias suffix: ![[image.png|200]] → "image.png"
            let target = raw.splitn(2, '|').next().unwrap_or(raw).trim();
            let lower = target.to_lowercase();

            if lower.ends_with(".png")
                || lower.ends_with(".jpg")
                || lower.ends_with(".jpeg")
                || lower.ends_with(".gif")
                || lower.ends_with(".svg")
                || lower.ends_with(".webp")
                || lower.ends_with(".avif")
            {
                // Resolve full vault path via asset index
                let src = vault
                    .resolve_asset(target)
                    .unwrap_or_else(|| format!("/vault-assets/{}", urlencoding::encode(target)));
                return format!(
                    "\n\n<figure class=\"embed-image\"><img src=\"{}\" alt=\"{}\" loading=\"lazy\"></figure>\n\n",
                    src,
                    html_escape(target)
                );
            }

            if lower.ends_with(".pdf") {
                let href = vault
                    .resolve_asset(target)
                    .unwrap_or_else(|| format!("/vault-assets/{}", urlencoding::encode(target)));
                return format!(
                    "\n\n<div class=\"embed-pdf\"><a href=\"{}\" target=\"_blank\" rel=\"noopener\">📄 {}</a></div>\n\n",
                    href,
                    html_escape(target)
                );
            }

            // Excalidraw embed: render inline as SVG
            if lower.ends_with(".excalidraw") {
                if let Some(entry) = vault.find_by_title(target) {
                    if entry.kind == NoteKind::Excalidraw {
                        let file_path = vault.root.join(&entry.path);
                        if let Ok(file_content) = std::fs::read_to_string(&file_path) {
                            if let Some(mut data) = super::excalidraw::parse_excalidraw(&file_content) {
                                super::excalidraw::resolve_embedded_files(
                                    &mut data,
                                    &vault.root,
                                    &vault.asset_paths,
                                );
                                if let Ok(json) = serde_json::to_string(&data) {
                                    return format!(
                                        "\n\n<div class=\"excalidraw-embed\" data-excalidraw='{}' title=\"{}\">\
                                         <span class=\"excalidraw-loading\">Loading diagram…</span></div>\n\n",
                                        json.replace('\'', "&#39;"),
                                        html_escape(target)
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Bases embed: render the .base file inline as a table/list
            if lower.ends_with(".base") {
                if let Some(entry) = vault.find_by_title(target) {
                    let file_path = vault.root.join(&entry.path);
                    if let Ok(yaml) = std::fs::read_to_string(&file_path) {
                        let inner = super::bases::render_base(&yaml, vault, sees_all);
                        return format!(
                            "\n\n<div class=\"bases-embed\" data-source=\"{}\">{}</div>\n\n",
                            html_escape(target),
                            inner
                        );
                    }
                }
            }

            let href = vault
                .resolve_link(target)
                .unwrap_or_else(|| format!("/note/{}", urlencoding::encode(target)));
            format!(
                "\n\n<div class=\"embed-note\"><a href=\"{}\" class=\"internal\">📎 {}</a></div>\n\n",
                href,
                html_escape(target)
            )
        })
        .into_owned()
}

fn postprocess_callouts(html: &str) -> String {
    CALLOUT_RE
        .replace_all(html, |caps: &regex::Captures| {
            let kind = caps
                .get(1)
                .map(|m| m.as_str().to_lowercase())
                .unwrap_or_default();
            let fold = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            let title_text = caps.get(3).map(|m| m.as_str()).unwrap_or("");
            // Body = remainder of first <p> after title line + any extra blockquote content
            let body_in_p = caps.get(4).map(|m| m.as_str()).unwrap_or("").trim();
            let body_extra = caps.get(5).map(|m| m.as_str()).unwrap_or("").trim();
            let body = if body_in_p.is_empty() {
                body_extra.to_string()
            } else if body_extra.is_empty() {
                format!("<p>{}</p>", body_in_p)
            } else {
                format!("<p>{}</p>{}", body_in_p, body_extra)
            };

            let display_title = if title_text.trim().is_empty() {
                capitalize_first(&kind)
            } else {
                title_text.trim().to_string()
            };

            let icon = callout_icon(&kind);
            let foldable = if !fold.is_empty() { " foldable" } else { "" };
            let collapsed = if fold == "-" { " collapsed" } else { "" };
            let fold_btn = if !fold.is_empty() {
                r#"<span class="callout-fold-icon">▾</span>"#
            } else {
                ""
            };

            format!(
                r#"<div class="callout {kind}{foldable}{collapsed}" data-callout="{kind}">
  <div class="callout-title">
    <span class="callout-icon">{icon}</span>
    <span class="callout-title-inner">{title}</span>
    {fold_btn}
  </div>
  <div class="callout-content">{body}</div>
</div>"#,
                kind = kind,
                foldable = foldable,
                collapsed = collapsed,
                icon = icon,
                title = display_title,
                fold_btn = fold_btn,
                body = body,
            )
        })
        .into_owned()
}

fn callout_icon(kind: &str) -> &'static str {
    match kind {
        "note" => "📝",
        "info" | "information" => "ℹ️",
        "tip" | "hint" => "💡",
        "warning" | "caution" | "attention" => "⚠️",
        "danger" | "error" => "🚫",
        "bug" => "🐛",
        "success" | "check" | "done" => "✅",
        "question" | "help" | "faq" => "❓",
        "quote" | "cite" => "💬",
        "abstract" | "summary" | "tldr" => "📋",
        "example" => "📌",
        "todo" => "☐",
        "important" => "❗",
        _ => "📝",
    }
}

fn inject_heading_ids(html: String) -> (String, Vec<TocEntry>) {
    let mut toc = vec![];
    let result = HEADING_RE.replace_all(&html, |caps: &regex::Captures| {
        let level: u8 = caps[1].parse().unwrap_or(1);
        let inner = &caps[2];
        let plain = TAG_RE.replace_all(inner, "").to_string();
        let anchor = slugify(&plain);

        toc.push(TocEntry {
            level,
            title: plain.clone(),
            anchor: anchor.clone(),
        });

        format!(
            "<h{level} id=\"{anchor}\">{inner}<a class=\"anchor-link\" href=\"#{anchor}\" aria-label=\"Link to heading\">#</a></h{level}>",
            level = level,
            anchor = anchor,
            inner = inner,
        )
    });
    (result.into_owned(), toc)
}

fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Extract `$$...$$` and `$...$` expressions, replace with plain ASCII placeholders
/// so comrak cannot mangle their content (underscores, asterisks, angle brackets…).
/// Null bytes are stripped by comrak; we use a unique alphanumeric token instead.
fn protect_math(content: &str) -> (String, Vec<(bool, String)>) {
    let mut slots: Vec<(bool, String)> = Vec::new();

    // Block math first (must come before inline to avoid $$ → two inline $)
    let s = MATH_BLOCK_RE.replace_all(content, |caps: &regex::Captures| {
        let idx = slots.len();
        slots.push((true, caps[1].to_string()));
        format!("OBSMATBLK{}END", idx)
    });

    // Inline math
    let s = MATH_INLINE_RE.replace_all(&s, |caps: &regex::Captures| {
        let idx = slots.len();
        slots.push((false, caps[1].to_string()));
        format!("OBSMATINL{}END", idx)
    });

    (s.into_owned(), slots)
}

/// Restore math placeholders as MathJax-ready `\[...\]` / `\(...\)` markup.
/// Block placeholders may be wrapped in `<p>…</p>` by comrak; those wrappers are removed.
fn restore_math(html: String, slots: &[(bool, String)]) -> String {
    let mut result = html;
    for (i, (is_block, latex)) in slots.iter().enumerate() {
        let (placeholder, replacement) = if *is_block {
            let ph = format!("OBSMATBLK{}END", i);
            let rep = format!(
                "<div class=\"math-block\">\\[{}\\]</div>",
                latex
            );
            (ph, rep)
        } else {
            let ph = format!("OBSMATINL{}END", i);
            let rep = format!(
                "<span class=\"math-inline\">\\({}\\)</span>",
                latex
            );
            (ph, rep)
        };
        // comrak wraps standalone block placeholders in <p>…</p> — strip that wrapper
        let p_wrapped = format!("<p>{}</p>", placeholder);
        result = result.replace(&p_wrapped, &replacement);
        // Also replace any remaining unwrapped occurrences
        result = result.replace(&placeholder, &replacement);
    }
    result
}
