use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;

pub const SESSION_COOKIE: &str = "ob_session";

/// Information about the currently authenticated user, injected into request extensions.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CurrentUser {
    pub id: u32,
    pub username: String,
    pub is_admin: bool,
    pub can_upload: bool,
    pub can_manage_users: bool,
    pub can_manage_lists: bool,
    pub can_access_admin: bool,
    pub folder_restriction: bool,
    pub allowed_folders: Vec<String>,
}

impl CurrentUser {
    /// Returns true if this user should be able to see the given note slug.
    pub fn can_see_note(&self, slug: &str) -> bool {
        if self.is_admin || !self.folder_restriction {
            return true;
        }
        self.allowed_folders.iter().any(|folder| {
            let f = folder.trim_end_matches('/');
            slug == f || slug.starts_with(&format!("{}/", f))
        })
    }
}

// ── Axum middleware ──────────────────────────────────────────────────────────

pub async fn auth_layer(
    State(state): State<crate::AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    // Paths that never require authentication
    let is_public = path == "/login"
        || path == "/health"
        || path == "/api/auth/token"
        || path == "/auto-login"
        || path.starts_with("/register/")
        || path.starts_with("/static/");

    if let Some(token) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        if let Ok(Some((user, perms, folders))) = crate::db::get_session_user(&state.db, &token).await {
            let can_manage_users = user.is_admin || perms.can_manage_users;
            let can_manage_lists = user.is_admin || perms.can_manage_lists;
            let current_user = CurrentUser {
                id: user.id,
                username: user.username,
                is_admin: user.is_admin,
                can_upload: user.is_admin || perms.can_upload,
                can_manage_users,
                can_manage_lists,
                can_access_admin: user.is_admin || can_manage_users || can_manage_lists,
                folder_restriction: !user.is_admin && perms.folder_restriction,
                allowed_folders: folders,
            };
            req.extensions_mut().insert(current_user);
            return next.run(req).await;
        }
    }

    // Also accept Bearer token for API clients (uploader script)
    if let Some(bearer) = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.to_string())
    {
        if let Ok(Some((user, perms, folders))) = crate::db::get_session_user(&state.db, &bearer).await {
            let can_manage_users = user.is_admin || perms.can_manage_users;
            let can_manage_lists = user.is_admin || perms.can_manage_lists;
            let current_user = CurrentUser {
                id: user.id,
                username: user.username,
                is_admin: user.is_admin,
                can_upload: user.is_admin || perms.can_upload,
                can_manage_users,
                can_manage_lists,
                can_access_admin: user.is_admin || can_manage_users || can_manage_lists,
                folder_restriction: !user.is_admin && perms.folder_restriction,
                allowed_folders: folders,
            };
            req.extensions_mut().insert(current_user);
            return next.run(req).await;
        }
    }

    if is_public {
        return next.run(req).await;
    }

    // Not authenticated → redirect to login, preserving intended URL
    let encoded = urlencoding::encode(&path).to_string();
    Redirect::to(&format!("/login?next={}", encoded)).into_response()
}

// ── Password helpers ──────────────────────────────────────────────────────────

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    use argon2::{Argon2, PasswordHasher};
    use argon2::password_hash::SaltString;

    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("password hash: {}", e))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::{Argon2, PasswordVerifier};
    use argon2::password_hash::PasswordHash;

    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

pub fn generate_token() -> String {
    uuid::Uuid::new_v4().to_string()
}
