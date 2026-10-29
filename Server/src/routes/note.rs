use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use serde_json::json;

use crate::{auth::CurrentUser, vault::NoteKind, AppState};

pub async fn index(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    let vault = state.vault.read().await;
    let wl = state.whitelist.read().unwrap();
    let bl = state.blacklist.read().unwrap();

    let visible = |n: &&crate::vault::NoteEntry| {
        !bl.contains(&n.slug)
            && (wl.is_empty() || wl.contains(&n.slug))
            && current_user.can_see_note(&n.slug)
            && (current_user.is_admin || n.is_published)
    };
    let first = ["index", "home", "readme"]
        .iter()
        .find_map(|t| vault.find_by_title(t).filter(|n| visible(&n)))
        .or_else(|| vault.notes.values().find(|n| visible(n)))
        .map(|n| n.slug.clone());
    drop(wl);
    drop(bl);
    drop(vault);

    if let Some(slug) = first {
        return Redirect::to(&format!("/note/{}", slug)).into_response();
    }

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("site_description", &state.config.server.site_description);
    ctx.insert("current_user", &current_user);

    let html = state
        .tera
        .render("index.html", &ctx)
        .unwrap_or_else(|_| "<h1>Welcome</h1><p>Upload notes via FTP.</p>".to_string());
    Html(html).into_response()
}

pub async fn render_note(
    Path(path): Path<String>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    let vault = state.vault.read().await;

    let entry = match vault.notes.get(&path) {
        Some(e) => e.clone(),
        None => {
            let title = path.split('/').last().unwrap_or(&path);
            match vault.find_by_title(title) {
                Some(e) => {
                    let slug = e.slug.clone();
                    drop(vault);
                    return Redirect::to(&format!("/note/{}", slug)).into_response();
                }
                None => {
                    drop(vault);
                    return not_found(&state);
                }
            }
        }
    };

    // Whitelist / blacklist / folder-access check
    {
        let bl = state.blacklist.read().unwrap();
        let wl = state.whitelist.read().unwrap();
        if bl.contains(&entry.slug) {
            drop(bl);
            drop(wl);
            drop(vault);
            return not_found(&state);
        }
        if !wl.is_empty() && !wl.contains(&entry.slug) {
            drop(bl);
            drop(wl);
            drop(vault);
            return not_found(&state);
        }
    }
    if !current_user.can_see_note(&entry.slug) {
        drop(vault);
        return not_found(&state);
    }
    if !current_user.is_admin && !entry.is_published {
        drop(vault);
        return access_denied(&state);
    }

    let kind = entry.kind.clone();
    let note_path = vault.root.join(&entry.path);
    let slug = entry.slug.clone();
    let note_title = entry.title.clone();

    let backlinks: Vec<serde_json::Value> = vault
        .get_backlinks(&slug)
        .iter()
        .filter(|n| current_user.can_see_note(&n.slug))
        .map(|n| json!({ "title": n.title, "slug": n.slug }))
        .collect();

    let file_tree = build_file_tree_json_filtered(&vault, &current_user, &state);
    drop(vault);

    match kind {
        NoteKind::Markdown => {
            render_markdown(note_path, slug, note_title, backlinks, file_tree, current_user, state)
                .await
                .into_response()
        }
        NoteKind::Canvas => {
            render_canvas(note_path, slug, note_title, backlinks, file_tree, current_user, state)
                .await
                .into_response()
        }
        NoteKind::Excalidraw => {
            render_excalidraw(note_path, slug, note_title, backlinks, file_tree, current_user, state)
                .await
                .into_response()
        }
        NoteKind::Base => {
            render_base(note_path, slug, note_title, backlinks, file_tree, current_user, state)
                .await
                .into_response()
        }
    }
}

async fn render_markdown(
    path: std::path::PathBuf,
    slug: String,
    fallback_title: String,
    backlinks: Vec<serde_json::Value>,
    file_tree: serde_json::Value,
    current_user: CurrentUser,
    state: AppState,
) -> Html<String> {
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let vault = state.vault.read().await;
    let parsed = crate::parser::markdown::parse_markdown(&content, &vault);
    drop(vault);

    let page_title = if parsed.title.is_empty() { fallback_title } else { parsed.title.clone() };

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("page_title", &page_title);
    ctx.insert("description", &parsed.description);
    ctx.insert("content", &parsed.html);
    ctx.insert("toc", &parsed.toc);
    ctx.insert("tags", &parsed.tags);
    ctx.insert("backlinks", &backlinks);
    ctx.insert("file_tree", &file_tree);
    ctx.insert("slug", &slug);
    ctx.insert("current_user", &current_user);

    Html(state.tera.render("note.html", &ctx).unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")))
}

async fn render_canvas(
    path: std::path::PathBuf,
    slug: String,
    note_title: String,
    backlinks: Vec<serde_json::Value>,
    file_tree: serde_json::Value,
    current_user: CurrentUser,
    state: AppState,
) -> Html<String> {
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let canvas_json = match crate::parser::canvas::parse_canvas(&content) {
        Ok(c) => serde_json::to_string(&c).unwrap_or_else(|_| "{}".to_string()),
        Err(e) => { tracing::warn!("Canvas parse error for {}: {}", slug, e); "{}".to_string() }
    };

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("page_title", &note_title);
    ctx.insert("canvas_json", &canvas_json);
    ctx.insert("backlinks", &backlinks);
    ctx.insert("file_tree", &file_tree);
    ctx.insert("slug", &slug);
    ctx.insert("current_user", &current_user);

    Html(state.tera.render("canvas.html", &ctx).unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")))
}

async fn render_excalidraw(
    path: std::path::PathBuf,
    slug: String,
    note_title: String,
    backlinks: Vec<serde_json::Value>,
    file_tree: serde_json::Value,
    current_user: CurrentUser,
    state: AppState,
) -> Html<String> {
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let vault = state.vault.read().await;
    let excalidraw_json = {
        let mut data = crate::parser::excalidraw::parse_excalidraw(&content).unwrap_or_default();
        crate::parser::excalidraw::resolve_embedded_files(&mut data, &vault.root, &vault.asset_paths);
        serde_json::to_string(&data).unwrap_or_else(|_| r#"{"elements":[]}"#.to_string())
    };
    drop(vault);

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("page_title", &note_title);
    ctx.insert("excalidraw_json", &excalidraw_json);
    ctx.insert("backlinks", &backlinks);
    ctx.insert("file_tree", &file_tree);
    ctx.insert("slug", &slug);
    ctx.insert("current_user", &current_user);

    Html(state.tera.render("excalidraw.html", &ctx).unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")))
}

async fn render_base(
    path: std::path::PathBuf,
    slug: String,
    note_title: String,
    backlinks: Vec<serde_json::Value>,
    file_tree: serde_json::Value,
    current_user: CurrentUser,
    state: AppState,
) -> Html<String> {
    let yaml = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let vault = state.vault.read().await;
    let bases_html = crate::parser::bases::render_base(&yaml, &vault);
    drop(vault);

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("page_title", &note_title);
    ctx.insert("bases_html", &bases_html);
    ctx.insert("backlinks", &backlinks);
    ctx.insert("file_tree", &file_tree);
    ctx.insert("slug", &slug);
    ctx.insert("current_user", &current_user);

    Html(state.tera.render("bases.html", &ctx).unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")))
}

/// Filtered file tree that respects user folder/list permissions.
pub fn build_file_tree_json_filtered(
    vault: &crate::vault::Vault,
    user: &CurrentUser,
    state: &AppState,
) -> serde_json::Value {
    let wl = state.whitelist.read().unwrap();
    let bl = state.blacklist.read().unwrap();

    fn insert(node: &mut serde_json::Value, parts: &[&str], slug: &str, title: &str, kind: &str) {
        if parts.is_empty() { return; }
        let arr = node.as_array_mut().unwrap();
        if parts.len() == 1 {
            arr.push(serde_json::json!({ "name": title, "slug": slug, "type": "file", "kind": kind }));
            return;
        }
        let folder_name = parts[0];
        let pos = arr.iter().position(|n| n["type"] == "folder" && n["name"] == folder_name);
        let idx = match pos {
            Some(i) => i,
            None => {
                arr.push(serde_json::json!({ "name": folder_name, "type": "folder", "children": [] }));
                arr.len() - 1
            }
        };
        let mut children = arr[idx]["children"].take();
        insert(&mut children, &parts[1..], slug, title, kind);
        arr[idx]["children"] = children;
    }

    let mut root = serde_json::Value::Array(vec![]);
    let mut sorted: Vec<_> = vault.notes.values().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));

    for note in sorted {
        // Apply list filters
        if bl.contains(&note.slug) { continue; }
        if !wl.is_empty() && !wl.contains(&note.slug) { continue; }
        // Apply user folder filter
        if !user.can_see_note(&note.slug) { continue; }
        // Only show published notes to non-admins
        if !user.is_admin && !note.is_published { continue; }

        let parts: Vec<&str> = note.path.split('/').collect();
        let kind = match note.kind {
            crate::vault::NoteKind::Canvas => "canvas",
            crate::vault::NoteKind::Excalidraw => "excalidraw",
            crate::vault::NoteKind::Markdown => "markdown",
            crate::vault::NoteKind::Base => "base",
        };
        insert(&mut root, &parts, &note.slug, &note.title, kind);
    }
    root
}

fn not_found(state: &AppState) -> Response {
    let html = format!(
        r#"<!DOCTYPE html><html><head><title>Nicht gefunden — {title}</title>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="stylesheet" href="/static/css/style.css">
</head><body><div style="padding:4rem;text-align:center;font-family:var(--font-interface,sans-serif)">
<div style="font-size:4rem;margin-bottom:1rem">🔍</div>
<h1 style="margin:0 0 .5rem">404 – Nicht gefunden</h1>
<p style="color:var(--text-muted,#888);margin-bottom:2rem">Diese Notiz existiert nicht.</p>
<a href="/" style="padding:.5rem 1.2rem;background:var(--interactive-accent,#7c5cbf);color:#fff;border-radius:6px;text-decoration:none">← Zurück zur Startseite</a>
</div></body></html>"#,
        title = state.config.server.site_title
    );
    (StatusCode::NOT_FOUND, Html(html)).into_response()
}

fn access_denied(state: &AppState) -> Response {
    let html = format!(
        r#"<!DOCTYPE html><html><head><title>Kein Zugriff — {title}</title>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="stylesheet" href="/static/css/style.css">
</head><body><div style="padding:4rem;text-align:center;font-family:var(--font-interface,sans-serif)">
<div style="font-size:4rem;margin-bottom:1rem">🔒</div>
<h1 style="margin:0 0 .5rem">Kein Zugriff</h1>
<p style="color:var(--text-muted,#888);margin-bottom:2rem">Du hast keine Berechtigung, diese Seite zu sehen.</p>
<a href="/" style="padding:.5rem 1.2rem;background:var(--interactive-accent,#7c5cbf);color:#fff;border-radius:6px;text-decoration:none">← Zurück zur Startseite</a>
</div></body></html>"#,
        title = state.config.server.site_title
    );
    (StatusCode::FORBIDDEN, Html(html)).into_response()
}

