use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub admin: AdminConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub vault_path: String,
    pub site_title: String,
    pub site_description: String,
    pub base_url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub session_secret: String,
    pub session_duration_hours: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdminConfig {
    pub username: String,
    pub password: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.yml".to_string());

        if let Ok(content) = std::fs::read_to_string(&path) {
            return Ok(serde_yaml::from_str(&content)?);
        }

        // Last resort: build from env vars for backwards compatibility
        Ok(Config {
            server: ServerConfig {
                port: std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000),
                vault_path: std::env::var("VAULT_PATH").unwrap_or_else(|_| "./vault".to_string()),
                site_title: std::env::var("SITE_TITLE").unwrap_or_else(|_| "My Digital Garden".to_string()),
                site_description: std::env::var("SITE_DESCRIPTION")
                    .unwrap_or_else(|_| "A collection of interconnected notes".to_string()),
                base_url: std::env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL")
                    .unwrap_or_else(|_| "mysql://obsidian:changeme@mysql:3306/obsidian".to_string()),
            },
            auth: AuthConfig {
                session_secret: std::env::var("SESSION_SECRET")
                    .unwrap_or_else(|_| "insecure-default-secret-change-me".to_string()),
                session_duration_hours: 168,
            },
            admin: AdminConfig {
                username: std::env::var("ADMIN_USER").unwrap_or_else(|_| "admin".to_string()),
                password: std::env::var("ADMIN_PASS").unwrap_or_else(|_| "changeme".to_string()),
            },
        })
    }
}
