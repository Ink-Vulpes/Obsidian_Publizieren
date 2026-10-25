//! obsidian-uploader — local web UI for syncing the Obsidian vault to the server.
//!
//! Usage:
//!   obsidian-uploader [--vault <path>] [--port <port>]
//!
//! Opens http://localhost:<port> in the browser.

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct UploaderState {
    server_url: String,
    auth_token: String,
    vault_path: String,
}

type SharedState = Arc<RwLock<UploaderState>>;

// ── Local file scanning ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
struct LocalFile {
    path: String,
    size: u64,
    checksum: String,
    modified: i64,
}

fn scan_vault(vault_path: &str) -> Vec<LocalFile> {
    let root = PathBuf::from(vault_path);
    if !root.exists() {
        return vec![];
    }

    let mut files = vec![];
    for entry in walkdir::WalkDir::new(&root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        // Skip hidden files
        if path
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            continue;
        }

        let rel = match path.strip_prefix(&root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        let Ok(meta) = std::fs::metadata(path) else { continue };
        let Ok(bytes) = std::fs::read(path) else { continue };

        let checksum = hex::encode(Sha256::digest(&bytes));
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        files.push(LocalFile {
            path: rel,
            size: meta.len(),
            checksum,
            modified,
        });
    }
    files
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

async fn index() -> Html<&'static str> {
    Html(include_str!("../../templates/uploader.html"))
}

#[derive(Deserialize)]
struct ConnectBody {
    server_url: String,
    username: String,
    password: String,
    vault_path: String,
}

async fn api_connect(
    State(state): State<SharedState>,
    Json(body): Json<ConnectBody>,
) -> axum::response::Response {
    let url = body.server_url.trim_end_matches('/').to_string();

    let client = reqwest::Client::new();
    let resp = match client
        .post(format!("{}/api/auth/token", url))
        .json(&serde_json::json!({
            "username": body.username,
            "password": body.password
        }))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("Verbindungsfehler: {}", e) })),
            )
                .into_response()
        }
    };

    let status = resp.status();
    let data: serde_json::Value = resp.json().await.unwrap_or_default();

    if !status.is_success() {
        let err = data["error"].as_str().unwrap_or("Anmeldung fehlgeschlagen");
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": err })),
        )
            .into_response();
    }

    let token = data["token"].as_str().unwrap_or("").to_string();
    {
        let mut s = state.write().await;
        s.server_url = url;
        s.auth_token = token;
        s.vault_path = body.vault_path;
    }

    Json(serde_json::json!({
        "ok": true,
        "username": data["username"],
        "is_admin": data["is_admin"],
    }))
    .into_response()
}

#[derive(Serialize)]
struct DiffEntry {
    path: String,
    status: String,
    local_size: Option<u64>,
    remote_size: Option<u64>,
}

async fn api_diff(State(state): State<SharedState>) -> axum::response::Response {
    let (server_url, token, vault_path) = {
        let s = state.read().await;
        (s.server_url.clone(), s.auth_token.clone(), s.vault_path.clone())
    };

    if token.is_empty() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Nicht eingeloggt"}))).into_response();
    }

    // Fetch remote manifest
    let client = reqwest::Client::new();
    let remote_files: Vec<serde_json::Value> = match client
        .get(format!("{}/api/files/manifest", server_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
    {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error": e.to_string()}))).into_response()
        }
    };

    let remote_map: HashMap<String, (String, u64)> = remote_files
        .iter()
        .filter_map(|f| {
            let path = f["path"].as_str()?.to_string();
            let checksum = f["checksum"].as_str().unwrap_or("").to_string();
            let size = f["size"].as_u64().unwrap_or(0);
            Some((path, (checksum, size)))
        })
        .collect();

    let local_files = scan_vault(&vault_path);
    let local_map: HashMap<String, &LocalFile> = local_files.iter().map(|f| (f.path.clone(), f)).collect();

    let mut diff: Vec<DiffEntry> = vec![];

    // New or modified local files
    for lf in &local_files {
        match remote_map.get(&lf.path) {
            None => diff.push(DiffEntry {
                path: lf.path.clone(),
                status: "new".to_string(),
                local_size: Some(lf.size),
                remote_size: None,
            }),
            Some((remote_checksum, remote_size)) => {
                if lf.checksum != *remote_checksum {
                    diff.push(DiffEntry {
                        path: lf.path.clone(),
                        status: "modified".to_string(),
                        local_size: Some(lf.size),
                        remote_size: Some(*remote_size),
                    });
                }
            }
        }
    }

    // Deleted (on remote but not local)
    for (path, (_, size)) in &remote_map {
        if !local_map.contains_key(path.as_str()) {
            diff.push(DiffEntry {
                path: path.clone(),
                status: "deleted".to_string(),
                local_size: None,
                remote_size: Some(*size),
            });
        }
    }

    diff.sort_by(|a, b| a.path.cmp(&b.path));
    Json(diff).into_response()
}

#[derive(Deserialize)]
struct SyncBody {
    paths: Vec<String>,
    delete_paths: Vec<String>,
}

async fn api_sync(
    State(state): State<SharedState>,
    Json(body): Json<SyncBody>,
) -> axum::response::Response {
    let (server_url, token, vault_path) = {
        let s = state.read().await;
        (s.server_url.clone(), s.auth_token.clone(), s.vault_path.clone())
    };

    if token.is_empty() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"error": "Nicht eingeloggt"}))).into_response();
    }

    let client = reqwest::Client::new();
    let root = PathBuf::from(&vault_path);
    let mut uploaded = 0u32;
    let mut deleted = 0u32;
    let mut errors: Vec<String> = vec![];

    // Upload files in batches of 10
    for chunk in body.paths.chunks(10) {
        let mut form = multipart::Form::new();
        for rel_path in chunk {
            let full_path = root.join(rel_path);
            match std::fs::read(&full_path) {
                Ok(bytes) => {
                    let part = multipart::Part::bytes(bytes)
                        .file_name(rel_path.clone())
                        .headers({
                            let mut h = reqwest::header::HeaderMap::new();
                            h.insert("x-file-path", rel_path.parse().unwrap());
                            h
                        });
                    form = form.part("file", part);
                }
                Err(e) => errors.push(format!("{}: {}", rel_path, e)),
            }
        }

        match client
            .post(format!("{}/api/upload", server_url))
            .header("Authorization", format!("Bearer {}", token))
            .multipart(form)
            .send()
            .await
        {
            Ok(r) => {
                if let Ok(d) = r.json::<serde_json::Value>().await {
                    uploaded += d["uploaded"].as_u64().unwrap_or(0) as u32;
                    if let Some(errs) = d["errors"].as_array() {
                        for e in errs {
                            if let Some(s) = e.as_str() {
                                errors.push(s.to_string());
                            }
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("Upload-Fehler: {}", e)),
        }
    }

    // Delete remote files
    for rel_path in &body.delete_paths {
        match client
            .delete(format!("{}/api/files/delete?path={}", server_url, urlencoding::encode(rel_path)))
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => deleted += 1,
            Ok(r) => errors.push(format!("Delete {}: HTTP {}", rel_path, r.status())),
            Err(e) => errors.push(format!("Delete {}: {}", rel_path, e)),
        }
    }

    Json(serde_json::json!({
        "uploaded": uploaded,
        "deleted": deleted,
        "errors": errors,
    }))
    .into_response()
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut port: u16 = 7777;
    let mut vault_path = String::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(7777);
                i += 2;
            }
            "--vault" if i + 1 < args.len() => {
                vault_path = args[i + 1].clone();
                i += 2;
            }
            _ => i += 1,
        }
    }

    let state: SharedState = Arc::new(RwLock::new(UploaderState {
        vault_path,
        ..Default::default()
    }));

    let app = Router::new()
        .route("/", get(index))
        .route("/api/connect", post(api_connect))
        .route("/api/diff", get(api_diff))
        .route("/api/sync", post(api_sync))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    println!("Obsidian Uploader läuft auf http://{}", addr);
    println!("Öffne den Browser…");

    let url = format!("http://localhost:{}", port);
    if let Err(e) = open::that(&url) {
        println!("Browser konnte nicht automatisch geöffnet werden: {}", e);
        println!("Bitte öffne {} manuell.", url);
    }

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Port belegt");
    axum::serve(listener, app).await.expect("Server-Fehler");
}
