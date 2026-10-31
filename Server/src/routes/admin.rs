use axum::{
    extract::{Extension, Form, Json as ExtractJson, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect, Response},
};
use serde::Deserialize;

use crate::{auth::CurrentUser, AppState};

// ── Admin panel page ──────────────────────────────────────────────────────────

pub async fn admin_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.can_access_admin {
        return (StatusCode::FORBIDDEN, "Zugriff verweigert").into_response();
    }

    let users = crate::db::list_users_with_permissions(&state.db)
        .await
        .unwrap_or_default();
    let whitelist = crate::db::get_whitelist(&state.db).await.unwrap_or_default();
    let blacklist = crate::db::get_blacklist(&state.db).await.unwrap_or_default();
    let groups = crate::db::list_groups_with_members(&state.db).await.unwrap_or_default();

    let vault = state.vault.read().await;
    let all_slugs: Vec<String> = {
        let mut slugs: Vec<_> = vault.notes.keys().cloned().collect();
        slugs.sort();
        slugs
    };
    let all_folders: Vec<String> = {
        let mut folders: std::collections::HashSet<String> = std::collections::HashSet::new();
        for slug in vault.notes.keys() {
            let parts: Vec<_> = slug.split('/').collect();
            for i in 1..parts.len() {
                folders.insert(parts[..i].join("/"));
            }
        }
        let mut v: Vec<_> = folders.into_iter().collect();
        v.sort();
        v
    };
    drop(vault);

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("page_title", "Administration");
    ctx.insert("current_user", &current_user);
    ctx.insert("users", &users);
    ctx.insert("whitelist", &whitelist);
    ctx.insert("blacklist", &blacklist);
    ctx.insert("groups", &groups);
    ctx.insert("all_slugs", &all_slugs);
    ctx.insert("all_folders", &all_folders);

    Html(
        state
            .tera
            .render("admin.html", &ctx)
            .unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")),
    )
    .into_response()
}

// ── Invite creation ───────────────────────────────────────────────────────────

pub async fn create_invite(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.can_manage_users && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Keine Berechtigung"}))).into_response();
    }

    let token = crate::auth::generate_token();
    let expires = chrono::Utc::now() + chrono::Duration::hours(48);

    if let Err(e) = crate::db::create_invite(&state.db, &token, current_user.id, expires).await {
        tracing::error!("create_invite: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Serverfehler"})),
        )
            .into_response();
    }

    let url = format!("{}/register/{}", state.config.server.base_url, token);
    Json(serde_json::json!({ "url": url, "token": token, "expires_hours": 48 })).into_response()
}

// ── Permission updates ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdatePermForm {
    pub can_upload: Option<String>,
    pub can_manage_users: Option<String>,
    pub can_manage_lists: Option<String>,
    pub folder_restriction: Option<String>,
    pub folders: Option<String>,
}

pub async fn update_permissions(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(user_id): Path<u32>,
    Form(form): Form<UpdatePermForm>,
) -> Response {
    if !current_user.can_manage_users && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Keine Berechtigung").into_response();
    }
    if let Ok(Some(target)) = crate::db::find_user_by_id(&state.db, user_id).await {
        if target.is_admin {
            return (StatusCode::FORBIDDEN, "Admin-Berechtigungen können nicht verändert werden").into_response();
        }
    }

    let existing = crate::db::get_user_permissions(&state.db, user_id).await.unwrap_or_default();
    let perms = crate::db::UserPermissions {
        user_id,
        can_upload: form.can_upload.as_deref() == Some("on"),
        can_manage_users: form.can_manage_users.as_deref() == Some("on"),
        can_manage_lists: form.can_manage_lists.as_deref() == Some("on"),
        folder_restriction: form.folder_restriction.as_deref() == Some("on"),
        sees_published: existing.sees_published,
        sees_all: existing.sees_all,
    };

    if let Err(e) = crate::db::upsert_permissions(&state.db, &perms).await {
        tracing::error!("upsert_permissions: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Fehler beim Speichern").into_response();
    }

    if perms.folder_restriction {
        let folders: Vec<String> = form
            .folders
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if let Err(e) = crate::db::set_folder_access(&state.db, user_id, &folders).await {
            tracing::error!("set_folder_access: {}", e);
        }
    } else {
        // Remove all folder restrictions if not restricted mode
        let _ = crate::db::set_folder_access(&state.db, user_id, &[]).await;
    }

    Redirect::to("/admin").into_response()
}

// ── User deletion ─────────────────────────────────────────────────────────────

pub async fn delete_user(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(user_id): Path<u32>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Nur Admins"}))).into_response();
    }
    if user_id == current_user.id {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Kannst dich nicht selbst löschen"})),
        )
            .into_response();
    }
    if let Ok(Some(target)) = crate::db::find_user_by_id(&state.db, user_id).await {
        if target.is_admin {
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "Admin-Nutzer können nicht gelöscht werden"}))).into_response();
        }
    }

    if let Err(e) = crate::db::delete_user(&state.db, user_id).await {
        tracing::error!("delete_user: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Fehler"}))).into_response();
    }

    Json(serde_json::json!({"ok": true})).into_response()
}

// ── Whitelist / Blacklist ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListItemForm {
    pub slug: String,
}

pub async fn add_whitelist(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Form(form): Form<ListItemForm>,
) -> Response {
    if let Err(r) = require_lists(&current_user) { return r; }
    let _ = crate::db::add_to_whitelist(&state.db, &form.slug, current_user.id).await;
    refresh_lists(&state).await;
    Redirect::to("/admin#lists").into_response()
}

pub async fn remove_whitelist(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Form(form): Form<ListItemForm>,
) -> Response {
    if let Err(r) = require_lists(&current_user) { return r; }
    let _ = crate::db::remove_from_whitelist(&state.db, &form.slug).await;
    refresh_lists(&state).await;
    Redirect::to("/admin#lists").into_response()
}

pub async fn add_blacklist(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Form(form): Form<ListItemForm>,
) -> Response {
    if let Err(r) = require_lists(&current_user) { return r; }
    let _ = crate::db::add_to_blacklist(&state.db, &form.slug, current_user.id).await;
    refresh_lists(&state).await;
    Redirect::to("/admin#lists").into_response()
}

pub async fn remove_blacklist(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Form(form): Form<ListItemForm>,
) -> Response {
    if let Err(r) = require_lists(&current_user) { return r; }
    let _ = crate::db::remove_from_blacklist(&state.db, &form.slug).await;
    refresh_lists(&state).await;
    Redirect::to("/admin#lists").into_response()
}

// ── JSON API for Obsidian plugin ──────────────────────────────────────────────

pub async fn api_list_users(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.is_admin && !current_user.can_manage_users {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Keine Berechtigung"}))).into_response();
    }
    match crate::db::list_users_with_permissions(&state.db).await {
        Ok(users) => Json(users).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UpdatePermJson {
    pub can_upload: bool,
    pub can_manage_users: bool,
    pub can_manage_lists: bool,
    pub folder_restriction: bool,
    #[serde(default)]
    pub sees_published: bool,
    #[serde(default)]
    pub sees_all: bool,
    pub folders: Option<Vec<String>>,
}

pub async fn api_update_permissions(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(user_id): Path<u32>,
    ExtractJson(body): ExtractJson<UpdatePermJson>,
) -> Response {
    if !current_user.can_manage_users && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Keine Berechtigung"}))).into_response();
    }
    if let Ok(Some(target)) = crate::db::find_user_by_id(&state.db, user_id).await {
        if target.is_admin {
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Admin-Berechtigungen können nicht verändert werden"}))).into_response();
        }
    }
    let perms = crate::db::UserPermissions {
        user_id,
        can_upload: body.can_upload,
        can_manage_users: body.can_manage_users,
        can_manage_lists: body.can_manage_lists,
        folder_restriction: body.folder_restriction,
        sees_published: body.sees_published,
        sees_all: body.sees_all,
    };
    if let Err(e) = crate::db::upsert_permissions(&state.db, &perms).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response();
    }
    let folders = body.folders.unwrap_or_default();
    let _ = crate::db::set_folder_access(&state.db, user_id, &folders).await;
    Json(serde_json::json!({"ok": true})).into_response()
}

pub async fn api_get_lists(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.can_manage_lists && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Keine Berechtigung"}))).into_response();
    }
    let whitelist = crate::db::get_whitelist(&state.db).await.unwrap_or_default();
    let blacklist = crate::db::get_blacklist(&state.db).await.unwrap_or_default();
    Json(serde_json::json!({"whitelist": whitelist, "blacklist": blacklist})).into_response()
}

#[derive(Deserialize)]
pub struct SetListsBody {
    pub whitelist: Vec<String>,
    pub blacklist: Vec<String>,
}

pub async fn api_set_lists(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    ExtractJson(body): ExtractJson<SetListsBody>,
) -> Response {
    if !current_user.can_manage_lists && !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Keine Berechtigung"}))).into_response();
    }
    let _ = crate::db::set_whitelist(&state.db, &body.whitelist, current_user.id).await;
    let _ = crate::db::set_blacklist(&state.db, &body.blacklist, current_user.id).await;
    refresh_lists(&state).await;
    Json(serde_json::json!({"ok": true})).into_response()
}

// ── App settings API ──────────────────────────────────────────────────────────

pub async fn api_get_settings(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    let publish_field = state.publish_field.read().unwrap().clone();
    let group_field = state.group_field.read().unwrap().clone();
    Json(serde_json::json!({
        "publish_field": publish_field,
        "group_field": group_field,
    })).into_response()
}

#[derive(Deserialize)]
pub struct SetSettingsBody {
    pub publish_field: String,
    pub group_field: Option<String>,
}

pub async fn api_set_settings(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    ExtractJson(body): ExtractJson<SetSettingsBody>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }

    if let Err(e) = crate::db::set_setting(&state.db, "publish_field", &body.publish_field).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response();
    }

    let group_field_val = body.group_field.clone().unwrap_or_else(|| "group".to_string());
    let group_field_val = if group_field_val.trim().is_empty() { "group".to_string() } else { group_field_val.trim().to_string() };
    if let Err(e) = crate::db::set_setting(&state.db, "group_field", &group_field_val).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response();
    }

    // Update in-memory caches
    *state.publish_field.write().unwrap() = body.publish_field.clone();
    *state.group_field.write().unwrap() = group_field_val.clone();

    // Reload vault so publish/group field changes take effect
    match crate::vault::Vault::load(&state.config.server.vault_path, &body.publish_field, &group_field_val) {
        Ok(new_vault) => { *state.vault.write().await = new_vault; }
        Err(e) => { tracing::warn!("Vault reload after settings change failed: {}", e); }
    }

    Json(serde_json::json!({"ok": true})).into_response()
}

// ── Group API ─────────────────────────────────────────────────────────────────

pub async fn api_list_groups(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    match crate::db::list_groups_with_members(&state.db).await {
        Ok(groups) => Json(groups).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct CreateGroupBody {
    pub name: String,
}

pub async fn api_create_group(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    ExtractJson(body): ExtractJson<CreateGroupBody>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"Name darf nicht leer sein"}))).into_response();
    }
    match crate::db::create_group(&state.db, &name).await {
        Ok(id) => Json(serde_json::json!({"ok": true, "id": id})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn api_delete_group(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(group_id): Path<u32>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    match crate::db::delete_group(&state.db, group_id).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
pub struct AddMemberBody {
    pub user_id: u32,
}

pub async fn api_add_group_member(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(group_id): Path<u32>,
    ExtractJson(body): ExtractJson<AddMemberBody>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    match crate::db::add_user_to_group(&state.db, body.user_id, group_id).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn api_remove_group_member(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path((group_id, user_id)): Path<(u32, u32)>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    match crate::db::remove_user_from_group(&state.db, user_id, group_id).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

// ── External note metadata API ────────────────────────────────────────────────

pub async fn api_get_all_note_meta(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    let meta = crate::external_meta::read(&state.config.server.vault_path);
    Json(meta).into_response()
}

#[derive(Deserialize)]
pub struct SetNoteMetaBody {
    pub groups: Vec<String>,
}

pub async fn api_set_note_meta(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(slug): Path<String>,
    ExtractJson(body): ExtractJson<SetNoteMetaBody>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Nur Admins"}))).into_response();
    }
    let groups: Vec<String> = body.groups.into_iter()
        .map(|g| g.trim().to_lowercase())
        .filter(|g| !g.is_empty())
        .collect();
    let note_meta = crate::external_meta::NoteMeta {
        groups: if groups.is_empty() { None } else { Some(groups) },
    };
    if let Err(e) = crate::external_meta::update_note(&state.config.server.vault_path, &slug, note_meta) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response();
    }

    // Reload vault so the change takes effect immediately
    let pf = state.publish_field.read().unwrap().clone();
    let gf = state.group_field.read().unwrap().clone();
    if let Ok(new_vault) = crate::vault::Vault::load(&state.config.server.vault_path, &pf, &gf) {
        *state.vault.write().await = new_vault;
    }

    Json(serde_json::json!({"ok": true})).into_response()
}

fn require_lists(user: &CurrentUser) -> Result<(), Response> {
    if user.can_manage_lists || user.is_admin {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Keine Berechtigung").into_response())
    }
}

async fn refresh_lists(state: &AppState) {
    if let Ok(wl) = crate::db::get_whitelist(&state.db).await {
        let mut guard = state.whitelist.write().unwrap();
        *guard = wl.into_iter().collect();
    }
    if let Ok(bl) = crate::db::get_blacklist(&state.db).await {
        let mut guard = state.blacklist.write().unwrap();
        *guard = bl.into_iter().collect();
    }
}
