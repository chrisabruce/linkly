use crate::{
    auth::AuthUser,
    config::AppConfig,
    db, db_bio,
    models::{BioPage, BioPageAnalytics, BioPageFull},
    s3 as s3_util, AppState,
};
use askama::Template;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse, Json, Redirect, Response,
    },
};
use axum_extra::extract::{
    cookie::{Cookie, SameSite},
    CookieJar,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ── Template structs ──────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "bio_list.html")]
struct BioListTemplate {
    pages: Vec<BioPage>,
    #[allow(dead_code)]
    base_url: String,
    flash_success: Option<String>,
    flash_error: Option<String>,
    is_admin: bool,
    app_title: String,
}

#[derive(Template)]
#[template(path = "bio_form.html")]
struct BioFormTemplate {
    page: Option<BioPageFull>,
    base_url: String,
    s3_enabled: bool,
    image_search_enabled: bool,
    flash_error: Option<String>,
    templates: Vec<(&'static str, &'static str)>,
    social_platforms: Vec<(&'static str, &'static str)>,
    is_admin: bool,
    app_title: String,
}

#[derive(Template)]
#[template(path = "bio_analytics.html")]
struct BioAnalyticsTemplate {
    analytics: BioPageAnalytics,
    base_url: String,
    max_link_clicks: i64,
    top_browsers: Vec<(String, i64, i64)>,
    top_devices: Vec<(String, i64, i64)>,
    top_referers: Vec<(String, i64, i64)>,
    top_countries: Vec<(String, i64, i64)>,
    is_admin: bool,
    app_title: String,
}

// ── Form types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BioPageForm {
    slug: String,
    display_name: String,
    bio: Option<String>,
    profile_image_url: Option<String>,
    background_type: Option<String>,
    background_value: Option<String>,
    template_name: String,
    custom_css: Option<String>,
    email_address: Option<String>,
    is_published: Option<String>,
    links_json: Option<String>,
    social_links_json: Option<String>,
}

#[derive(Deserialize)]
pub struct ImageSearchQuery {
    q: String,
    page: Option<u32>,
}

#[derive(Serialize)]
struct ImageResult {
    url_regular: String,
    url_small: String,
    description: String,
    author: String,
    author_url: String,
    source: &'static str,
}

#[derive(Serialize)]
struct UploadResponse {
    url: String,
}

#[derive(Deserialize)]
struct LinkEntry {
    title: String,
    url: String,
    sort_order: i64,
    is_active: bool,
}

#[derive(Deserialize)]
struct SocialEntry {
    platform: String,
    url: String,
    sort_order: i64,
}

// ── Constants ─────────────────────────────────────────────────────────────

const TEMPLATE_CHOICES: &[(&str, &str)] = &[
    ("minimal", "Minimal"),
    ("bold", "Bold"),
    ("rounded", "Rounded"),
    ("glass", "Glass"),
    ("neon", "Neon"),
    ("pebble", "Pebble"),
    ("sunset", "Sunset"),
    ("mono", "Mono"),
    ("cyber", "Cyber"),
    ("candy", "Candy"),
    ("leaf", "Leaf"),
    ("iris", "Iris"),
    ("brutalist", "Brutalist"),
];

const SOCIAL_PLATFORMS: &[(&str, &str)] = &[
    ("twitter", "Twitter / X"),
    ("instagram", "Instagram"),
    ("facebook", "Facebook"),
    ("linkedin", "LinkedIn"),
    ("github", "GitHub"),
    ("youtube", "YouTube"),
    ("tiktok", "TikTok"),
    ("twitch", "Twitch"),
    ("discord", "Discord"),
    ("mastodon", "Mastodon"),
    ("threads", "Threads"),
    ("bluesky", "Bluesky"),
    ("spotify", "Spotify"),
    ("soundcloud", "SoundCloud"),
    ("dribbble", "Dribbble"),
    ("behance", "Behance"),
    ("pinterest", "Pinterest"),
    ("snapchat", "Snapchat"),
    ("telegram", "Telegram"),
    ("whatsapp", "WhatsApp"),
];

// ── Handlers ──────────────────────────────────────────────────────────────

/// GET /admin/bio
pub async fn list_bio_pages(
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

    let user_filter = if auth.is_admin() {
        None
    } else {
        Some(auth.user_id)
    };

    let pages = match db_bio::get_all_bio_pages(&state.db, user_filter).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to load bio pages: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load bio pages",
            )
                .into_response();
        }
    };

    let tmpl = BioListTemplate {
        pages,
        base_url: state.config.base_url.clone(),
        flash_success,
        flash_error,
        is_admin: auth.is_admin(),
        app_title: state.config.app_title.clone(),
    };

    (jar.remove(clear_success).remove(clear_error), tmpl).into_response()
}

/// GET /admin/bio/new
pub async fn new_bio_page(auth: AuthUser, State(state): State<Arc<AppState>>) -> Response {
    BioFormTemplate {
        page: None,
        base_url: state.config.base_url.clone(),
        s3_enabled: state.config.s3_configured(),
        image_search_enabled: state.config.image_search_configured(),
        flash_error: None,
        templates: TEMPLATE_CHOICES.to_vec(),
        social_platforms: SOCIAL_PLATFORMS.to_vec(),
        is_admin: auth.is_admin(),
        app_title: state.config.app_title.clone(),
    }
    .into_response()
}

/// POST /admin/bio  (create)
pub async fn create_bio_page(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    axum::extract::Form(form): axum::extract::Form<BioPageForm>,
) -> Response {
    let slug = form.slug.trim().to_lowercase();
    if slug.is_empty() || !slug.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return set_flash_and_redirect(
            jar,
            None,
            Some("Slug must contain only letters, numbers, and hyphens."),
            "/admin/bio/new",
        );
    }

    // Ensure slug doesn't collide with an existing short link code
    match db::get_link_by_code(&state.db, &slug).await {
        Ok(Some(_)) => {
            return set_flash_and_redirect(
                jar,
                None,
                Some("That slug conflicts with an existing short link code."),
                "/admin/bio/new",
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(
                "DB error checking short code collision for '{}': {:?}",
                slug,
                e
            );
        }
    }

    let bio = form.bio.as_deref().unwrap_or("").trim();

    let page = match db_bio::create_bio_page(
        &state.db,
        &slug,
        form.display_name.trim(),
        bio,
        &form.template_name,
        auth.user_id,
    )
    .await
    {
        Ok(page) => page,
        Err(e) => {
            tracing::error!("Failed to create links page: {:?}", e);
            let msg = if e.to_string().contains("UNIQUE") {
                "That slug is already taken."
            } else {
                "Database error creating links page."
            };
            return set_flash_and_redirect(jar, None, Some(msg), "/admin/bio/new");
        }
    };

    let id = page.id;

    // Save all remaining fields via update
    let is_published = form.is_published.as_deref() == Some("on");
    let custom_css = form.custom_css.as_deref().unwrap_or("");
    let email = form
        .email_address
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let profile_url = form
        .profile_image_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let bg_type = form.background_type.as_deref().unwrap_or("color");
    let bg_value = form.background_value.as_deref().unwrap_or("#ffffff").trim();

    tracing::debug!(
        "Saving page {}: profile_image={:?}, bg_type={}, bg_value={}, links_json={:?}, social_json={:?}",
        id, profile_url, bg_type, bg_value, form.links_json, form.social_links_json
    );

    if let Err(e) = db_bio::update_bio_page(
        &state.db,
        id,
        &slug,
        form.display_name.trim(),
        bio,
        profile_url,
        bg_type,
        bg_value,
        &form.template_name,
        custom_css,
        email,
        is_published,
    )
    .await
    {
        tracing::error!("Failed to update links page after create {}: {:?}", id, e);
    }

    // Save links
    if let Some(ref json) = form.links_json {
        if let Ok(entries) = serde_json::from_str::<Vec<LinkEntry>>(json) {
            let tuples: Vec<(String, String, i64, bool)> = entries
                .into_iter()
                .map(|e| (e.title, e.url, e.sort_order, e.is_active))
                .collect();
            if let Err(e) = db_bio::replace_bio_links(&state.db, id, &tuples).await {
                tracing::error!("Failed to save links for page {}: {:?}", id, e);
            }
        }
    }

    // Save social links
    if let Some(ref json) = form.social_links_json {
        if let Ok(entries) = serde_json::from_str::<Vec<SocialEntry>>(json) {
            let tuples: Vec<(String, String, i64)> = entries
                .into_iter()
                .map(|e| (e.platform, e.url, e.sort_order))
                .collect();
            if let Err(e) = db_bio::replace_bio_social_links(&state.db, id, &tuples).await {
                tracing::error!("Failed to save social links for page {}: {:?}", id, e);
            }
        }
    }

    set_flash_and_redirect(
        jar,
        Some("Links page created."),
        None,
        &format!("/admin/bio/{}/edit", id),
    )
}

/// GET /admin/bio/:id/edit
pub async fn edit_bio_page(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
) -> Response {
    let flash_error = jar.get("flash_error").map(|c| c.value().to_owned());
    let clear_error = Cookie::build(("flash_error", ""))
        .path("/")
        .max_age(time::Duration::seconds(0))
        .build();

    match db_bio::get_bio_page_full(&state.db, id).await {
        Ok(Some(page_full)) => {
            // Ownership check
            if !auth.is_admin() && page_full.page.user_id != Some(auth.user_id) {
                return (StatusCode::FORBIDDEN, "Access denied").into_response();
            }

            let tmpl = BioFormTemplate {
                page: Some(page_full),
                base_url: state.config.base_url.clone(),
                s3_enabled: state.config.s3_configured(),
                image_search_enabled: state.config.image_search_configured(),
                flash_error,
                templates: TEMPLATE_CHOICES.to_vec(),
                social_platforms: SOCIAL_PLATFORMS.to_vec(),
                is_admin: auth.is_admin(),
                app_title: state.config.app_title.clone(),
            };
            (jar.remove(clear_error), tmpl).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Links page not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to load bio page {}: {:?}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

/// POST /admin/bio/:id  (update)
pub async fn update_bio_page(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
    axum::extract::Form(form): axum::extract::Form<BioPageForm>,
) -> Response {
    // Ownership check
    if let Ok(Some(page)) = db_bio::get_bio_page_by_id(&state.db, id).await {
        if !auth.is_admin() && page.user_id != Some(auth.user_id) {
            return (StatusCode::FORBIDDEN, "Access denied").into_response();
        }
    }

    let slug = form.slug.trim().to_lowercase();
    if slug.is_empty() || !slug.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return set_flash_and_redirect(
            jar,
            None,
            Some("Slug must contain only letters, numbers, and hyphens."),
            &format!("/admin/bio/{}/edit", id),
        );
    }

    // Ensure slug doesn't collide with an existing short link code
    match db::get_link_by_code(&state.db, &slug).await {
        Ok(Some(_)) => {
            return set_flash_and_redirect(
                jar,
                None,
                Some("That slug conflicts with an existing short link code."),
                &format!("/admin/bio/{}/edit", id),
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(
                "DB error checking short code collision for '{}': {:?}",
                slug,
                e
            );
        }
    }

    let is_published = form.is_published.as_deref() == Some("on");
    let custom_css = form.custom_css.as_deref().unwrap_or("");
    let email = form
        .email_address
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let profile_url = form
        .profile_image_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let bg_type = form.background_type.as_deref().unwrap_or("color");
    let bg_value = form.background_value.as_deref().unwrap_or("#ffffff").trim();
    let bio = form.bio.as_deref().unwrap_or("").trim();

    tracing::debug!(
        "Updating page {}: profile_image={:?}, bg_type={}, bg_value={}, links_json={:?}, social_json={:?}",
        id, profile_url, bg_type, bg_value, form.links_json, form.social_links_json
    );

    // Update page settings
    match db_bio::update_bio_page(
        &state.db,
        id,
        &slug,
        form.display_name.trim(),
        bio,
        profile_url,
        bg_type,
        bg_value,
        &form.template_name,
        custom_css,
        email,
        is_published,
    )
    .await
    {
        Ok(()) => {}
        Err(e) => {
            tracing::error!("Failed to update bio page {}: {:?}", id, e);
            let msg = if e.to_string().contains("UNIQUE") {
                "That slug is already taken."
            } else {
                "Database error updating links page."
            };
            return set_flash_and_redirect(
                jar,
                None,
                Some(msg),
                &format!("/admin/bio/{}/edit", id),
            );
        }
    }

    // Parse and replace links
    if let Some(ref json) = form.links_json {
        if let Ok(entries) = serde_json::from_str::<Vec<LinkEntry>>(json) {
            let tuples: Vec<(String, String, i64, bool)> = entries
                .into_iter()
                .map(|e| (e.title, e.url, e.sort_order, e.is_active))
                .collect();
            if let Err(e) = db_bio::replace_bio_links(&state.db, id, &tuples).await {
                tracing::error!("Failed to replace bio links for page {}: {:?}", id, e);
            }
        }
    }

    // Parse and replace social links
    if let Some(ref json) = form.social_links_json {
        if let Ok(entries) = serde_json::from_str::<Vec<SocialEntry>>(json) {
            let tuples: Vec<(String, String, i64)> = entries
                .into_iter()
                .map(|e| (e.platform, e.url, e.sort_order))
                .collect();
            if let Err(e) = db_bio::replace_bio_social_links(&state.db, id, &tuples).await {
                tracing::error!("Failed to replace social links for page {}: {:?}", id, e);
            }
        }
    }

    set_flash_and_redirect(
        jar,
        Some("Links page updated."),
        None,
        &format!("/admin/bio/{}/edit", id),
    )
}

/// POST /admin/bio/:id/delete
pub async fn delete_bio_page(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    jar: CookieJar,
) -> Response {
    // Ownership check
    if let Ok(Some(page)) = db_bio::get_bio_page_by_id(&state.db, id).await {
        if !auth.is_admin() && page.user_id != Some(auth.user_id) {
            return set_flash_and_redirect(jar, None, Some("Access denied."), "/admin/bio");
        }
    }

    match db_bio::delete_bio_page(&state.db, id).await {
        Ok(true) => set_flash_and_redirect(jar, Some("Links page deleted."), None, "/admin/bio"),
        Ok(false) => set_flash_and_redirect(jar, None, Some("Links page not found."), "/admin/bio"),
        Err(e) => {
            tracing::error!("Failed to delete bio page {}: {:?}", id, e);
            set_flash_and_redirect(
                jar,
                None,
                Some("Failed to delete links page."),
                "/admin/bio",
            )
        }
    }
}

/// GET /admin/bio/:id/analytics
pub async fn bio_analytics(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Response {
    let analytics = match db_bio::get_bio_page_analytics(&state.db, id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Links page not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to load analytics for bio page {}: {:?}", id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load analytics",
            )
                .into_response();
        }
    };

    // Ownership check
    if !auth.is_admin() && analytics.page.user_id != Some(auth.user_id) {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    let total_views = analytics.total_views;
    let max_link_clicks = analytics
        .link_click_counts
        .first()
        .map(|l| l.click_count)
        .unwrap_or(0);

    // Compute breakdowns from page views
    let top_browsers = with_pct(
        count_field(analytics.views.iter().map(|v| v.browser.as_deref())),
        total_views,
    );
    let top_devices = with_pct(
        count_field(analytics.views.iter().map(|v| v.device_type.as_deref())),
        total_views,
    );
    let top_referers = with_pct(
        count_field(analytics.views.iter().map(|v| v.referer.as_deref())),
        total_views,
    );
    let top_countries = with_pct(
        count_field(analytics.views.iter().map(|v| v.country.as_deref())),
        total_views,
    );

    BioAnalyticsTemplate {
        analytics,
        base_url: state.config.base_url.clone(),
        max_link_clicks,
        top_browsers,
        top_devices,
        top_referers,
        top_countries,
        is_admin: auth.is_admin(),
        app_title: state.config.app_title.clone(),
    }
    .into_response()
}

/// POST /admin/bio/upload
/// Accepts multipart form with a "file" field. Returns JSON: { "url": "..." }
pub async fn upload_image(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Response {
    if !state.config.s3_configured() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "S3 not configured"})),
        )
            .into_response();
    }

    let bucket = match s3_util::get_bucket(&state.config) {
        Some(b) => b,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to initialize S3").into_response()
        }
    };

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("file") {
            continue;
        }

        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_owned();
        let extension = match content_type.as_str() {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => {
                return (StatusCode::BAD_REQUEST, "Unsupported image type").into_response();
            }
        };

        let data = match field.bytes().await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("Failed to read upload: {:?}", e);
                return (StatusCode::BAD_REQUEST, "Failed to read file").into_response();
            }
        };

        // Limit to 5 MB
        if data.len() > 5 * 1024 * 1024 {
            return (StatusCode::BAD_REQUEST, "File too large (max 5 MB)").into_response();
        }

        tracing::info!("Uploading {} bytes ({}) to S3...", data.len(), content_type);
        match s3_util::upload_image(&bucket, &data, &content_type, extension).await {
            Ok(url) => {
                tracing::info!("S3 upload success, URL: {}", url);
                return Json(UploadResponse { url }).into_response();
            }
            Err(e) => {
                tracing::error!("S3 upload failed: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Upload failed").into_response();
            }
        }
    }

    (StatusCode::BAD_REQUEST, "No file field found").into_response()
}

/// GET /admin/bio/unsplash?q=nature&page=1
/// Kept for backward compatibility — delegates to the unified search.
pub async fn search_unsplash(
    auth: AuthUser,
    state: State<Arc<AppState>>,
    query: Query<ImageSearchQuery>,
) -> Response {
    search_images(auth, state, query).await
}

/// GET /admin/bio/search-images?q=nature&page=1
/// Searches configured image providers (Pexels + Unsplash) concurrently and returns
/// interleaved results for variety.
pub async fn search_images(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(query): Query<ImageSearchQuery>,
) -> Response {
    if !state.config.image_search_configured() {
        return (
            StatusCode::BAD_REQUEST,
            "No image search provider configured",
        )
            .into_response();
    }

    let page = query.page.unwrap_or(1);
    let has_both =
        state.config.pexels_api_key.is_some() && state.config.unsplash_access_key.is_some();
    let per_page: u32 = if has_both { 8 } else { 15 };
    let client = reqwest::Client::new();

    // Query both providers concurrently
    let (pexels, unsplash) = tokio::join!(
        fetch_pexels(&client, &state.config, &query.q, page, per_page),
        fetch_unsplash(&client, &state.config, &query.q, page, per_page),
    );

    let results = if has_both && !pexels.is_empty() && !unsplash.is_empty() {
        interleave(pexels, unsplash, 2)
    } else {
        let mut combined = pexels;
        combined.extend(unsplash);
        combined
    };

    Json(results).into_response()
}

/// Fetch image results from the Pexels API. Returns an empty vec if unconfigured or on error.
async fn fetch_pexels(
    client: &reqwest::Client,
    config: &AppConfig,
    query: &str,
    page: u32,
    per_page: u32,
) -> Vec<ImageResult> {
    let Some(key) = &config.pexels_api_key else {
        return Vec::new();
    };

    let per_page = per_page.to_string();
    let page = page.to_string();

    let r = match client
        .get("https://api.pexels.com/v1/search")
        .query(&[
            ("query", query),
            ("page", &page),
            ("per_page", &per_page),
            ("orientation", "landscape"),
        ])
        .header("Authorization", key.as_str())
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Pexels API error: {e:?}");
            return Vec::new();
        }
    };

    let body: serde_json::Value = r.json().await.unwrap_or_default();
    body["photos"]
        .as_array()
        .map(|photos| {
            photos
                .iter()
                .filter_map(|item| {
                    Some(ImageResult {
                        url_regular: item["src"]["large2x"].as_str()?.to_owned(),
                        url_small: item["src"]["medium"].as_str()?.to_owned(),
                        description: item["alt"].as_str().unwrap_or("").to_owned(),
                        author: item["photographer"]
                            .as_str()
                            .unwrap_or("Unknown")
                            .to_owned(),
                        author_url: item["photographer_url"].as_str().unwrap_or("").to_owned(),
                        source: "Pexels",
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Fetch image results from the Unsplash API. Returns an empty vec if unconfigured or on error.
async fn fetch_unsplash(
    client: &reqwest::Client,
    config: &AppConfig,
    query: &str,
    page: u32,
    per_page: u32,
) -> Vec<ImageResult> {
    let Some(key) = &config.unsplash_access_key else {
        return Vec::new();
    };

    let per_page = per_page.to_string();
    let page = page.to_string();

    let r = match client
        .get("https://api.unsplash.com/search/photos")
        .query(&[
            ("query", query),
            ("page", &page),
            ("per_page", &per_page),
            ("orientation", "landscape"),
        ])
        .header("Authorization", format!("Client-ID {key}"))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Unsplash API error: {e:?}");
            return Vec::new();
        }
    };

    let body: serde_json::Value = r.json().await.unwrap_or_default();
    body["results"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .filter_map(|item| {
                    Some(ImageResult {
                        url_regular: item["urls"]["regular"].as_str()?.to_owned(),
                        url_small: item["urls"]["small"].as_str()?.to_owned(),
                        description: item["alt_description"].as_str().unwrap_or("").to_owned(),
                        author: item["user"]["name"]
                            .as_str()
                            .unwrap_or("Unknown")
                            .to_owned(),
                        author_url: item["user"]["links"]["html"]
                            .as_str()
                            .unwrap_or("")
                            .to_owned(),
                        source: "Unsplash",
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Interleave two vectors by taking `chunk` items from each alternately.
/// Consumes both vectors without cloning.
fn interleave(a: Vec<ImageResult>, b: Vec<ImageResult>, chunk: usize) -> Vec<ImageResult> {
    let mut result = Vec::with_capacity(a.len() + b.len());
    let mut a = a.into_iter();
    let mut b = b.into_iter();
    loop {
        let before = result.len();
        result.extend(a.by_ref().take(chunk));
        result.extend(b.by_ref().take(chunk));
        if result.len() == before {
            break;
        }
    }
    result
}

// ── Datastar validation endpoints ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct DatastarQuery {
    datastar: Option<String>,
}

/// GET /admin/validate-slug
/// Returns a Datastar SSE event to patch `#slug-validation`.
pub async fn validate_slug(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<DatastarQuery>,
) -> impl IntoResponse {
    // Datastar sends signals as JSON in a single `datastar` query param:
    // ?datastar={"slug":"value","currentid":123}
    let signals = q
        .datastar
        .as_deref()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok());
    let slug = signals
        .as_ref()
        .and_then(|v| v.get("slug")?.as_str().map(String::from))
        .unwrap_or_default();
    let current_id = signals.as_ref().and_then(|v| v.get("currentid")?.as_i64());
    let slug = slug.trim().to_lowercase();
    tracing::info!("validate_slug called with: {:?}", slug);

    let icon_style = r#"position:absolute; right:0.6rem; top:50%; transform:translateY(-50%); font-size:1.1rem; pointer-events:none;"#;

    let fragment = if slug.is_empty() {
        format!(
            r#"<span id="slug-validation" style="{}"></span>"#,
            icon_style
        )
    } else if !slug.chars().all(|c| c.is_alphanumeric() || c == '-') {
        format!(
            r#"<span id="slug-validation" style="{} color:#dc2626;">&#10007;</span>"#,
            icon_style
        )
    } else if let Ok(Some(_)) = db::get_link_by_code(&state.db, &slug).await {
        format!(
            r#"<span id="slug-validation" style="{} color:#dc2626;">&#10007;</span>"#,
            icon_style
        )
    } else {
        // Check other bio page slugs (skip current page if editing)
        match db_bio::get_bio_page_by_slug(&state.db, &slug).await {
            Ok(Some(existing)) if current_id != Some(existing.id) => {
                format!(
                    r#"<span id="slug-validation" style="{} color:#dc2626;">&#10007;</span>"#,
                    icon_style
                )
            }
            Err(e) => {
                tracing::error!("DB error checking slug '{}': {:?}", slug, e);
                format!(
                    r#"<span id="slug-validation" style="{} color:#16a34a;">&#10003;</span>"#,
                    icon_style
                )
            }
            _ => {
                format!(
                    r#"<span id="slug-validation" style="{} color:#16a34a;">&#10003;</span>"#,
                    icon_style
                )
            }
        }
    };

    tracing::info!(
        "validate_slug responding with fragment: {}",
        &fragment[..fragment.len().min(80)]
    );
    datastar_patch(fragment)
}

// ── Private helpers ───────────────────────────────────────────────────────

/// Build a Datastar SSE `datastar-patch-elements` response from an HTML fragment.
/// Sends a single SSE event and closes the stream (no keep-alive).
fn datastar_patch(
    fragment: String,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let data = format!("elements {}", fragment);
    let event = Event::default().event("datastar-patch-elements").data(data);
    Sse::new(tokio_stream::once(Ok(event)))
}

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

/// Tally occurrences of each non-None value, sort descending, return top 10.
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
