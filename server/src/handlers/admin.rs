use crate::{
    auth::AuthUser,
    db,
    models::{AnalyticsSummary, LinkWithStats},
    AppState,
};
use askama::Template;
use axum::{
    extract::{Form, Path, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{
    cookie::{Cookie, SameSite},
    CookieJar,
};
use serde::Deserialize;
use std::sync::Arc;

// ── Template structs ───────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    links: Vec<LinkWithStats>,
    base_url: String,
    flash_success: Option<String>,
    flash_error: Option<String>,
}

#[derive(Template)]
#[template(path = "analytics.html")]
struct AnalyticsTemplate {
    summary: AnalyticsSummary,
    short_url: String,
    // Pre-computed breakdowns: (name, count, pct_of_total)
    top_browsers: Vec<(String, i64, i64)>,
    top_os: Vec<(String, i64, i64)>,
    top_devices: Vec<(String, i64, i64)>,
    top_referers: Vec<(String, i64, i64)>,
    top_countries: Vec<(String, i64, i64)>,
}

// ── Form types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
}

#[derive(Deserialize)]
pub struct CreateLinkForm {
    url: String,
    title: Option<String>,
    description: Option<String>,
    custom_code: Option<String>,
}

// ── Handlers ───────────────────────────────────────────────────────────────

/// GET /
/// Redirect root visitors to the configured ROOT_REDIRECT_URL (e.g. the
/// studio's public website).  Admins must navigate directly to /admin.
pub async fn index(State(state): State<Arc<AppState>>) -> Redirect {
    Redirect::to(&state.config.root_redirect_url)
}

/// GET /admin
/// Redirect /admin to /admin/dashboard.
pub async fn admin_index() -> Redirect {
    Redirect::to("/admin/dashboard")
}

// ── Login / Logout ─────────────────────────────────────────────────────────

/// GET /admin/login
pub async fn login_page(jar: CookieJar, State(state): State<Arc<AppState>>) -> Response {
    // If already authenticated, skip the login page.
    if let Some(cookie) = jar.get("session_id") {
        if state.sessions.is_valid(cookie.value()).await {
            return Redirect::to("/admin/dashboard").into_response();
        }
    }
    LoginTemplate { error: None }.into_response()
}

/// POST /admin/login
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    if form.password != state.config.admin_password {
        // Use a small artificial delay to blunt brute-force attempts.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        return LoginTemplate {
            error: Some("Incorrect password.".into()),
        }
        .into_response();
    }

    let token = state.sessions.create().await;

    let cookie = Cookie::build(("session_id", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(
            state.config.session_duration_hours as i64 * 3600,
        ))
        .build();

    (jar.add(cookie), Redirect::to("/admin/dashboard")).into_response()
}

/// GET /admin/logout
pub async fn logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> Response {
    if let Some(cookie) = jar.get("session_id") {
        state.sessions.remove(cookie.value()).await;
    }

    let removal = Cookie::build(("session_id", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    (jar.add(removal), Redirect::to("/admin/login")).into_response()
}

// ── Dashboard ──────────────────────────────────────────────────────────────

/// GET /admin/dashboard
pub async fn dashboard(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Response {
    // Read and clear flash cookies
    let flash_success = jar.get("flash_success").map(|c| c.value().to_owned());
    let flash_error = jar.get("flash_error").map(|c| c.value().to_owned());

    let clear_success = Cookie::build(("flash_success", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();
    let clear_error = Cookie::build(("flash_error", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    let links = match db::get_all_links_with_stats(&state.db).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("Failed to load links: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load links",
            )
                .into_response();
        }
    };

    let tmpl = DashboardTemplate {
        links,
        base_url: state.config.base_url.clone(),
        flash_success,
        flash_error,
    };

    (jar.remove(clear_success).remove(clear_error), tmpl).into_response()
}

// ── Create link ────────────────────────────────────────────────────────────

/// POST /admin/links
pub async fn create_link(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<CreateLinkForm>,
) -> Response {
    // Basic URL validation
    let url = form.url.trim().to_owned();
    if url.is_empty() {
        return set_flash_and_redirect(
            jar,
            None,
            Some("URL must not be empty."),
            "/admin/dashboard",
        );
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return set_flash_and_redirect(
            jar,
            None,
            Some("URL must start with http:// or https://"),
            "/admin/dashboard",
        );
    }

    // Determine the short code to use
    let short_code = match form
        .custom_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(code) => {
            // Validate custom code: alphanumeric + hyphens only
            if !code.chars().all(|c| c.is_alphanumeric() || c == '-') {
                return set_flash_and_redirect(
                    jar,
                    None,
                    Some("Custom code may only contain letters, numbers, and hyphens."),
                    "/admin/dashboard",
                );
            }
            code.to_owned()
        }
        None => generate_unique_code(&state.db).await,
    };

    let title = form
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    let description = form
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    match db::create_link(
        &state.db,
        &short_code,
        &url,
        title.as_deref(),
        description.as_deref(),
    )
    .await
    {
        Ok(link) => {
            // Update the cache immediately
            state.cache.set(&link.short_code, &link.original_url);
            set_flash_and_redirect(
                jar,
                Some(&format!(
                    "Link created: {}/{}",
                    state.config.base_url, link.short_code
                )),
                None,
                "/admin/dashboard",
            )
        }
        Err(e) => {
            tracing::error!("Failed to create link: {:?}", e);
            let msg = if e.to_string().contains("UNIQUE") {
                "That short code is already taken. Try another.".to_owned()
            } else {
                format!("Database error: {e}")
            };
            set_flash_and_redirect(jar, None, Some(&msg), "/admin/dashboard")
        }
    }
}

// ── Delete link ────────────────────────────────────────────────────────────

/// POST /admin/links/:id/delete
pub async fn delete_link(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<i64>,
) -> Response {
    // Fetch the link first so we can evict it from the cache
    let link = match db::get_link_by_id(&state.db, id).await {
        Ok(Some(l)) => l,
        Ok(None) => {
            return set_flash_and_redirect(jar, None, Some("Link not found."), "/admin/dashboard");
        }
        Err(e) => {
            tracing::error!("Failed to fetch link {}: {:?}", id, e);
            return set_flash_and_redirect(
                jar,
                None,
                Some("Database error while looking up link."),
                "/admin/dashboard",
            );
        }
    };

    match db::delete_link(&state.db, id).await {
        Ok(true) => {
            state.cache.remove(&link.short_code);
            set_flash_and_redirect(
                jar,
                Some(&format!("Link '{}' deleted.", link.short_code)),
                None,
                "/admin/dashboard",
            )
        }
        Ok(false) => set_flash_and_redirect(jar, None, Some("Link not found."), "/admin/dashboard"),
        Err(e) => {
            tracing::error!("Failed to delete link {}: {:?}", id, e);
            set_flash_and_redirect(
                jar,
                None,
                Some("Failed to delete link."),
                "/admin/dashboard",
            )
        }
    }
}

// ── Analytics ──────────────────────────────────────────────────────────────

/// GET /admin/links/:id/analytics
pub async fn analytics(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Response {
    let summary = match db::get_analytics(&state.db, id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return (axum::http::StatusCode::NOT_FOUND, "Link not found.").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to load analytics for link {}: {:?}", id, e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load analytics.",
            )
                .into_response();
        }
    };

    let short_url = format!("{}/{}", state.config.base_url, summary.link.short_code);

    let total = summary.total_clicks;
    let top_browsers = with_pct(
        count_field(summary.clicks.iter().map(|c| c.browser.as_deref())),
        total,
    );
    let top_os = with_pct(
        count_field(summary.clicks.iter().map(|c| c.os.as_deref())),
        total,
    );
    let top_devices = with_pct(
        count_field(summary.clicks.iter().map(|c| c.device_type.as_deref())),
        total,
    );
    let top_referers = with_pct(
        count_field(summary.clicks.iter().map(|c| c.referer.as_deref())),
        total,
    );
    let top_countries = with_pct(
        count_field(summary.clicks.iter().map(|c| c.country.as_deref())),
        total,
    );

    AnalyticsTemplate {
        summary,
        short_url,
        top_browsers,
        top_os,
        top_devices,
        top_referers,
        top_countries,
    }
    .into_response()
}

// ── Private helpers ────────────────────────────────────────────────────────

/// Set a flash cookie and redirect to the given path.
fn set_flash_and_redirect(
    jar: CookieJar,
    success: Option<&str>,
    error: Option<&str>,
    destination: &str,
) -> Response {
    let mut jar = jar;

    if let Some(msg) = success {
        let c = Cookie::build(("flash_success", msg.to_owned()))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::seconds(30))
            .build();
        jar = jar.add(c);
    }

    if let Some(msg) = error {
        let c = Cookie::build(("flash_error", msg.to_owned()))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(time::Duration::seconds(30))
            .build();
        jar = jar.add(c);
    }

    (jar, Redirect::to(destination)).into_response()
}

/// Generate a random 7-character alphanumeric short code that doesn't already
/// exist in the database.  Tries up to 10 times before giving up and returning
/// whatever was last generated (the UNIQUE constraint in the DB is the real
/// guard).
async fn generate_unique_code(pool: &sqlx::SqlitePool) -> String {
    for _ in 0..10 {
        let code = random_code(7);
        match db::get_link_by_code(pool, &code).await {
            Ok(None) => return code,
            _ => continue,
        }
    }
    random_code(9) // fallback: longer code is even less likely to collide
}

/// Generate a random alphanumeric string of the given length.
fn random_code(len: usize) -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Tally occurrences of each non-None value, sort descending by count, and
/// return the top 10.
fn count_field<'a>(iter: impl Iterator<Item = Option<&'a str>>) -> Vec<(String, i64)> {
    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for val in iter.flatten() {
        if !val.is_empty() {
            *counts.entry(val.to_owned()).or_insert(0) += 1;
        }
    }
    let mut sorted: Vec<(String, i64)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.truncate(10);
    sorted
}

/// Attach a percentage-of-total column to each breakdown row.
fn with_pct(items: Vec<(String, i64)>, total: i64) -> Vec<(String, i64, i64)> {
    items
        .into_iter()
        .map(|(name, count)| {
            let pct = if total > 0 { count * 100 / total } else { 0 };
            (name, count, pct)
        })
        .collect()
}
