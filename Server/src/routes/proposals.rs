use axum::{
    extract::{Extension, Json as ExtractJson, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
};
use serde::Deserialize;

use crate::{auth::CurrentUser, AppState};

// ── Propose page ──────────────────────────────────────────────────────────────

pub async fn propose_page(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Path(slug): Path<String>,
) -> Response {
    if !current_user.can_propose {
        return (StatusCode::FORBIDDEN, "Keine Berechtigung").into_response();
    }

    let vault = state.vault.read().await;
    let entry = match vault.notes.get(&slug) {
        Some(e) => e.clone(),
        None => return (StatusCode::NOT_FOUND, "Note nicht gefunden").into_response(),
    };
    drop(vault);

    let file_path = std::path::Path::new(&state.config.server.vault_path).join(&entry.path);
    let raw_content = std::fs::read_to_string(&file_path).unwrap_or_default();

    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("page_title", &format!("Änderungsantrag: {}", entry.title));
    ctx.insert("current_user", &current_user);
    ctx.insert("slug", &slug);
    ctx.insert("note_title", &entry.title);
    ctx.insert("raw_content", &raw_content);

    Html(
        state
            .tera
            .render("propose.html", &ctx)
            .unwrap_or_else(|e| format!("<pre>Template error: {e}</pre>")),
    )
    .into_response()
}

// ── Submit proposal API ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SubmitProposalBody {
    pub note_slug: String,
    pub proposed_content: String,
    pub message: Option<String>,
}

pub async fn submit_proposal(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    ExtractJson(body): ExtractJson<SubmitProposalBody>,
) -> Response {
    if !current_user.can_propose {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"Keine Berechtigung"}))).into_response();
    }

    // Verify the note exists
    let vault = state.vault.read().await;
    if !vault.notes.contains_key(&body.note_slug) {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"Note nicht gefunden"}))).into_response();
    }
    drop(vault);

    let message = body.message.as_deref().filter(|s| !s.trim().is_empty());

    match crate::db::create_proposal(
        &state.db,
        &body.note_slug,
        current_user.id,
        &current_user.username,
        &body.proposed_content,
        message,
    )
    .await
    {
        Ok(id) => Json(serde_json::json!({"ok": true, "id": id})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
