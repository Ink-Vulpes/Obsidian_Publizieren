use axum::{
    extract::{Extension, Form, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::{auth::CurrentUser, AppState};

#[derive(serde::Serialize)]
struct ProposalView {
    id: u32,
    note_slug: String,
    proposer_username: String,
    proposed_content: String,
    message: Option<String>,
    created_at: DateTime<Utc>,
    current_content: Option<String>,
}

// ── Admin panel page (redirect to /admin/users) ───────────────────────────────

pub async fn admin_page(
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.can_access_admin {
        return (StatusCode::FORBIDDEN, "Zugriff verweigert").into_response();
    }
    Redirect::to("/admin/users").into_response()
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn base_ctx(state: &AppState, current_user: &CurrentUser, tab: &str) -> tera::Context {
    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("current_user", current_user);
    ctx.insert("active_tab", tab);
    ctx
}

fn render(state: &AppState, template: &str, ctx: tera::Context) -> Response {
    Html(
        state.tera.render(template, &ctx)
            .unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")),
    )
    .into_response()
}

// ── /admin/users ──────────────────────────────────────────────────────────────

pub async fn admin_users_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.can_access_admin {
        return (StatusCode::FORBIDDEN, "Zugriff verweigert").into_response();
    }
    if !current_user.can_manage_users && !current_user.is_admin {
        return Redirect::to("/admin").into_response();
    }

    let users = crate::db::list_users_with_permissions(&state.db).await.unwrap_or_default();
    let groups = crate::db::list_groups_with_members(&state.db).await.unwrap_or_default();

    let vault = state.vault.read().await;
    let mut all_slugs: Vec<String> = vault.notes.keys().cloned().collect();
    all_slugs.sort();
    drop(vault);

    let mut ctx = base_ctx(&state, &current_user, "users");
    ctx.insert("users", &users);
    ctx.insert("groups", &groups);
    ctx.insert("all_slugs", &all_slugs);
    render(&state, "admin_users.html", ctx)
}

// ── /admin/groups ─────────────────────────────────────────────────────────────

pub async fn admin_groups_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Zugriff verweigert").into_response();
    }

    let groups = crate::db::list_groups_with_members(&state.db).await.unwrap_or_default();
    let users = crate::db::list_users_with_permissions(&state.db).await.unwrap_or_default();

    let mut ctx = base_ctx(&state, &current_user, "groups");
    ctx.insert("groups", &groups);
    ctx.insert("users", &users);
    render(&state, "admin_groups.html", ctx)
}

// ── /admin/proposals ──────────────────────────────────────────────────────────

pub async fn admin_proposals_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Zugriff verweigert").into_response();
    }

    let raw_proposals = crate::db::list_proposals(&state.db).await.unwrap_or_default();
    let vault = state.vault.read().await;
    let proposals: Vec<ProposalView> = raw_proposals
        .into_iter()
        .map(|p| {
            let current_content = vault.notes.get(&p.note_slug).and_then(|entry| {
                let path = std::path::Path::new(&state.config.server.vault_path).join(&entry.path);
                std::fs::read_to_string(path).ok()
            });
            ProposalView {
                id: p.id,
                note_slug: p.note_slug,
                proposer_username: p.proposer_username,
                proposed_content: p.proposed_content,
                message: p.message,
                created_at: p.created_at,
                current_content,
            }
        })
        .collect();
    let proposals_count = proposals.len();
    drop(vault);

    let mut ctx = base_ctx(&state, &current_user, "proposals");
    ctx.insert("proposals", &proposals);
    ctx.insert("proposals_count", &proposals_count);
    render(&state, "admin_proposals.html", ctx)
}

// ── /admin/settings ───────────────────────────────────────────────────────────

pub async fn admin_settings_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> Response {
    if !current_user.is_admin {
        return (StatusCode::FORBIDDEN, "Zugriff verweigert").into_response();
    }

    let ctx = base_ctx(&state, &current_user, "settings");
    render(&state, "admin_settings.html", ctx)
}

// ── Permission update (HTML form POST) ───────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdatePermForm {
    pub can_upload: Option<String>,
    pub can_manage_users: Option<String>,
    pub can_manage_lists: Option<String>,
    pub folder_restriction: Option<String>,
    pub sees_published: Option<String>,
    pub sees_all: Option<String>,
    pub can_propose: Option<String>,
    pub can_use_mcp: Option<String>,
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

    let perms = crate::db::UserPermissions {
        user_id,
        can_upload: form.can_upload.as_deref() == Some("on"),
        can_manage_users: form.can_manage_users.as_deref() == Some("on"),
        can_manage_lists: form.can_manage_lists.as_deref() == Some("on"),
        folder_restriction: form.folder_restriction.as_deref() == Some("on"),
        sees_published: form.sees_published.as_deref() == Some("on"),
        sees_all: form.sees_all.as_deref() == Some("on"),
        can_propose: form.can_propose.as_deref() == Some("on"),
        can_use_mcp: form.can_use_mcp.as_deref() == Some("on"),
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
        let _ = crate::db::set_folder_access(&state.db, user_id, &[]).await;
    }

    Redirect::to("/admin/users").into_response()
}

// ── User deletion ─────────────────────────────────────────────────────────────
// Kept as REST for the web admin DELETE call from JS

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

// ── Invite creation ───────────────────────────────────────────────────────────
// Kept as REST for the web admin JS call

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
