use axum::{
    extract::{Extension, Form, Path, State},
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

    let perms = crate::db::UserPermissions {
        user_id,
        can_upload: form.can_upload.as_deref() == Some("on"),
        can_manage_users: form.can_manage_users.as_deref() == Some("on"),
        can_manage_lists: form.can_manage_lists.as_deref() == Some("on"),
        folder_restriction: form.folder_restriction.as_deref() == Some("on"),
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
