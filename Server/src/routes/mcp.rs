use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{auth::CurrentUser, routes::api::note_visible, AppState};

// ── MCP Setup Page ────────────────────────────────────────────────────────────

pub async fn mcp_setup_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.can_use_mcp && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Kein Zugriff auf MCP-Setup").into_response();
    }

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("current_user", &current_user);
    ctx.insert("base_url", &state.config.server.base_url);

    Html(
        state.tera.render("mcp_setup.html", &ctx)
            .unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")),
    ).into_response()
}

// ── Token generation (long-lived API token for MCP) ───────────────────────────

#[derive(Deserialize)]
pub struct GenerateTokenRequest {
    pub name: String,
}

pub async fn generate_mcp_token(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(body): Json<GenerateTokenRequest>,
) -> Response {
    if !current_user.can_use_mcp && !current_user.is_admin {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Kein MCP-Zugriff"})),
        ).into_response();
    }

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Token-Name darf nicht leer sein"})),
        ).into_response();
    }

    let perms = match crate::db::get_user_permissions(&state.db, current_user.id).await {
        Ok(p) => p,
        Err(_) => return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Datenbankfehler"})),
        ).into_response(),
    };
    let folders = crate::db::get_allowed_folders(&state.db, current_user.id).await.unwrap_or_default();
    let groups = crate::db::get_user_group_names(&state.db, current_user.id).await.unwrap_or_default();

    let token = match crate::auth::create_jwt(
        current_user.id,
        &current_user.username,
        current_user.is_admin,
        perms.can_upload,
        perms.can_manage_users,
        perms.can_manage_lists,
        perms.folder_restriction,
        folders,
        groups,
        perms.sees_published,
        perms.sees_all,
        perms.can_propose,
        perms.can_use_mcp,
        8760, // 1 year
        &state.config.auth.session_secret,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("generate_mcp_token: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Token-Fehler"})),
            ).into_response();
        }
    };

    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    if let Err(e) = crate::db::create_mcp_token(&state.db, current_user.id, &name, &hash).await {
        tracing::error!("create_mcp_token DB: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Datenbankfehler beim Speichern des Tokens"})),
        ).into_response();
    }

    let expires_at = chrono::Utc::now() + chrono::Duration::hours(8760);
    Json(serde_json::json!({
        "token": token,
        "expires_at": expires_at.to_rfc3339(),
    })).into_response()
}

pub async fn list_mcp_tokens(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.can_use_mcp && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Kein MCP-Zugriff"}))).into_response();
    }
    match crate::db::list_mcp_tokens(&state.db, current_user.id).await {
        Ok(tokens) => Json(serde_json::json!({"tokens": tokens})).into_response(),
        Err(e) => {
            tracing::error!("list_mcp_tokens: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Datenbankfehler"}))).into_response()
        }
    }
}

pub async fn delete_mcp_token_handler(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<u32>,
) -> Response {
    if !current_user.can_use_mcp && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Kein MCP-Zugriff"}))).into_response();
    }
    match crate::db::delete_mcp_token(&state.db, id, current_user.id).await {
        Ok(true) => Json(serde_json::json!({"ok": true})).into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Token nicht gefunden"}))).into_response(),
        Err(e) => {
            tracing::error!("delete_mcp_token: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Datenbankfehler"}))).into_response()
        }
    }
}

// ── MCP JSON-RPC 2.0 Endpoint ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }
    fn err(id: Option<Value>, code: i32, msg: impl Into<String>) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: None, error: Some(JsonRpcError { code, message: msg.into() }) }
    }
}

pub async fn mcp_handler(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    if !current_user.can_use_mcp && !current_user.is_admin {
        return Json(JsonRpcResponse::err(req.id, -32001, "Kein MCP-Zugriff")).into_response();
    }

    let resp = match req.method.as_str() {
        "initialize" => handle_initialize(&req.id),
        "tools/list" => handle_tools_list(&req.id),
        "tools/call" => handle_tool_call(&req.id, req.params.as_ref(), &state, &current_user).await,
        "ping" => JsonRpcResponse::ok(req.id, serde_json::json!({})),
        _ => JsonRpcResponse::err(req.id, -32601, format!("Method not found: {}", req.method)),
    };

    Json(resp).into_response()
}

fn handle_initialize(id: &Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::ok(id.clone(), serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "obsidian-publisher", "version": "1.0.0" }
    }))
}

fn handle_tools_list(id: &Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::ok(id.clone(), serde_json::json!({
        "tools": [
            {
                "name": "search_notes",
                "description": "Sucht Notizen im Vault. Gibt Treffer mit Titel, Slug und Textauszug zurück.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Suchbegriff" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "read_note",
                "description": "Liest den vollständigen Markdown-Inhalt einer Notiz.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "slug": { "type": "string", "description": "Notiz-Slug (z.B. '01 MOC/Index')" }
                    },
                    "required": ["slug"]
                }
            },
            {
                "name": "list_notes",
                "description": "Listet alle sichtbaren Notizen mit Titel, Slug und Tags.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Max. Anzahl (Standard: 100)" }
                    }
                }
            },
            {
                "name": "search_by_metadata",
                "description": "Sucht Notizen anhand von Frontmatter-Metadaten und/oder Tags. Nützlich um z.B. alle Notizen eines bestimmten Typs, Autors oder Status zu finden.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "description": "Frontmatter-Schlüssel, nach dem gesucht wird (z.B. 'status', 'author', 'type')"
                        },
                        "value": {
                            "type": "string",
                            "description": "Optionaler Wert – fehlt er, wird nur geprüft ob der Schlüssel vorhanden ist. Teilstring-Suche (case-insensitiv)."
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optionale Tag-Filter – mind. einer muss vorhanden sein."
                        },
                        "limit": { "type": "integer", "description": "Max. Anzahl (Standard: 50)" }
                    }
                }
            }
        ]
    }))
}

async fn handle_tool_call(
    id: &Option<Value>,
    params: Option<&Value>,
    state: &AppState,
    user: &CurrentUser,
) -> JsonRpcResponse {
    let Some(params) = params else {
        return JsonRpcResponse::err(id.clone(), -32602, "Missing params");
    };

    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return JsonRpcResponse::err(id.clone(), -32602, "Missing tool name"),
    };

    let args = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

    match name {
        "search_notes" => {
            let query = match args.get("query").and_then(|v| v.as_str()) {
                Some(q) => q.to_lowercase(),
                None => return JsonRpcResponse::err(id.clone(), -32602, "Missing query"),
            };

            let vault = state.vault.read().await;
            let mut results: Vec<Value> = vec![];

            for note in vault.notes.values() {
                if !note_visible(note, user, state) { continue; }
                let in_title = note.title.to_lowercase().contains(&query);
                let in_body = vault.search_index.get(&note.slug)
                    .map(|t| t.to_lowercase().contains(&query))
                    .unwrap_or(false);
                if in_title || in_body {
                    let excerpt = vault.search_index.get(&note.slug)
                        .map(|t| {
                            if let Some(pos) = t.to_lowercase().find(&query) {
                                let start = pos.saturating_sub(80);
                                let end = (pos + query.len() + 80).min(t.len());
                                format!("…{}…", &t[start..end])
                            } else {
                                t.chars().take(160).collect::<String>()
                            }
                        })
                        .unwrap_or_default();
                    results.push(serde_json::json!({
                        "slug": note.slug,
                        "title": note.title,
                        "tags": note.tags,
                        "excerpt": excerpt,
                    }));
                    if results.len() >= 20 { break; }
                }
            }

            JsonRpcResponse::ok(id.clone(), serde_json::json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&results).unwrap_or_default() }]
            }))
        }

        "read_note" => {
            let slug = match args.get("slug").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return JsonRpcResponse::err(id.clone(), -32602, "Missing slug"),
            };

            let vault = state.vault.read().await;
            let note = match vault.notes.get(slug) {
                Some(n) => n.clone(),
                None => return JsonRpcResponse::ok(id.clone(), serde_json::json!({
                    "content": [{ "type": "text", "text": format!("Notiz '{}' nicht gefunden.", slug) }],
                    "isError": true
                })),
            };
            drop(vault);

            if !note_visible(&note, user, state) {
                return JsonRpcResponse::ok(id.clone(), serde_json::json!({
                    "content": [{ "type": "text", "text": "Zugriff verweigert." }],
                    "isError": true
                }));
            }

            let path = std::path::Path::new(&state.config.server.vault_path).join(&note.path);
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| "Inhalt nicht lesbar.".to_string());

            JsonRpcResponse::ok(id.clone(), serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": format!("# {}\nSlug: {}\nTags: {}\n\n---\n\n{}",
                        note.title, note.slug, note.tags.join(", "), content)
                }]
            }))
        }

        "list_notes" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
            let vault = state.vault.read().await;

            let notes: Vec<Value> = vault.notes.values()
                .filter(|n| note_visible(n, user, state))
                .take(limit)
                .map(|n| serde_json::json!({
                    "slug": n.slug,
                    "title": n.title,
                    "tags": n.tags,
                    "is_published": n.is_published,
                }))
                .collect();

            JsonRpcResponse::ok(id.clone(), serde_json::json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&notes).unwrap_or_default() }]
            }))
        }

        "search_by_metadata" => {
            let key = args.get("key").and_then(|v| v.as_str()).map(str::to_lowercase);
            let value = args.get("value").and_then(|v| v.as_str()).map(str::to_lowercase);
            let tags_filter: Vec<String> = args.get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|t| t.as_str()).map(|t| t.to_lowercase()).collect())
                .unwrap_or_default();
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

            let vault = state.vault.read().await;
            let mut results: Vec<Value> = vec![];

            'outer: for note in vault.notes.values() {
                if !note_visible(note, user, state) { continue; }

                // Tags filter (any match)
                if !tags_filter.is_empty() {
                    let note_tags_lc: Vec<String> = note.tags.iter().map(|t| t.to_lowercase()).collect();
                    if !tags_filter.iter().any(|ft| note_tags_lc.contains(ft)) { continue; }
                }

                // Metadata filter
                if let Some(k) = &key {
                    match note.frontmatter.iter().find(|(fk, _)| fk.to_lowercase() == *k) {
                        None => continue 'outer,
                        Some((_, fv)) => {
                            if let Some(expected) = &value {
                                if !fv.to_lowercase().contains(expected.as_str()) {
                                    continue 'outer;
                                }
                            }
                        }
                    }
                }

                let frontmatter: Vec<Value> = {
                    let mut fm: Vec<(String, String)> = note.frontmatter.iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    fm.sort_by(|a, b| a.0.cmp(&b.0));
                    fm.into_iter().map(|(k, v)| serde_json::json!({"key": k, "value": v})).collect()
                };

                results.push(serde_json::json!({
                    "slug": note.slug,
                    "title": note.title,
                    "tags": note.tags,
                    "frontmatter": frontmatter,
                }));

                if results.len() >= limit { break; }
            }

            results.sort_by(|a, b| {
                a.get("slug").and_then(|v| v.as_str())
                    .cmp(&b.get("slug").and_then(|v| v.as_str()))
            });

            JsonRpcResponse::ok(id.clone(), serde_json::json!({
                "content": [{ "type": "text", "text": serde_json::to_string_pretty(&results).unwrap_or_default() }]
            }))
        }

        _ => JsonRpcResponse::err(id.clone(), -32602, format!("Unknown tool: {name}")),
    }
}
