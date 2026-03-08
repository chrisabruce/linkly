use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::DefaultBodyLimit,
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
mod db_bio;
mod db_users;
mod geo;
mod handlers;
mod models;
mod password;
mod s3;

use cache::LinkCache;
use geo::GeoCache;

// ── Shared application state ───────────────────────────────────────────────

pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub config: config::AppConfig,
    pub cache: LinkCache,
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

    // ── Ensure seed admin exists ────────────────────────────────────────
    if let (Some(email), Some(pass)) = (&config.seed_admin_email, &config.seed_admin_password) {
        match db_users::get_user_by_email(&db, email).await? {
            Some(_) => {
                tracing::debug!("Seed admin '{}' already exists, skipping", email);
            }
            None => {
                let hash = password::hash_password(pass)
                    .map_err(|e| anyhow::anyhow!("Failed to hash seed password: {}", e))?;
                let admin = db_users::create_user(&db, email, "Admin", &hash, "admin", true, false).await?;
                tracing::info!("Seeded admin user: {}", email);

                // Attribute existing unowned links/pages to the seed admin
                sqlx::query("UPDATE links SET user_id = ?1 WHERE user_id IS NULL")
                    .bind(admin.id)
                    .execute(&db)
                    .await?;
                sqlx::query("UPDATE bio_pages SET user_id = ?1 WHERE user_id IS NULL")
                    .bind(admin.id)
                    .execute(&db)
                    .await?;
            }
        }
    } else {
        let user_count = db_users::count_users(&db).await.unwrap_or(0);
        if user_count == 0 {
            tracing::warn!(
                "No users exist and SEED_ADMIN_EMAIL / SEED_ADMIN_PASSWORD not set. \
                 The first user to register will become admin."
            );
        }
    }

    // Build shared state
    let cache = LinkCache::new();
    db::warm_cache(&db, &cache).await?;

    let geo_cache = GeoCache::new();

    let state = Arc::new(AppState {
        db,
        config,
        cache,
        geo_cache,
    });

    // ── Router ─────────────────────────────────────────────────────────────
    let admin_router = Router::new()
        .route("/", get(handlers::admin::admin_index))
        .route(
            "/login",
            get(handlers::admin::login_page).post(handlers::admin::login),
        )
        .route(
            "/register",
            get(handlers::admin::register_page).post(handlers::admin::register),
        )
        .route("/logout", get(handlers::admin::logout))
        .route(
            "/change-password",
            get(handlers::admin::change_password_page).post(handlers::admin::change_password),
        )
        .route("/dashboard", get(handlers::admin::dashboard))
        .route("/short-links", get(handlers::admin::short_links))
        .route("/validate-code", get(handlers::admin::validate_code))
        .route("/links", post(handlers::admin::create_link))
        .route("/links/:id/delete", post(handlers::admin::delete_link))
        .route("/links/:id/analytics", get(handlers::admin::analytics))
        // Bio pages
        .route(
            "/bio",
            get(handlers::bio::list_bio_pages).post(handlers::bio::create_bio_page),
        )
        .route("/bio/new", get(handlers::bio::new_bio_page))
        .route("/bio/validate-slug", get(handlers::bio::validate_slug))
        .route("/bio/upload", post(handlers::bio::upload_image))
        .route("/bio/unsplash", get(handlers::bio::search_unsplash))
        .route("/bio/:id/edit", get(handlers::bio::edit_bio_page))
        .route("/bio/:id/analytics", get(handlers::bio::bio_analytics))
        .route("/bio/:id", post(handlers::bio::update_bio_page))
        .route("/bio/:id/delete", post(handlers::bio::delete_bio_page))
        // User management (admin only)
        .route("/users", get(handlers::users::list_users).post(handlers::users::create_user))
        .route("/users/:id/approve", post(handlers::users::approve_user))
        .route("/users/:id/role", post(handlers::users::change_role))
        .route("/users/:id/delete", post(handlers::users::delete_user))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024));

    let app = Router::new()
        .route("/", get(handlers::admin::index))
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .nest("/admin", admin_router)
        .route("/c/:id", get(handlers::redirect::bio_link_click))
        .route("/:code", get(handlers::redirect::redirect))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    // ── Serve ──────────────────────────────────────────────────────────────
    let bind_addr = format!(
        "{}:{}",
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
