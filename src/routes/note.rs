use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use serde_json::json;

use crate::{vault::NoteKind, AppState};

pub async fn index(State(state): State<AppState>) -> Response {
    let vault = state.vault.read().await;

    let first = vault
        .find_by_title("index")
        .or_else(|| vault.find_by_title("home"))
        .or_else(|| vault.find_by_title("readme"))
        .or_else(|| vault.notes.values().next())
        .map(|n| n.slug.clone());

    drop(vault);

    if let Some(slug) = first {
        return Redirect::to(&format!("/note/{}", slug)).into_response();
    }

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.site_title);
    ctx.insert("site_description", &state.config.site_description);

    let html = state
        .tera
        .render("index.html", &ctx)
        .unwrap_or_else(|_| "<h1>Welcome</h1><p>Upload notes via FTP.</p>".to_string());
    Html(html).into_response()
}

pub async fn render_note(
    Path(path): Path<String>,
    State(state): State<AppState>,
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

    let kind = entry.kind.clone();
    let note_path = vault.root.join(&entry.path);
    let slug = entry.slug.clone();
    let note_title = entry.title.clone();

    let backlinks: Vec<serde_json::Value> = vault
        .get_backlinks(&slug)
        .iter()
        .map(|n| json!({ "title": n.title, "slug": n.slug }))
        .collect();

    let file_tree = build_file_tree_json(&vault);
    drop(vault);

    match kind {
        NoteKind::Markdown => {
            render_markdown(note_path, slug, note_title, backlinks, file_tree, state)
                .await
                .into_response()
        }
        NoteKind::Canvas => {
            render_canvas(note_path, slug, note_title, backlinks, file_tree, state)
                .await
                .into_response()
        }
        NoteKind::Excalidraw => {
            render_excalidraw(note_path, slug, note_title, backlinks, file_tree, state)
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
    state: AppState,
) -> Html<String> {
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let vault = state.vault.read().await;
    let parsed = crate::parser::markdown::parse_markdown(&content, &vault);
    drop(vault);

    let page_title = if parsed.title.is_empty() {
        fallback_title
    } else {
        parsed.title.clone()
    };

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.site_title);
    ctx.insert("page_title", &page_title);
    ctx.insert("description", &parsed.description);
    ctx.insert("content", &parsed.html);
    ctx.insert("toc", &parsed.toc);
    ctx.insert("tags", &parsed.tags);
    ctx.insert("backlinks", &backlinks);
    ctx.insert("file_tree", &file_tree);
    ctx.insert("slug", &slug);

    let html = state
        .tera
        .render("note.html", &ctx)
        .unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>"));
    Html(html)
}

async fn render_canvas(
    path: std::path::PathBuf,
    slug: String,
    note_title: String,
    backlinks: Vec<serde_json::Value>,
    file_tree: serde_json::Value,
    state: AppState,
) -> Html<String> {
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let canvas_json = match crate::parser::canvas::parse_canvas(&content) {
        Ok(c) => serde_json::to_string(&c).unwrap_or_else(|_| "{}".to_string()),
        Err(e) => {
            tracing::warn!("Canvas parse error for {}: {}", slug, e);
            "{}".to_string()
        }
    };

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.site_title);
    ctx.insert("page_title", &note_title);
    ctx.insert("canvas_json", &canvas_json);
    ctx.insert("backlinks", &backlinks);
    ctx.insert("file_tree", &file_tree);
    ctx.insert("slug", &slug);

    Html(
        state
            .tera
            .render("canvas.html", &ctx)
            .unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")),
    )
}

async fn render_excalidraw(
    path: std::path::PathBuf,
    slug: String,
    note_title: String,
    backlinks: Vec<serde_json::Value>,
    file_tree: serde_json::Value,
    state: AppState,
) -> Html<String> {
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let excalidraw_json = crate::parser::excalidraw::parse_excalidraw(&content)
        .and_then(|d| serde_json::to_string(&d).ok())
        .unwrap_or_else(|| r#"{"elements":[]}"#.to_string());

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.site_title);
    ctx.insert("page_title", &note_title);
    ctx.insert("excalidraw_json", &excalidraw_json);
    ctx.insert("backlinks", &backlinks);
    ctx.insert("file_tree", &file_tree);
    ctx.insert("slug", &slug);

    Html(
        state
            .tera
            .render("excalidraw.html", &ctx)
            .unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")),
    )
}

fn not_found(state: &AppState) -> Response {
    let html = format!(
        r#"<!DOCTYPE html><html><head><title>Not Found — {}</title>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="stylesheet" href="/static/css/style.css">
</head><body><div style="padding:4rem;text-align:center">
<h1>404</h1><p>Note not found.</p><a href="/">← Home</a></div></body></html>"#,
        state.config.site_title
    );
    (StatusCode::NOT_FOUND, Html(html)).into_response()
}

pub fn build_file_tree_json(vault: &crate::vault::Vault) -> serde_json::Value {
    fn insert(node: &mut serde_json::Value, parts: &[&str], slug: &str, title: &str, kind: &str) {
        if parts.is_empty() {
            return;
        }
        let arr = node.as_array_mut().unwrap();
        if parts.len() == 1 {
            arr.push(json!({ "name": title, "slug": slug, "type": "file", "kind": kind }));
            return;
        }
        let folder_name = parts[0];
        let pos = arr
            .iter()
            .position(|n| n["type"] == "folder" && n["name"] == folder_name);
        let idx = match pos {
            Some(i) => i,
            None => {
                arr.push(json!({ "name": folder_name, "type": "folder", "children": [] }));
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
        let parts: Vec<&str> = note.path.split('/').collect();
        let kind = match note.kind {
            NoteKind::Canvas => "canvas",
            NoteKind::Excalidraw => "excalidraw",
            NoteKind::Markdown => "markdown",
        };
        insert(&mut root, &parts, &note.slug, &note.title, kind);
    }
    root
}
