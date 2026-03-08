use crate::{
    auth::{self, AuthUser},
    db,
    db_bio,
    db_users,
    models::{AnalyticsSummary, BioPageWithClicks, LinkWithStats},
    password,
    AppState,
};
use askama::Template;
use axum::{
    extract::{Form, Path, Query, State},
    response::{
        sse::{Event, Sse},
        IntoResponse, Redirect, Response,
    },
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
    app_title: String,
}

#[derive(Template)]
#[template(path = "register.html")]
struct RegisterTemplate {
    error: Option<String>,
    app_title: String,
}

#[derive(Template)]
#[template(path = "change_password.html")]
struct ChangePasswordTemplate {
    error: Option<String>,
    app_title: String,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    total_short_links: i64,
    total_short_link_clicks: i64,
    total_bio_pages: i64,
    total_bio_link_clicks: i64,
    top_short_links: Vec<LinkWithStats>,
    max_short_link_clicks: i64,
    top_bio_pages: Vec<BioPageWithClicks>,
    max_bio_page_clicks: i64,
    recent_activity: Vec<RecentActivityRow>,
    is_admin: bool,
    app_title: String,
}

/// Unified row for the "Recent Activity" table on the dashboard.
struct RecentActivityRow {
    time: String,
    is_short_link: bool,
    label: String,
    country: Option<String>,
    browser: Option<String>,
    referer: Option<String>,
}

#[derive(Template)]
#[template(path = "short_links.html")]
struct ShortLinksTemplate {
    links: Vec<LinkWithStats>,
    base_url: String,
    flash_success: Option<String>,
    flash_error: Option<String>,
    is_admin: bool,
    app_title: String,
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
    is_admin: bool,
    app_title: String,
}

// ── Form types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    email: String,
    display_name: String,
    password: String,
    password_confirm: String,
}

#[derive(Deserialize)]
pub struct ChangePasswordForm {
    new_password: String,
    new_password_confirm: String,
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
/// Redirect root visitors to the configured ROOT_REDIRECT_URL.
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
    if let Some(cookie) = jar.get("auth_token") {
        if auth::verify_jwt(cookie.value(), &state.config.jwt_secret).is_some() {
            return Redirect::to("/admin/dashboard").into_response();
        }
    }
    LoginTemplate { error: None, app_title: state.config.app_title.clone() }.into_response()
}

/// POST /admin/login
pub async fn login(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Response {
    let email = form.email.trim().to_lowercase();

    // Look up user by email
    let user = match db_users::get_user_by_email(&state.db, &email).await {
        Ok(Some(u)) => u,
        _ => {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            return LoginTemplate {
                error: Some("Invalid email or password.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response();
        }
    };

    // Verify password (blocking to avoid stalling async runtime)
    let hash = user.password_hash.clone();
    let pass = form.password.clone();
    let valid = tokio::task::spawn_blocking(move || password::verify_password(&pass, &hash))
        .await
        .unwrap_or(false);

    if !valid {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        return LoginTemplate {
            error: Some("Invalid email or password.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }

    // Check approval
    if !user.is_approved {
        return LoginTemplate {
            error: Some("Your account is pending approval by an admin.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }

    // Issue JWT
    let token = match auth::create_jwt(
        user.id,
        &user.email,
        &user.role,
        &state.config.jwt_secret,
        state.config.session_duration_hours,
        user.force_password_change,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to create JWT: {:?}", e);
            return LoginTemplate {
                error: Some("Internal error. Please try again.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response();
        }
    };

    let cookie = Cookie::build(("auth_token", token))
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
pub async fn logout(jar: CookieJar) -> Response {
    let removal = Cookie::build(("auth_token", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    (jar.add(removal), Redirect::to("/admin/login")).into_response()
}

// ── Change Password ───────────────────────────────────────────────────────

/// GET /admin/change-password
pub async fn change_password_page(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Response {
    ChangePasswordTemplate { error: None, app_title: state.config.app_title.clone() }.into_response()
}

/// POST /admin/change-password
pub async fn change_password(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<ChangePasswordForm>,
) -> Response {
    if form.new_password.len() < 8 {
        return ChangePasswordTemplate {
            error: Some("Password must be at least 8 characters.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }
    if form.new_password != form.new_password_confirm {
        return ChangePasswordTemplate {
            error: Some("Passwords do not match.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }

    // Hash new password
    let pass = form.new_password.clone();
    let hash = match tokio::task::spawn_blocking(move || password::hash_password(&pass)).await {
        Ok(Ok(h)) => h,
        _ => {
            return ChangePasswordTemplate {
                error: Some("Internal error. Please try again.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response();
        }
    };

    // Update password in DB (also clears force_password_change)
    if let Err(e) = db_users::update_user_password(&state.db, auth.user_id, &hash).await {
        tracing::error!("Failed to update password for user {}: {:?}", auth.user_id, e);
        return ChangePasswordTemplate {
            error: Some("Failed to update password.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }

    // Issue new JWT without the fpc flag
    let token = match auth::create_jwt(
        auth.user_id,
        &auth.email,
        &auth.role,
        &state.config.jwt_secret,
        state.config.session_duration_hours,
        false,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to create JWT after password change: {:?}", e);
            return ChangePasswordTemplate {
                error: Some("Password changed but failed to refresh session. Please log in again.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response();
        }
    };

    let cookie = Cookie::build(("auth_token", token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(
            state.config.session_duration_hours as i64 * 3600,
        ))
        .build();

    (jar.add(cookie), Redirect::to("/admin/dashboard")).into_response()
}

// ── Register ──────────────────────────────────────────────────────────────

/// GET /admin/register
pub async fn register_page(jar: CookieJar, State(state): State<Arc<AppState>>) -> Response {
    // If already authenticated, go to dashboard
    if let Some(cookie) = jar.get("auth_token") {
        if auth::verify_jwt(cookie.value(), &state.config.jwt_secret).is_some() {
            return Redirect::to("/admin/dashboard").into_response();
        }
    }
    RegisterTemplate { error: None, app_title: state.config.app_title.clone() }.into_response()
}

/// POST /admin/register
pub async fn register(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(form): Form<RegisterForm>,
) -> Response {
    let email = form.email.trim().to_lowercase();
    let display_name = form.display_name.trim().to_string();

    // Validation
    if email.is_empty() || !email.contains('@') {
        return RegisterTemplate {
            error: Some("Please enter a valid email address.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }
    if display_name.is_empty() {
        return RegisterTemplate {
            error: Some("Display name is required.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }
    if form.password.len() < 8 {
        return RegisterTemplate {
            error: Some("Password must be at least 8 characters.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }
    if form.password != form.password_confirm {
        return RegisterTemplate {
            error: Some("Passwords do not match.".into()),
            app_title: state.config.app_title.clone(),
        }
        .into_response();
    }

    // Check if email already exists
    match db_users::get_user_by_email(&state.db, &email).await {
        Ok(Some(_)) => {
            return RegisterTemplate {
                error: Some("An account with that email already exists.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response();
        }
        Err(e) => {
            tracing::error!("DB error checking email: {:?}", e);
            return RegisterTemplate {
                error: Some("Internal error. Please try again.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response();
        }
        Ok(None) => {}
    }

    // Hash password
    let pass = form.password.clone();
    let hash = match tokio::task::spawn_blocking(move || password::hash_password(&pass)).await {
        Ok(Ok(h)) => h,
        _ => {
            return RegisterTemplate {
                error: Some("Internal error. Please try again.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response();
        }
    };

    // If no users exist, first user becomes admin + auto-approved
    let user_count = db_users::count_users(&state.db).await.unwrap_or(1);
    let (role, is_approved) = if user_count == 0 {
        ("admin", true)
    } else {
        ("user", false)
    };

    match db_users::create_user(&state.db, &email, &display_name, &hash, role, is_approved, false).await {
        Ok(user) => {
            if is_approved {
                // Auto-login for first user (admin)
                if let Ok(token) = auth::create_jwt(
                    user.id,
                    &user.email,
                    &user.role,
                    &state.config.jwt_secret,
                    state.config.session_duration_hours,
                    false,
                ) {
                    let cookie = Cookie::build(("auth_token", token))
                        .path("/")
                        .http_only(true)
                        .same_site(SameSite::Lax)
                        .max_age(time::Duration::seconds(
                            state.config.session_duration_hours as i64 * 3600,
                        ))
                        .build();
                    return (jar.add(cookie), Redirect::to("/admin/dashboard")).into_response();
                }
            }
            // Normal user — show success message on login page
            LoginTemplate {
                error: Some("Account created! An admin must approve your account before you can log in.".into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create user: {:?}", e);
            let msg = if e.to_string().contains("UNIQUE") {
                "An account with that email already exists."
            } else {
                "Failed to create account. Please try again."
            };
            RegisterTemplate {
                error: Some(msg.into()),
                app_title: state.config.app_title.clone(),
            }
            .into_response()
        }
    }
}

// ── Dashboard (analytics overview) ─────────────────────────────────────────

/// GET /admin/dashboard
pub async fn dashboard(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Response {
    let user_filter = if auth.is_admin() { None } else { Some(auth.user_id) };

    let total_short_links = db::count_links(&state.db, user_filter).await.unwrap_or(0);
    let total_short_link_clicks = db::count_total_clicks(&state.db, user_filter).await.unwrap_or(0);
    let total_bio_pages = db_bio::count_bio_pages(&state.db, user_filter).await.unwrap_or(0);
    let total_bio_link_clicks = db_bio::count_total_bio_link_clicks(&state.db, user_filter).await.unwrap_or(0);

    let top_short_links = db::top_links_by_clicks(&state.db, 10, user_filter).await.unwrap_or_default();
    let top_bio_pages = db_bio::top_bio_pages_by_clicks(&state.db, 10, user_filter).await.unwrap_or_default();

    let recent_short = db::recent_clicks_with_labels(&state.db, 20, user_filter).await.unwrap_or_default();
    let recent_bio = db_bio::recent_bio_link_clicks(&state.db, 20, user_filter).await.unwrap_or_default();

    // Merge recent activity into a single sorted list
    let mut recent_activity: Vec<RecentActivityRow> = Vec::new();

    for (label, clicked_at, country, browser, referer) in &recent_short {
        recent_activity.push(RecentActivityRow {
            time: clicked_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            is_short_link: true,
            label: label.clone(),
            country: country.clone(),
            browser: browser.clone(),
            referer: referer.clone(),
        });
    }

    for click in &recent_bio {
        recent_activity.push(RecentActivityRow {
            time: click.clicked_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            is_short_link: false,
            label: format!("{} ({})", click.link_title, click.page_slug),
            country: click.country.clone(),
            browser: click.browser.clone(),
            referer: click.referer.clone(),
        });
    }

    // Sort descending by time
    recent_activity.sort_by(|a, b| b.time.cmp(&a.time));
    recent_activity.truncate(30);

    let max_short_link_clicks = top_short_links.first().map(|l| l.click_count).unwrap_or(0);
    let max_bio_page_clicks = top_bio_pages.first().map(|p| p.click_count).unwrap_or(0);

    DashboardTemplate {
        total_short_links,
        total_short_link_clicks,
        total_bio_pages,
        total_bio_link_clicks,
        top_short_links,
        max_short_link_clicks,
        top_bio_pages,
        max_bio_page_clicks,
        recent_activity,
        is_admin: auth.is_admin(),
        app_title: state.config.app_title.clone(),
    }
    .into_response()
}

// ── Short Links ───────────────────────────────────────────────────────────

/// GET /admin/short-links
pub async fn short_links(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Response {
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

    let user_filter = if auth.is_admin() { None } else { Some(auth.user_id) };

    let links = match db::get_all_links_with_stats(&state.db, user_filter).await {
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

    let tmpl = ShortLinksTemplate {
        links,
        base_url: state.config.base_url.clone(),
        flash_success,
        flash_error,
        is_admin: auth.is_admin(),
        app_title: state.config.app_title.clone(),
    };

    (jar.remove(clear_success).remove(clear_error), tmpl).into_response()
}

// ── Create link ────────────────────────────────────────────────────────────

/// POST /admin/links
pub async fn create_link(
    auth: AuthUser,
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
            "/admin/short-links",
        );
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return set_flash_and_redirect(
            jar,
            None,
            Some("URL must start with http:// or https://"),
            "/admin/short-links",
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
                    "/admin/short-links",
                );
            }
            // Ensure custom code doesn't collide with a bio page slug
            match db_bio::bio_slug_exists(&state.db, code).await {
                Ok(true) => {
                    return set_flash_and_redirect(
                        jar,
                        None,
                        Some("That code conflicts with an existing links page slug."),
                        "/admin/short-links",
                    );
                }
                Ok(false) => {}
                Err(e) => {
                    tracing::error!("DB error checking bio slug collision for '{}': {:?}", code, e);
                }
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
        auth.user_id,
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
                "/admin/short-links",
            )
        }
        Err(e) => {
            tracing::error!("Failed to create link: {:?}", e);
            let msg = if e.to_string().contains("UNIQUE") {
                "That short code is already taken. Try another.".to_owned()
            } else {
                format!("Database error: {e}")
            };
            set_flash_and_redirect(jar, None, Some(&msg), "/admin/short-links")
        }
    }
}

// ── Delete link ────────────────────────────────────────────────────────────

/// POST /admin/links/:id/delete
pub async fn delete_link(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(id): Path<i64>,
) -> Response {
    // Fetch the link first so we can check ownership and evict from cache
    let link = match db::get_link_by_id(&state.db, id).await {
        Ok(Some(l)) => l,
        Ok(None) => {
            return set_flash_and_redirect(jar, None, Some("Link not found."), "/admin/short-links");
        }
        Err(e) => {
            tracing::error!("Failed to fetch link {}: {:?}", id, e);
            return set_flash_and_redirect(
                jar,
                None,
                Some("Database error while looking up link."),
                "/admin/short-links",
            );
        }
    };

    // Ownership check: non-admins can only delete their own links
    if !auth.is_admin() && link.user_id != Some(auth.user_id) {
        return set_flash_and_redirect(jar, None, Some("Access denied."), "/admin/short-links");
    }

    match db::delete_link(&state.db, id).await {
        Ok(true) => {
            state.cache.remove(&link.short_code);
            set_flash_and_redirect(
                jar,
                Some(&format!("Link '{}' deleted.", link.short_code)),
                None,
                "/admin/short-links",
            )
        }
        Ok(false) => set_flash_and_redirect(jar, None, Some("Link not found."), "/admin/short-links"),
        Err(e) => {
            tracing::error!("Failed to delete link {}: {:?}", id, e);
            set_flash_and_redirect(
                jar,
                None,
                Some("Failed to delete link."),
                "/admin/short-links",
            )
        }
    }
}

// ── Analytics ──────────────────────────────────────────────────────────────

/// GET /admin/links/:id/analytics
pub async fn analytics(
    auth: AuthUser,
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

    // Ownership check
    if !auth.is_admin() && summary.link.user_id != Some(auth.user_id) {
        return (axum::http::StatusCode::FORBIDDEN, "Access denied.").into_response();
    }

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
        is_admin: auth.is_admin(),
        app_title: state.config.app_title.clone(),
    }
    .into_response()
}

// ── Datastar validation endpoints ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct DatastarQuery {
    datastar: Option<String>,
}

/// GET /admin/validate-code
/// Returns a Datastar SSE event to patch `#code-validation`.
pub async fn validate_code(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DatastarQuery>,
) -> impl IntoResponse {
    // Datastar sends signals as JSON in a single `datastar` query param:
    // ?datastar={"customcode":"value"}
    let code = q.datastar
        .as_deref()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .and_then(|v| v.get("customcode")?.as_str().map(String::from))
        .unwrap_or_default();
    let code = code.trim();
    tracing::info!("validate_code called with: {:?}", code);

    let fragment = if code.is_empty() {
        r#"<span id="code-validation" style="position:absolute; right:0.6rem; top:50%; transform:translateY(-50%); font-size:1.1rem; pointer-events:none;"></span>"#.to_string()
    } else if !code.chars().all(|c| c.is_alphanumeric() || c == '-') {
        r#"<span id="code-validation" style="position:absolute; right:0.6rem; top:50%; transform:translateY(-50%); font-size:1.1rem; pointer-events:none; color:#dc2626;">&#10007;</span>"#.to_string()
    } else if let Ok(Some(_)) = db::get_link_by_code(&state.db, code).await {
        r#"<span id="code-validation" style="position:absolute; right:0.6rem; top:50%; transform:translateY(-50%); font-size:1.1rem; pointer-events:none; color:#dc2626;">&#10007;</span>"#.to_string()
    } else if let Ok(true) = db_bio::bio_slug_exists(&state.db, code).await {
        r#"<span id="code-validation" style="position:absolute; right:0.6rem; top:50%; transform:translateY(-50%); font-size:1.1rem; pointer-events:none; color:#dc2626;">&#10007;</span>"#.to_string()
    } else {
        r#"<span id="code-validation" style="position:absolute; right:0.6rem; top:50%; transform:translateY(-50%); font-size:1.1rem; pointer-events:none; color:#16a34a;">&#10003;</span>"#.to_string()
    };

    tracing::info!("validate_code responding with fragment: {}", &fragment[..fragment.len().min(80)]);
    datastar_patch(fragment)
}

// ── Private helpers ────────────────────────────────────────────────────────

/// Build a Datastar SSE `datastar-patch-elements` response from an HTML fragment.
/// Sends a single SSE event and closes the stream (no keep-alive).
fn datastar_patch(fragment: String) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let data = format!("elements {}", fragment);
    let event = Event::default()
        .event("datastar-patch-elements")
        .data(data);
    Sse::new(tokio_stream::once(Ok(event)))
}

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
/// exist in the database.
async fn generate_unique_code(pool: &sqlx::SqlitePool) -> String {
    for _ in 0..10 {
        let code = random_code(7);
        match db::get_link_by_code(pool, &code).await {
            Ok(None) => return code,
            _ => continue,
        }
    }
    random_code(9)
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
