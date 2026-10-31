use axum::{extract::DefaultBodyLimit, http::HeaderValue, middleware, routing::{delete, get, post}, Router};
use tower_http::set_header::SetResponseHeaderLayer;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tokio::sync::RwLock as AsyncRwLock;
use tower_http::cors::{AllowHeaders, AllowMethods, CorsLayer};
use tower_http::services::ServeDir;

mod auth;
mod config;
mod db;
mod external_meta;
mod parser;
mod routes;
mod vault;

pub use config::Config;
pub use vault::Vault;

#[derive(Clone)]
pub struct AppState {
    pub vault: Arc<AsyncRwLock<Vault>>,
    pub config: Arc<Config>,
    pub tera: Arc<tera::Tera>,
    pub db: sqlx::MySqlPool,
    /// Global whitelist cache (empty = show all)
    pub whitelist: Arc<RwLock<HashSet<String>>>,
    /// Global blacklist cache
    pub blacklist: Arc<RwLock<HashSet<String>>>,
    /// Frontmatter field checked for publish flag; empty = no filter
    pub publish_field: Arc<RwLock<String>>,
    /// Frontmatter field used for group-based access; default "group"
    pub group_field: Arc<RwLock<String>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // ── Config ────────────────────────────────────────────────────────────────
    let config = Arc::new(Config::load().unwrap_or_else(|e| {
        tracing::warn!("Could not load config.yml ({}), using env vars", e);
        Config::load().expect("Failed to build config from env vars")
    }));

    // ── Database ──────────────────────────────────────────────────────────────
    tracing::info!("Connecting to database…");
    let db = db::create_pool(&config.database.url).await
        .expect("Failed to connect to MySQL – is the database running?");

    tracing::info!("Running schema migrations…");
    db::run_migrations(&db).await.expect("Migration failed");

    // Bootstrap admin user on first startup
    if db::count_users(&db).await.unwrap_or(0) == 0 {
        tracing::info!("No users found – creating initial admin '{}'", config.admin.username);
        let hash = auth::hash_password(&config.admin.password)
            .expect("Failed to hash admin password");
        db::create_user(&db, &config.admin.username, &hash, true)
            .await
            .expect("Failed to create admin user");
    }

    // ── Settings ──────────────────────────────────────────────────────────────
    let publish_field_str = db::get_setting(&db, "publish_field")
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| "published".to_string());
    let publish_field: Arc<RwLock<String>> = Arc::new(RwLock::new(publish_field_str.clone()));

    let group_field_str = db::get_setting(&db, "group_field")
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| "group".to_string());
    let group_field: Arc<RwLock<String>> = Arc::new(RwLock::new(group_field_str.clone()));

    // ── Vault ─────────────────────────────────────────────────────────────────
    tracing::info!("Loading vault from {}", config.server.vault_path);
    let vault = Vault::load(&config.server.vault_path, &publish_field_str, &group_field_str).expect("Failed to load vault");
    let vault = Arc::new(AsyncRwLock::new(vault));

    // ── Whitelist / Blacklist cache ───────────────────────────────────────────
    let whitelist: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(
        db::get_whitelist(&db).await.unwrap_or_default().into_iter().collect(),
    ));
    let blacklist: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(
        db::get_blacklist(&db).await.unwrap_or_default().into_iter().collect(),
    ));

    // ── Templates ─────────────────────────────────────────────────────────────
    let tera = Arc::new(build_templates()?);

    let state = AppState { vault, config: config.clone(), tera, db, whitelist, blacklist, publish_field, group_field };

    // ── Router ────────────────────────────────────────────────────────────────
    let app = Router::new()
        // Health (no auth)
        .route("/health", get(|| async { "ok" }))
        // Auth (no auth required)
        .route("/login", get(routes::auth::login_page).post(routes::auth::login))
        .route("/logout", post(routes::auth::logout))
        .route("/auto-login", get(routes::auth::auto_login))
        .route("/register/:token", get(routes::auth::register_page).post(routes::auth::register))
        // Static files (no auth, no-cache so JS/CSS updates are picked up immediately)
        .nest_service(
            "/static",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    axum::http::header::CACHE_CONTROL,
                    HeaderValue::from_static("no-store"),
                ))
                .service(ServeDir::new("static")),
        )
        // API auth endpoint for upload script
        .route("/api/auth/token", post(routes::auth::api_token))
        // Protected routes
        .route("/", get(routes::note::index))
        .route("/note/*path", get(routes::note::render_note))
        .route("/api/graph", get(routes::api::graph_data))
        .route("/api/search", get(routes::api::search))
        .route("/api/files", get(routes::api::file_list))
        .route("/api/reload", get(routes::api::reload_vault))
        .route("/api/files/manifest", get(routes::api::file_manifest))
        .route("/api/upload", post(routes::api::upload_files)
            .layer(DefaultBodyLimit::max(500 * 1024 * 1024)))
        .route("/api/files/delete", delete(routes::api::delete_file))
        .route("/api/files/delete-all", delete(routes::api::delete_all_files))
        .route("/api/sync/download", get(routes::api::download_file))
        .route("/api/sync/device", post(routes::api::register_device))
        .route("/api/admin/users", get(routes::admin::api_list_users))
        .route("/api/admin/users/:id/permissions", post(routes::admin::api_update_permissions))
        .route("/api/admin/lists", get(routes::admin::api_get_lists).post(routes::admin::api_set_lists))
        .route("/api/admin/settings", get(routes::admin::api_get_settings).post(routes::admin::api_set_settings))
        .route("/api/admin/groups", get(routes::admin::api_list_groups).post(routes::admin::api_create_group))
        .route("/api/admin/groups/:id", delete(routes::admin::api_delete_group))
        .route("/api/admin/groups/:id/members", post(routes::admin::api_add_group_member))
        .route("/api/admin/groups/:id/members/:uid", delete(routes::admin::api_remove_group_member))
        .route("/api/admin/note-meta", get(routes::admin::api_get_all_note_meta))
        .route("/api/admin/note-meta/*slug", post(routes::admin::api_set_note_meta))
        .route("/admin", get(routes::admin::admin_page))
        .route("/admin/invite", post(routes::admin::create_invite))
        .route("/admin/users/:id/permissions", post(routes::admin::update_permissions))
        .route("/admin/users/:id", delete(routes::admin::delete_user))
        .route("/admin/whitelist/add", post(routes::admin::add_whitelist))
        .route("/admin/whitelist/remove", post(routes::admin::remove_whitelist))
        .route("/admin/blacklist/add", post(routes::admin::add_blacklist))
        .route("/admin/blacklist/remove", post(routes::admin::remove_blacklist))
        .nest_service("/vault-assets", ServeDir::new(config.server.vault_path.clone()))
        .layer(middleware::from_fn_with_state(state.clone(), auth::auth_layer))
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(AllowMethods::any())
                .allow_headers(AllowHeaders::any()),
        )
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.server.port);
    tracing::info!("Serving at http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_templates() -> anyhow::Result<tera::Tera> {
    let mut tera = tera::Tera::default();
    tera.add_raw_templates(vec![
        ("base.html",       include_str!("../templates/base.html")),
        ("note.html",       include_str!("../templates/note.html")),
        ("canvas.html",     include_str!("../templates/canvas.html")),
        ("excalidraw.html", include_str!("../templates/excalidraw.html")),
        ("index.html",      include_str!("../templates/index.html")),
        ("login.html",      include_str!("../templates/login.html")),
        ("register.html",   include_str!("../templates/register.html")),
        ("admin.html",      include_str!("../templates/admin.html")),
        ("bases.html",      include_str!("../templates/bases.html")),
    ])?;
    Ok(tera)
}
