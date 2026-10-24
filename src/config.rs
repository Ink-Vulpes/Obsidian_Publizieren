#[derive(Clone, Debug)]
pub struct Config {
    pub vault_path: String,
    pub port: u16,
    pub site_title: String,
    pub site_description: String,
    pub base_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            vault_path: std::env::var("VAULT_PATH")
                .unwrap_or_else(|_| "./vault".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            site_title: std::env::var("SITE_TITLE")
                .unwrap_or_else(|_| "My Digital Garden".to_string()),
            site_description: std::env::var("SITE_DESCRIPTION")
                .unwrap_or_else(|_| "A collection of interconnected notes".to_string()),
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
        }
    }
}
