use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// SQLite connection string, e.g. "sqlite:./linkly.db"
    pub database_url: String,

    /// Plain-text admin password loaded from the environment at startup
    pub admin_password: String,

    /// Host to bind the HTTP server to, e.g. "0.0.0.0"
    pub host: String,

    /// Port to listen on
    pub port: u16,

    /// Public base URL used when generating short links, e.g. "https://go.example.com"
    /// Must NOT have a trailing slash.
    pub base_url: String,

    /// How many hours an admin session token remains valid
    pub session_duration_hours: u64,

    /// URL to redirect visitors to when they hit the root path ("/").
    /// Defaults to "https://secedastudios.com".
    /// Set ROOT_REDIRECT_URL in the environment to override.
    pub root_redirect_url: String,
}

impl AppConfig {
    /// Load configuration from environment variables (populated by dotenvy before this is called).
    pub fn from_env() -> Result<Self> {
        let admin_password = std::env::var("ADMIN_PASSWORD")
            .context("ADMIN_PASSWORD must be set in the environment or .env file")?;

        if admin_password.trim().is_empty() {
            anyhow::bail!("ADMIN_PASSWORD must not be empty");
        }

        let port = std::env::var("PORT")
            .unwrap_or_else(|_| "3000".into())
            .parse::<u16>()
            .context("PORT must be a valid port number (1–65535)")?;

        let session_duration_hours = std::env::var("SESSION_DURATION_HOURS")
            .unwrap_or_else(|_| "24".into())
            .parse::<u64>()
            .unwrap_or(24);

        let base_url = std::env::var("BASE_URL")
            .unwrap_or_else(|_| format!("http://localhost:{port}"))
            .trim_end_matches('/')
            .to_owned();

        let root_redirect_url = std::env::var("ROOT_REDIRECT_URL")
            .unwrap_or_else(|_| "https://secedastudios.com".into())
            .trim_end_matches('/')
            .to_owned();

        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./linkly.db".into()),
            admin_password,
            host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port,
            base_url,
            session_duration_hours,
            root_redirect_url,
        })
    }
}
