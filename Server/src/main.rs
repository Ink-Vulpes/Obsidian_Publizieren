use axum::{extract::DefaultBodyLimit, http::HeaderValue, middleware, routing::{delete, get, post}, Router};
use tower_http::set_header::SetResponseHeaderLayer;
use std::sync::{Arc, RwLock};
use tokio::sync::RwLock as AsyncRwLock;
use tower_http::cors::{AllowHeaders, AllowMethods, CorsLayer};
use tower_http::services::ServeDir;

mod auth;
mod config;
mod db;
mod external_meta;
mod graphql;
mod parser;
mod query;
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
    /// Frontmatter field checked for publish flag; empty = no filter
    pub publish_field: Arc<RwLock<String>>,
    /// Frontmatter field used for group-based access; default "group"
    pub group_field: Arc<RwLock<String>>,
}

/// Reload vault from disk and apply current group queries.
pub async fn reload_vault(state: &AppState) {
    let pf = state.publish_field.read().unwrap().clone();
    let gf = state.group_field.read().unwrap().clone();
    match vault::Vault::load(&state.config.server.vault_path, &pf, &gf) {
        Ok(mut new_vault) => {
            if let Ok(groups) = db::list_groups_with_members(&state.db).await {
                let gq: Vec<vault::GroupQuery> = groups
                    .into_iter()
                    .map(|g| vault::GroupQuery { name: g.name, query: g.query })
                    .collect();
                new_vault.apply_group_queries(&gq);
            }
            *state.vault.write().await = new_vault;
        }
        Err(e) => tracing::warn!("Vault reload failed: {}", e),
    }
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
    let mut initial_vault = Vault::load(&config.server.vault_path, &publish_field_str, &group_field_str)
        .expect("Failed to load vault");
    if let Ok(groups) = db::list_groups_with_members(&db).await {
        let gq: Vec<vault::GroupQuery> = groups
            .into_iter()
            .map(|g| vault::GroupQuery { name: g.name, query: g.query })
            .collect();
        initial_vault.apply_group_queries(&gq);
    }
    let vault = Arc::new(AsyncRwLock::new(initial_vault));

    // ── Templates ─────────────────────────────────────────────────────────────
    let tera = Arc::new(build_templates()?);

    let state = AppState { vault, config: config.clone(), tera, db, publish_field, group_field };

    // ── GraphQL schema ────────────────────────────────────────────────────────
    let schema = Arc::new(graphql::build_schema(state.clone()));

    // ── Router ────────────────────────────────────────────────────────────────
    let app = Router::new()
        // Health (no auth)
        .route("/health", get(|| async { "ok" }))
        // Auth (no auth required)
        .route("/login", get(routes::auth::login_page).post(routes::auth::login))
        .route("/logout", post(routes::auth::logout))
        .route("/auto-login", get(routes::auth::auto_login))
        .route("/register/:token", get(routes::auth::register_page).post(routes::auth::register))
        // Static files (no-cache so JS/CSS updates are picked up immediately)
        .nest_service(
            "/static",
            tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    axum::http::header::CACHE_CONTROL,
                    HeaderValue::from_static("no-store"),
                ))
                .service(ServeDir::new("static")),
        )
        // Legacy REST token endpoint (backward compat with upload script)
        .route("/api/auth/token", post(routes::auth::api_token))
        // GraphQL endpoint (auth-optional: middleware injects CurrentUser when token present)
        .route("/graphql", post(graphql::graphql_handler))
        // Protected REST routes (file ops, search, graph – used by note pages)
        .route("/", get(routes::note::index))
        .route("/note/*path", get(routes::note::render_note))
        .route("/api/graph", get(routes::api::graph_data))
        .route("/api/search", get(routes::api::search))
        .route("/api/files", get(routes::api::file_list))
        .route("/api/upload", post(routes::api::upload_files)
            .layer(DefaultBodyLimit::max(500 * 1024 * 1024)))
        .route("/api/sync/download", get(routes::api::download_file))
        // Admin HTML pages + form handlers
        .route("/admin", get(routes::admin::admin_page))
        .route("/admin/users", get(routes::admin::admin_users_page))
        .route("/admin/groups", get(routes::admin::admin_groups_page))
        .route("/admin/proposals", get(routes::admin::admin_proposals_page))
        .route("/admin/settings", get(routes::admin::admin_settings_page))
        .route("/admin/invite", post(routes::admin::create_invite))
        .route("/admin/users/:id/permissions", post(routes::admin::update_permissions))
        .route("/admin/users/:id", delete(routes::admin::delete_user))
        // MCP setup page and API endpoint
        .route("/mcp-setup", get(routes::mcp::mcp_setup_page))
        .route("/mcp-setup/token", post(routes::mcp::generate_mcp_token))
        .route("/mcp", post(routes::mcp::mcp_handler))
        // Proposal pages
        .route("/propose/*slug", get(routes::proposals::propose_page))
        .route("/api/proposals", post(routes::proposals::submit_proposal))
        .nest_service("/vault-assets", ServeDir::new(config.server.vault_path.clone()))
        .layer(axum::Extension(schema))
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
        ("admin.html",          include_str!("../templates/admin.html")),
        ("admin_layout.html",   include_str!("../templates/admin_layout.html")),
        ("admin_users.html",    include_str!("../templates/admin_users.html")),
        ("admin_groups.html",   include_str!("../templates/admin_groups.html")),
        ("admin_proposals.html",include_str!("../templates/admin_proposals.html")),
        ("admin_settings.html", include_str!("../templates/admin_settings.html")),
        ("propose.html",        include_str!("../templates/propose.html")),
        ("bases.html",      include_str!("../templates/bases.html")),
        ("mcp_setup.html",  include_str!("../templates/mcp_setup.html")),
    ])?;
    Ok(tera)
}
