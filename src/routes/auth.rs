use axum::{
    extract::{Extension, Form, Path, Query, State},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use cookie::{Cookie, SameSite};
use serde::Deserialize;
use time::Duration;

use crate::{auth::{CurrentUser, SESSION_COOKIE}, AppState};

// ── Login ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginQuery {
    pub next: Option<String>,
    pub registered: Option<String>,
}

pub async fn login_page(
    State(state): State<AppState>,
    Query(params): Query<LoginQuery>,
    maybe_user: Option<Extension<CurrentUser>>,
) -> Response {
    if maybe_user.is_some() {
        return Redirect::to("/").into_response();
    }
    render_login(&state, "", &params.next.unwrap_or_default(), params.registered.is_some())
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    let next = form.next.as_deref().unwrap_or_default();

    let user = match crate::db::find_user_by_username(&state.db, &form.username).await {
        Ok(Some(u)) => u,
        _ => return render_login(&state, "Ungültige Anmeldedaten.", next, false),
    };

    if !crate::auth::verify_password(&form.password, &user.password_hash) {
        return render_login(&state, "Ungültige Anmeldedaten.", next, false);
    }

    let token = crate::auth::generate_token();
    let expires = chrono::Utc::now()
        + chrono::Duration::hours(state.config.auth.session_duration_hours);

    if let Err(e) = crate::db::create_session(&state.db, &token, user.id, expires).await {
        tracing::error!("create_session: {}", e);
        return render_login(&state, "Serverfehler, bitte erneut versuchen.", next, false);
    }

    let mut c = Cookie::new(SESSION_COOKIE, token);
    c.set_http_only(true);
    c.set_path("/");
    c.set_same_site(SameSite::Lax);
    c.set_max_age(Duration::hours(state.config.auth.session_duration_hours));

    let redirect_to = form
        .next
        .filter(|s| !s.is_empty() && s.starts_with('/'))
        .unwrap_or_else(|| "/".to_string());

    (jar.add(c), Redirect::to(&redirect_to)).into_response()
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        let _ = crate::db::delete_session(&state.db, c.value()).await;
    }
    let removal = Cookie::build((SESSION_COOKIE, ""))
        .path("/")
        .max_age(Duration::seconds(0))
        .build();
    (jar.remove(removal), Redirect::to("/login")).into_response()
}

fn render_login(state: &AppState, error: &str, next: &str, registered: bool) -> Response {
    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("next", next);
    ctx.insert("error", error);
    ctx.insert("registered", &registered);
    Html(state.tera.render("login.html", &ctx).unwrap_or_default()).into_response()
}

// ── API token (for upload script) ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TokenRequest {
    pub username: String,
    pub password: String,
}

pub async fn api_token(
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<TokenRequest>,
) -> Response {
    let user = match crate::db::find_user_by_username(&state.db, &body.username).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({ "error": "Invalid credentials" })),
            )
                .into_response()
        }
    };

    if !crate::auth::verify_password(&body.password, &user.password_hash) {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "Invalid credentials" })),
        )
            .into_response();
    }

    let token = crate::auth::generate_token();
    let expires = chrono::Utc::now()
        + chrono::Duration::hours(state.config.auth.session_duration_hours);

    if let Err(e) = crate::db::create_session(&state.db, &token, user.id, expires).await {
        tracing::error!("api_token create_session: {}", e);
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "error": "Server error" })),
        )
            .into_response();
    }

    axum::Json(serde_json::json!({
        "token": token,
        "username": user.username,
        "is_admin": user.is_admin,
        "expires_at": expires.to_rfc3339(),
    }))
    .into_response()
}

// ── Registration via invite token ─────────────────────────────────────────────

pub async fn register_page(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response {
    let valid = crate::db::is_invite_valid(&state.db, &token).await.unwrap_or(false);
    render_register(&state, &token, valid, "")
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    pub password_confirm: String,
}

pub async fn register(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(form): Form<RegisterForm>,
) -> Response {
    if !crate::db::is_invite_valid(&state.db, &token).await.unwrap_or(false) {
        return render_register(&state, &token, false, "Einladungslink ungültig oder abgelaufen.");
    }

    if form.username.trim().len() < 3 {
        return render_register(&state, &token, true, "Benutzername muss mindestens 3 Zeichen haben.");
    }
    if form.password.len() < 8 {
        return render_register(&state, &token, true, "Passwort muss mindestens 8 Zeichen haben.");
    }
    if form.password != form.password_confirm {
        return render_register(&state, &token, true, "Passwörter stimmen nicht überein.");
    }
    if crate::db::find_user_by_username(&state.db, form.username.trim())
        .await
        .unwrap_or(None)
        .is_some()
    {
        return render_register(&state, &token, true, "Benutzername bereits vergeben.");
    }

    let hash = match crate::auth::hash_password(&form.password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("hash_password: {}", e);
            return render_register(&state, &token, true, "Serverfehler.");
        }
    };

    if !crate::db::consume_invite(&state.db, &token).await.unwrap_or(false) {
        return render_register(&state, &token, false, "Einladungslink konnte nicht eingelöst werden.");
    }

    match crate::db::create_user(&state.db, form.username.trim(), &hash, false).await {
        Ok(_) => Redirect::to("/login?registered=1").into_response(),
        Err(e) => {
            tracing::error!("create_user: {}", e);
            render_register(&state, &token, true, "Fehler beim Erstellen des Nutzers.")
        }
    }
}

fn render_register(state: &AppState, token: &str, valid: bool, error: &str) -> Response {
    let mut ctx = tera::Context::new();
    ctx.insert("site_title", &state.config.server.site_title);
    ctx.insert("token", token);
    ctx.insert("valid", &valid);
    ctx.insert("error", error);
    Html(state.tera.render("register.html", &ctx).unwrap_or_default()).into_response()
}
