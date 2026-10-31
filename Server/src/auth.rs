use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

pub const SESSION_COOKIE: &str = "ob_session";

// ── JWT Claims ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub sub: u32,
    pub username: String,
    pub is_admin: bool,
    pub can_upload: bool,
    pub can_manage_users: bool,
    pub can_manage_lists: bool,
    pub folder_restriction: bool,
    pub allowed_folders: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub sees_published: bool,
    #[serde(default)]
    pub sees_all: bool,
    pub exp: usize,
    pub iat: usize,
}

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
    pub groups: Vec<String>,
    pub sees_published: bool,
    pub sees_all: bool,
}

impl CurrentUser {
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

impl From<Claims> for CurrentUser {
    fn from(c: Claims) -> Self {
        let can_manage_users = c.can_manage_users;
        let can_manage_lists = c.can_manage_lists;
        CurrentUser {
            id: c.sub,
            username: c.username,
            is_admin: c.is_admin,
            can_upload: c.can_upload,
            can_manage_users,
            can_manage_lists,
            can_access_admin: c.is_admin || can_manage_users || can_manage_lists,
            folder_restriction: c.folder_restriction,
            allowed_folders: c.allowed_folders,
            groups: c.groups,
            sees_published: c.sees_published,
            sees_all: c.sees_all,
        }
    }
}

impl CurrentUser {
    pub fn can_see_group_note(&self, required_groups: &[String]) -> bool {
        if self.is_admin || required_groups.is_empty() {
            return true;
        }
        required_groups.iter().any(|g| self.groups.contains(g))
    }
}

// ── JWT helpers ───────────────────────────────────────────────────────────────

pub fn create_jwt(
    user_id: u32,
    username: &str,
    is_admin: bool,
    can_upload: bool,
    can_manage_users: bool,
    can_manage_lists: bool,
    folder_restriction: bool,
    allowed_folders: Vec<String>,
    groups: Vec<String>,
    sees_published: bool,
    sees_all: bool,
    duration_hours: i64,
    secret: &str,
) -> anyhow::Result<String> {
    let now = chrono::Utc::now().timestamp() as usize;
    let exp = now + (duration_hours as usize) * 3600;

    let claims = Claims {
        sub: user_id,
        username: username.to_string(),
        is_admin,
        can_upload,
        can_manage_users,
        can_manage_lists,
        folder_restriction,
        allowed_folders,
        groups,
        sees_published,
        sees_all,
        exp,
        iat: now,
    };

    Ok(encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?)
}

pub fn verify_jwt(token: &str, secret: &str) -> Option<CurrentUser> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()?;

    Some(CurrentUser::from(data.claims))
}

// ── Axum middleware ───────────────────────────────────────────────────────────

pub async fn auth_layer(
    State(state): State<crate::AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();

    let is_public = path == "/login"
        || path == "/health"
        || path == "/api/auth/token"
        || path == "/auto-login"
        || path.starts_with("/register/")
        || path.starts_with("/static/");

    let secret = &state.config.auth.session_secret;

    // Try session cookie first
    if let Some(token) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        if let Some(user) = verify_jwt(&token, secret) {
            req.extensions_mut().insert(user);
            return next.run(req).await;
        }
    }

    // Try Bearer token (API clients)
    if let Some(bearer) = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.to_string())
    {
        if let Some(user) = verify_jwt(&bearer, secret) {
            req.extensions_mut().insert(user);
            return next.run(req).await;
        }
    }

    if is_public {
        return next.run(req).await;
    }

    let encoded = urlencoding::encode(&path).to_string();
    Redirect::to(&format!("/login?next={}", encoded)).into_response()
}

// ── Token generator (for invite tokens etc.) ──────────────────────────────────

pub fn generate_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ── Password helpers ──────────────────────────────────────────────────────────

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    use argon2::{password_hash::SaltString, Argon2, PasswordHasher};

    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("password hash: {}", e))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};

    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}
