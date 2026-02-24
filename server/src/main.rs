use std::{net::SocketAddr, sync::Arc};

use axum::{
    routing::{get, post},
    Router,
};
use sqlx::sqlite::SqlitePoolOptions;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod cache;
mod config;
mod db;

mod geo;
mod handlers;
mod models;

use auth::SessionStore;
use cache::LinkCache;
use geo::GeoCache;

// ── Shared application state ───────────────────────────────────────────────

pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub config: config::AppConfig,
    pub cache: LinkCache,
    pub sessions: SessionStore,
    /// In-memory cache for IP → GeoInfo lookups so the same IP is never
    /// looked up more than once per server lifetime.
    pub geo_cache: GeoCache,
}

// ── Entry point ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env (ignore error if file is absent — env vars may already be set)
    dotenvy::dotenv().ok();

    // Initialise structured logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "linkly=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration from environment
    let config = config::AppConfig::from_env()?;
    tracing::info!("Starting Linkly on {}:{}", config.host, config.port);
    tracing::info!("Base URL: {}", config.base_url);

    // Open SQLite connection pool
    // CREATE the file if it doesn't exist yet
    let db = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(
            config
                .database_url
                .parse::<sqlx::sqlite::SqliteConnectOptions>()?
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .foreign_keys(true),
        )
        .await?;

    // Run embedded migrations (files in migrations/)
    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("Database migrations applied");

    // Build shared state
    let cache = LinkCache::new();
    db::warm_cache(&db, &cache).await?;

    let sessions = SessionStore::new(config.session_duration_hours);
    let geo_cache = GeoCache::new();

    let state = Arc::new(AppState {
        db,
        config,
        cache,
        sessions,
        geo_cache,
    });

    // ── Router ─────────────────────────────────────────────────────────────
    let admin_router = Router::new()
        // Root of /admin → dashboard (or login redirect via AuthUser)
        .route(
            "/",
            get(|| async { axum::response::Redirect::to("/admin/dashboard") }),
        )
        .route(
            "/login",
            get(handlers::admin::login_page).post(handlers::admin::login),
        )
        .route("/logout", get(handlers::admin::logout))
        .route("/dashboard", get(handlers::admin::dashboard))
        .route("/links", post(handlers::admin::create_link))
        .route("/links/:id/delete", post(handlers::admin::delete_link))
        .route("/links/:id/analytics", get(handlers::admin::analytics));

    let app = Router::new()
        // Root redirect
        .route("/", get(handlers::admin::index))
        // Fly.io health check — returns 200 OK with no auth required
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        // Admin panel (all under /admin/*)
        .nest("/admin", admin_router)
        // Short-link redirect — must come LAST so /admin/* takes priority
        .route("/:code", get(handlers::redirect::redirect))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // ── Serve ──────────────────────────────────────────────────────────────
    let bind_addr = format!(
        "{}:{}",
        // Re-read from state would require a clone; just re-parse from env.
        std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".into()),
        std::env::var("PORT").unwrap_or_else(|_| "3000".into()),
    );

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("Listening on http://{}", listener.local_addr()?);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
