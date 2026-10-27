use axum::{
    body::Bytes,
    extract::{Multipart, Query, State},
    extract::Json as ExtractJson,
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::{auth::CurrentUser, AppState};

// ── Note visibility helper ────────────────────────────────────────────────────

fn note_visible(note: &crate::vault::NoteEntry, user: &CurrentUser, state: &AppState) -> bool {
    let wl = state.whitelist.read().unwrap();
    let bl = state.blacklist.read().unwrap();

    if bl.contains(&note.slug) {
        return false;
    }
    if !wl.is_empty() && !wl.contains(&note.slug) {
        return false;
    }
    if !user.is_admin && !note.is_published {
        return false;
    }
    user.can_see_note(&note.slug)
}

// ── Search ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub tags: Vec<String>,
}

pub async fn search(
    Query(params): Query<SearchQuery>,
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Json<Vec<SearchResult>> {
    let q = params.q.unwrap_or_default();
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Json(vec![]);
    }

    let vault = state.vault.read().await;
    let mut seen = std::collections::HashSet::new();
    let mut results: Vec<SearchResult> = vec![];

    for note in vault.notes.values() {
        if !note_visible(note, &current_user, &state) {
            continue;
        }
        if note.title.to_lowercase().contains(&q) {
            seen.insert(note.slug.clone());
            results.push(SearchResult {
                title: note.title.clone(),
                slug: note.slug.clone(),
                excerpt: note.tags.join(", "),
                tags: note.tags.clone(),
            });
        }
    }

    for (slug, text) in &vault.search_index {
        if seen.contains(slug) {
            continue;
        }
        let Some(note_entry) = vault.notes.get(slug) else { continue };
        if !note_visible(note_entry, &current_user, &state) {
            continue;
        }
        let lower = text.to_lowercase();
        if lower.contains(&q) {
            let pos = lower.find(&q).unwrap_or(0);
            let start = pos.saturating_sub(60);
            let end = (pos + q.len() + 60).min(text.len());
            let start = text.char_indices().map(|(i, _)| i).filter(|&i| i <= start).last().unwrap_or(0);
            let end = text.char_indices().map(|(i, _)| i).find(|&i| i >= end).unwrap_or(text.len());
            let excerpt = format!("…{}…", text[start..end].trim());
            seen.insert(slug.clone());
            results.push(SearchResult {
                title: note_entry.title.clone(),
                slug: slug.clone(),
                excerpt,
                tags: note_entry.tags.clone(),
            });
        }
    }

    results.truncate(20);
    Json(results)
}

// ── Graph ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub link_count: usize,
}

#[derive(Serialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
}

pub async fn graph_data(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Json<GraphData> {
    let vault = state.vault.read().await;

    let nodes: Vec<GraphNode> = vault
        .notes
        .values()
        .filter(|n| note_visible(n, &current_user, &state))
        .map(|n| {
            let backlink_count = vault.backlinks.get(&n.slug).map(|v| v.len()).unwrap_or(0);
            GraphNode {
                id: n.slug.clone(),
                title: n.title.clone(),
                tags: n.tags.clone(),
                link_count: backlink_count + n.links_to.len(),
            }
        })
        .collect();

    let node_slugs: std::collections::HashSet<_> = nodes.iter().map(|n| &n.id).collect();
    let mut links = vec![];
    for note in vault.notes.values() {
        if !node_slugs.contains(&note.slug) {
            continue;
        }
        for link_title in &note.links_to {
            if let Some(target) = vault.title_to_slug.get(&link_title.to_lowercase()) {
                if node_slugs.contains(target) {
                    links.push(GraphLink { source: note.slug.clone(), target: target.clone() });
                }
            }
        }
    }

    Json(GraphData { nodes, links })
}

// ── File list ─────────────────────────────────────────────────────────────────

pub async fn file_list(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Json<serde_json::Value> {
    let vault = state.vault.read().await;
    let tree = crate::routes::note::build_file_tree_json_filtered(&vault, &current_user, &state);
    drop(vault);
    Json(tree)
}

// ── Vault reload ──────────────────────────────────────────────────────────────

pub async fn reload_vault(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pf = state.publish_field.read().unwrap().clone();
    match crate::vault::Vault::load(&state.config.server.vault_path, &pf) {
        Ok(new_vault) => {
            let count = new_vault.notes.len();
            let mut vault = state.vault.write().await;
            *vault = new_vault;
            drop(vault);
            // Reload lists from DB
            if let Ok(wl) = crate::db::get_whitelist(&state.db).await {
                *state.whitelist.write().unwrap() = wl.into_iter().collect();
            }
            if let Ok(bl) = crate::db::get_blacklist(&state.db).await {
                *state.blacklist.write().unwrap() = bl.into_iter().collect();
            }
            Json(serde_json::json!({ "status": "ok", "notes": count }))
        }
        Err(e) => Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
    }
}

// ── Files manifest (for upload script) ───────────────────────────────────────

#[derive(Serialize)]
pub struct FileManifestEntry {
    pub path: String,
    pub size: u64,
    pub checksum: String,
    pub modified: i64,
}

pub async fn file_manifest(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Json<Vec<FileManifestEntry>> {
    if !current_user.can_upload && !current_user.is_admin {
        return Json(vec![]);
    }

    let vault_root = PathBuf::from(&state.config.server.vault_path);
    let mut entries = vec![];

    for ent in walkdir::WalkDir::new(&vault_root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = ent.path();
        let rel = match path.strip_prefix(&vault_root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let Ok(bytes) = std::fs::read(path) else { continue };
        let checksum = hex::encode(Sha256::digest(&bytes));
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        entries.push(FileManifestEntry {
            path: rel,
            size: meta.len(),
            checksum,
            modified,
        });
    }

    Json(entries)
}

// ── File upload ───────────────────────────────────────────────────────────────

pub async fn upload_files(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    mut multipart: Multipart,
) -> axum::response::Response {
    if !current_user.can_upload && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Keine Upload-Berechtigung"}))).into_response();
    }

    let vault_root = PathBuf::from(&state.config.server.vault_path);
    let mut uploaded = 0u32;
    let mut errors: Vec<String> = vec![];

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name != "file" {
            continue;
        }
        let filename = field.file_name().unwrap_or("unknown").to_string();
        let rel_path = field
            .headers()
            .get("x-file-path")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| filename.clone());

        // Security: prevent path traversal without requiring parent dirs to exist.
        // Only allow Normal path components (no "..", no absolute paths, no Windows prefixes).
        let rel_clean = rel_path.replace('\\', "/");
        let rel_pb = std::path::PathBuf::from(&rel_clean);
        let is_safe = !rel_clean.is_empty()
            && rel_pb
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !is_safe {
            errors.push(format!("{}: path traversal rejected", rel_path));
            continue;
        }
        let target = vault_root.join(&rel_pb);

        let data: Bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{}: read error: {}", rel_path, e));
                continue;
            }
        };

        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                errors.push(format!("{}: mkdir error: {}", rel_path, e));
                continue;
            }
            // Ensure parent directories are traversable (fixes vsftpd-created dirs with 700)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755));
            }
        }

        // If a previous upload (e.g. via vsftpd) left a file with restrictive permissions,
        // remove it first so we can replace it.
        if target.exists() {
            let _ = std::fs::remove_file(&target);
        }

        if let Err(e) = std::fs::write(&target, &data) {
            errors.push(format!("{}: write error: {}", rel_path, e));
        } else {
            // Ensure file is readable by the web server (fixes umask / vsftpd chmod 700)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644));
            }
            let checksum = hex::encode(sha2::Sha256::digest(&data));
            let _ = crate::db::upsert_vault_file(
                &state.db, &rel_clean, &checksum, data.len() as u64,
                current_user.id, None,
            ).await;
            uploaded += 1;
        }
    }

    axum::Json(serde_json::json!({
        "uploaded": uploaded,
        "errors": errors,
    }))
    .into_response()
}

// ── File deletion ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DeleteQuery {
    pub path: String,
}

pub async fn delete_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(params): Query<DeleteQuery>,
) -> axum::response::Response {
    if !current_user.can_upload && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Keine Berechtigung").into_response();
    }

    let vault_root = PathBuf::from(&state.config.server.vault_path);
    let target = vault_root.join(&params.path);

    // Security: prevent path traversal
    let canonical_vault = vault_root.canonicalize().unwrap_or(vault_root.clone());
    let Ok(canonical_target) = target.canonicalize() else {
        return (StatusCode::NOT_FOUND, "Datei nicht gefunden").into_response();
    };
    if !canonical_target.starts_with(&canonical_vault) {
        return (StatusCode::FORBIDDEN, "Pfad außerhalb des Vaults").into_response();
    }

    match std::fs::remove_file(&target) {
        Ok(_) => {
            let _ = crate::db::delete_vault_file(&state.db, &params.path).await;
            Json(serde_json::json!({"ok": true})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

// ── File download (for bidirectional sync) ────────────────────────────────────

pub async fn download_file(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Query(params): Query<DeleteQuery>,
) -> axum::response::Response {
    if !current_user.can_upload && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Keine Berechtigung").into_response();
    }

    let vault_root = PathBuf::from(&state.config.server.vault_path);
    let rel_clean = params.path.replace('\\', "/");
    let rel_pb = std::path::PathBuf::from(&rel_clean);
    let is_safe = !rel_clean.is_empty()
        && rel_pb.components().all(|c| matches!(c, std::path::Component::Normal(_)));
    if !is_safe {
        return (StatusCode::BAD_REQUEST, "Ungültiger Pfad").into_response();
    }

    let target = vault_root.join(&rel_pb);
    match std::fs::read(&target) {
        Ok(data) => (
            [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
            data,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Datei nicht gefunden").into_response(),
    }
}

// ── Device registration ───────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct RegisterDeviceBody {
    pub device_id: String,
    pub device_name: String,
}

pub async fn register_device(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    ExtractJson(body): ExtractJson<RegisterDeviceBody>,
) -> axum::response::Response {
    match crate::db::upsert_sync_device(&state.db, &body.device_id, &body.device_name, current_user.id).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

use axum::response::IntoResponse;
