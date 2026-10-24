use axum::{Router, routing::get};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

mod config;
mod parser;
mod routes;
mod vault;

pub use config::Config;
pub use vault::Vault;

#[derive(Clone)]
pub struct AppState {
    pub vault: Arc<RwLock<Vault>>,
    pub config: Arc<Config>,
    pub tera: Arc<tera::Tera>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let config = Arc::new(Config::from_env());

    tracing::info!("Loading vault from {}", config.vault_path);
    let vault = Vault::load(&config.vault_path)?;
    let vault = Arc::new(RwLock::new(vault));

    let tera = Arc::new(build_templates()?);

    let state = AppState {
        vault,
        config: config.clone(),
        tera,
    };

    let app = Router::new()
        .route("/", get(routes::note::index))
        .route("/note/*path", get(routes::note::render_note))
        .route("/api/graph", get(routes::api::graph_data))
        .route("/api/search", get(routes::api::search))
        .route("/api/files", get(routes::api::file_list))
        .route("/api/reload", get(routes::api::reload_vault))
        .nest_service("/vault-assets", ServeDir::new(config.vault_path.clone()))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("Serving at http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_templates() -> anyhow::Result<tera::Tera> {
    let mut tera = tera::Tera::default();
    tera.add_raw_templates(vec![
        ("base.html", include_str!("../templates/base.html")),
        ("note.html", include_str!("../templates/note.html")),
        ("canvas.html", include_str!("../templates/canvas.html")),
        ("excalidraw.html", include_str!("../templates/excalidraw.html")),
        ("index.html", include_str!("../templates/index.html")),
    ])?;
    Ok(tera)
}
