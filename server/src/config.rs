use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct AppConfig {
    /// SQLite connection string, e.g. "sqlite:./linkly.db"
    pub database_url: String,

    /// Secret key for signing JWT tokens
    pub jwt_secret: String,

    /// Optional: seed the first admin user on empty database
    pub seed_admin_email: Option<String>,
    pub seed_admin_password: Option<String>,

    /// Host to bind the HTTP server to, e.g. "0.0.0.0"
    pub host: String,

    /// Port to listen on
    pub port: u16,

    /// Public base URL used when generating short links, e.g. "https://go.example.com"
    /// Must NOT have a trailing slash.
    pub base_url: String,

    /// How many hours an auth token remains valid
    pub session_duration_hours: u64,

    /// URL to redirect visitors to when they hit the root path ("/").
    pub root_redirect_url: String,

    /// S3 configuration (all optional — if any are missing, uploads are disabled)
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,

    /// Unsplash API access key (optional — if missing, Unsplash search is hidden)
    pub unsplash_access_key: Option<String>,

    /// Pexels API key (optional — combined with Unsplash for image search)
    pub pexels_api_key: Option<String>,

    /// Application title shown in nav, page titles, and footer. Defaults to "Linkly".
    pub app_title: String,
}

impl AppConfig {
    /// Load configuration from environment variables (populated by dotenvy before this is called).
    pub fn from_env() -> Result<Self> {
        let jwt_secret = std::env::var("JWT_SECRET")
            .context("JWT_SECRET must be set in the environment or .env file")?;

        if jwt_secret.trim().is_empty() {
            anyhow::bail!("JWT_SECRET must not be empty");
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

        let seed_admin_email = std::env::var("SEED_ADMIN_EMAIL")
            .ok()
            .filter(|s| !s.is_empty());
        let seed_admin_password = std::env::var("SEED_ADMIN_PASSWORD")
            .or_else(|_| std::env::var("ADMIN_PASSWORD")) // backward compat
            .ok()
            .filter(|s| !s.is_empty());

        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./linkly.db".into()),
            jwt_secret,
            seed_admin_email,
            seed_admin_password,
            host: std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port,
            base_url,
            session_duration_hours,
            root_redirect_url,
            s3_bucket: std::env::var("S3_BUCKET").ok(),
            s3_region: std::env::var("S3_REGION").ok(),
            s3_endpoint: std::env::var("S3_ENDPOINT").ok(),
            s3_access_key: std::env::var("S3_ACCESS_KEY").ok(),
            s3_secret_key: std::env::var("S3_SECRET_KEY").ok(),
            unsplash_access_key: std::env::var("UNSPLASH_ACCESS_KEY").ok(),
            pexels_api_key: std::env::var("PEXELS_API_KEY").ok(),
            app_title: std::env::var("APP_TITLE").unwrap_or_else(|_| "Linkly".into()),
        })
    }

    /// Returns true if all required S3 credentials are configured.
    pub fn s3_configured(&self) -> bool {
        self.s3_bucket.is_some()
            && self.s3_region.is_some()
            && self.s3_access_key.is_some()
            && self.s3_secret_key.is_some()
    }

    /// Returns true if the Unsplash API key is set.
    pub fn unsplash_configured(&self) -> bool {
        self.unsplash_access_key.is_some()
    }

    /// Returns true if any image search provider is configured.
    pub fn image_search_configured(&self) -> bool {
        self.unsplash_access_key.is_some() || self.pexels_api_key.is_some()
    }
}
