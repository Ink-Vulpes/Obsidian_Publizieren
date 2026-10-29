use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use once_cell::sync::Lazy;
use regex::Regex;

pub fn parse_frontmatter_fields(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(fm) = extract_frontmatter(content) else { return map };
    for line in fm.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').trim_matches('\'').to_string();
            if !key.is_empty() && !key.starts_with('#') && !key.starts_with('-') {
                map.insert(key, val);
            }
        }
    }
    map
}

static WIKILINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[\[([^\]|#\n]+?)(?:[|#][^\]]*)?\]\]").unwrap());

static INLINE_TAG_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|\s)#([a-zA-Z][a-zA-Z0-9_/-]*)").unwrap());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    Markdown,
    Canvas,
    Excalidraw,
    Base,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteEntry {
    pub path: String,
    pub title: String,
    pub slug: String,
    pub kind: NoteKind,
    pub tags: Vec<String>,
    pub links_to: Vec<String>,
    /// true when publish_field is empty (no filter) or when the note's frontmatter
    /// has publish_field: true.
    pub is_published: bool,
    /// All frontmatter key-value pairs (string representation).
    pub frontmatter: HashMap<String, String>,
}

/// Extensions that are served as vault assets (not rendered as notes).
const ASSET_EXTS: &[&str] = &[
    "pdf", "png", "jpg", "jpeg", "gif", "svg", "webp", "avif",
    "mp4", "webm", "mp3", "ogg", "wav",
    "zip", "docx", "xlsx", "pptx",
];

fn is_asset_ext(ext: &str) -> bool {
    ASSET_EXTS.contains(&ext.to_lowercase().as_str())
}

#[derive(Debug)]
pub struct Vault {
    pub root: PathBuf,
    pub notes: HashMap<String, NoteEntry>,
    pub backlinks: HashMap<String, Vec<String>>,
    pub title_to_slug: HashMap<String, String>,
    pub search_index: HashMap<String, String>,
    /// filename.lowercase() → vault-relative path (e.g. "doc.pdf" → "04 Anderes/doc.pdf")
    pub asset_paths: HashMap<String, String>,
}

impl Vault {
    /// `publish_field`: frontmatter key to check for `true`; empty string = no filter (all visible).
    pub fn load(root: &str, publish_field: &str) -> anyhow::Result<Self> {
        let root = PathBuf::from(root);
        if !root.exists() {
            std::fs::create_dir_all(&root)?;
        }

        let mut notes: HashMap<String, NoteEntry> = HashMap::new();
        let mut title_to_slug: HashMap<String, String> = HashMap::new();
        let mut search_index: HashMap<String, String> = HashMap::new();
        let mut asset_paths: HashMap<String, String> = HashMap::new();

        for entry in WalkDir::new(&root)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                !e.file_name().to_string_lossy().starts_with('.')
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Walker error: {}", e);
                    continue;
                }
            };

            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            // Track asset files for link resolution
            if is_asset_ext(ext) {
                let rel_path = path
                    .strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let filename = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                asset_paths.entry(filename).or_insert(rel_path);
                continue;
            }

            let kind = match ext {
                "md" => {
                    if is_excalidraw_md(path) {
                        NoteKind::Excalidraw
                    } else {
                        NoteKind::Markdown
                    }
                }
                "canvas" => NoteKind::Canvas,
                "excalidraw" => NoteKind::Excalidraw,
                "base" => NoteKind::Base,
                _ => continue,
            };

            let rel_path = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");

            let title = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string();

            let slug = rel_path
                .strip_suffix(".md")
                .or_else(|| rel_path.strip_suffix(".canvas"))
                .or_else(|| rel_path.strip_suffix(".excalidraw"))
                .or_else(|| rel_path.strip_suffix(".base"))
                .unwrap_or(&rel_path)
                .to_string();

            let (tags, links_to) = if kind == NoteKind::Markdown {
                extract_note_metadata(path)
            } else {
                (vec![], vec![])
            };

            let (is_published, frontmatter) = if kind == NoteKind::Markdown {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let plain = strip_markdown(&content);
                    search_index.insert(slug.clone(), plain);
                    let published = if publish_field.is_empty() {
                        true
                    } else {
                        extract_frontmatter_bool(&content, publish_field)
                    };
                    let fm = parse_frontmatter_fields(&content);
                    (published, fm)
                } else {
                    (false, HashMap::new())
                }
            } else if kind == NoteKind::Base {
                // .base files are always "published" so they can be resolved for embeds
                (true, HashMap::new())
            } else {
                (false, HashMap::new())
            };

            title_to_slug.insert(title.to_lowercase(), slug.clone());

            notes.insert(
                slug.clone(),
                NoteEntry {
                    path: rel_path,
                    title,
                    slug,
                    kind,
                    tags,
                    links_to,
                    is_published,
                    frontmatter,
                },
            );
        }

        let mut backlinks: HashMap<String, Vec<String>> = HashMap::new();
        let entries: Vec<NoteEntry> = notes.values().cloned().collect();
        for note in &entries {
            for link_title in &note.links_to {
                let target = title_to_slug
                    .get(&link_title.to_lowercase())
                    .cloned()
                    .unwrap_or_else(|| link_title.clone());
                backlinks
                    .entry(target)
                    .or_default()
                    .push(note.slug.clone());
            }
        }

        tracing::info!("Loaded {} notes and {} assets from vault", notes.len(), asset_paths.len());
        Ok(Vault {
            root,
            notes,
            backlinks,
            title_to_slug,
            search_index,
            asset_paths,
        })
    }

    pub fn find_by_title(&self, title: &str) -> Option<&NoteEntry> {
        let slug = self.title_to_slug.get(&title.to_lowercase())?;
        self.notes.get(slug)
    }

    pub fn resolve_link(&self, title: &str) -> Option<String> {
        // Asset files (PDF, images, …) → /vault-assets/
        let ext = std::path::Path::new(title)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if is_asset_ext(ext) {
            return self.resolve_asset(title);
        }
        // Regular notes → /note/
        let slug = self.title_to_slug.get(&title.to_lowercase())?;
        Some(format!("/note/{}", slug))
    }

    /// Resolve an asset filename to its /vault-assets/ URL.
    pub fn resolve_asset(&self, name: &str) -> Option<String> {
        let key = std::path::Path::new(name)
            .file_name()?
            .to_string_lossy()
            .to_lowercase();
        let rel = self.asset_paths.get(&key)?;
        Some(format!(
            "/vault-assets/{}",
            rel.split('/').map(urlencoding::encode).collect::<Vec<_>>().join("/")
        ))
    }

    pub fn get_backlinks(&self, slug: &str) -> Vec<&NoteEntry> {
        self.backlinks
            .get(slug)
            .map(|slugs| {
                slugs
                    .iter()
                    .filter_map(|s| self.notes.get(s.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn is_excalidraw_md(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|c| {
            c.contains("excalidraw-plugin:")
                || c.contains("tags: [excalidraw]")
                || c.contains("tags:\n  - excalidraw")
        })
        .unwrap_or(false)
}

fn extract_note_metadata(path: &Path) -> (Vec<String>, Vec<String>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (vec![], vec![]),
    };
    (extract_tags(&content), extract_wikilinks(&content))
}

pub fn extract_tags(content: &str) -> Vec<String> {
    let mut tags = vec![];

    if let Some(fm) = extract_frontmatter(content) {
        let mut in_tags = false;
        for line in fm.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("tags:") {
                let rest = trimmed["tags:".len()..].trim();
                if rest.starts_with('[') {
                    let inner = rest.trim_matches(|c| c == '[' || c == ']');
                    for t in inner.split(',') {
                        let t = t.trim().trim_matches('"').trim_matches('\'');
                        if !t.is_empty() {
                            tags.push(t.to_string());
                        }
                    }
                    in_tags = false;
                } else {
                    in_tags = true;
                }
            } else if in_tags && trimmed.starts_with("- ") {
                tags.push(trimmed[2..].trim().to_string());
            } else if in_tags && !trimmed.is_empty() && !trimmed.starts_with('-') {
                in_tags = false;
            }
        }
    }

    for cap in INLINE_TAG_RE.captures_iter(content) {
        tags.push(cap[1].to_string());
    }

    tags.sort();
    tags.dedup();
    tags
}

pub fn extract_wikilinks(content: &str) -> Vec<String> {
    WIKILINK_RE
        .captures_iter(content)
        .map(|c| c[1].trim().to_string())
        .collect()
}

pub fn extract_frontmatter(content: &str) -> Option<&str> {
    let c = content.trim_start_matches('\u{feff}');
    if !c.starts_with("---") {
        return None;
    }
    let after = c.get(3..)?;
    let end = after.find("\n---")?;
    Some(&after[..end])
}

/// Returns true when the frontmatter contains `field: true` (case-insensitive value).
pub fn extract_frontmatter_bool(content: &str, field: &str) -> bool {
    let Some(fm) = extract_frontmatter(content) else { return false };
    let prefix = format!("{}:", field);
    for line in fm.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with(&prefix.to_lowercase()) {
            let value = trimmed[prefix.len()..].trim().to_lowercase();
            return value == "true";
        }
    }
    false
}

fn strip_markdown(content: &str) -> String {
    let body = if let Some(fm_end) = content.find("\n---\n") {
        let after = &content[fm_end + 5..];
        after
    } else if content.starts_with("---") {
        content
    } else {
        content
    };

    let re_code = Regex::new(r"```[\s\S]*?```").unwrap();
    let re_links = Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap();
    let re_md_links = Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap();
    let re_headings = Regex::new(r"#+\s+").unwrap();
    let re_html = Regex::new(r"<[^>]+>").unwrap();
    let re_ws = Regex::new(r"\s+").unwrap();

    let s = re_code.replace_all(body, " ");
    let s = re_links.replace_all(&s, "$1");
    let s = re_md_links.replace_all(&s, "$1");
    let s = re_headings.replace_all(&s, "");
    let s = re_html.replace_all(&s, "");
    let s = re_ws.replace_all(&s, " ");
    s.trim().to_string()
}
