use crate::{db, db_bio, geo, models::BioLink, models::BioPageFull, AppState};
use askama::Template;
use axum::{
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use std::{net::SocketAddr, sync::Arc};
use woothee::parser::Parser;

#[derive(Template)]
#[template(path = "bio_page.html")]
struct BioPageTemplate {
    page: BioPageFull,
    active_links: Vec<BioLink>,
}

/// GET /:code
///
/// 1. Check if the code matches a published bio page slug — if so, render it.
/// 2. Otherwise check the in-memory cache for a short code (fast path).
/// 3. On a cache miss, fall back to the database for short links.
/// 4. Spawn a background task to record the click.
/// 5. Return a 302 redirect to the original URL.
pub async fn redirect(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    // ── 1. Check for a published bio page ────────────────────────────────
    match db_bio::get_published_bio_page_full(&state.db, &code).await {
        Ok(Some(page_full)) => {
            let active_links: Vec<BioLink> = page_full
                .links
                .iter()
                .filter(|l| l.is_active)
                .cloned()
                .collect();

            // Log the page view in the background
            let page_id = page_full.page.id;
            let ip = extract_ip(&headers, addr);
            let user_agent = headers
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let referer = headers
                .get("referer")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let (browser, os, device_type) = parse_user_agent(user_agent.as_deref());

            let state_bg = state.clone();
            tokio::spawn(async move {
                let (country, region, city) = if let Some(ref ip_str) = ip {
                    match geo::lookup(ip_str, &state_bg.geo_cache).await {
                        Some(info) => (Some(info.country), Some(info.region), Some(info.city)),
                        None => (None, None, None),
                    }
                } else {
                    (None, None, None)
                };

                let _ = db_bio::log_bio_page_view(
                    &state_bg.db,
                    page_id,
                    ip.as_deref(),
                    user_agent.as_deref(),
                    referer.as_deref(),
                    browser.as_deref(),
                    os.as_deref(),
                    device_type.as_deref(),
                    country.as_deref(),
                    region.as_deref(),
                    city.as_deref(),
                )
                .await;
            });

            return BioPageTemplate {
                page: page_full,
                active_links,
            }
            .into_response();
        }
        Ok(None) => {} // Not a bio page — continue to short link lookup
        Err(e) => {
            tracing::error!("DB error checking bio page '{}': {:?}", code, e);
            // Don't fail hard — fall through to short link lookup
        }
    }

    // ── 2. Resolve short link URL ────────────────────────────────────────
    let original_url = match state.cache.get(&code) {
        Some(url) => url,
        None => {
            // Cache miss — check the database
            match db::get_link_by_code(&state.db, &code).await {
                Ok(Some(link)) => {
                    // Backfill the cache for next time
                    state.cache.set(&link.short_code, &link.original_url);
                    link.original_url
                }
                Ok(None) => {
                    return (StatusCode::NOT_FOUND, "Not found").into_response();
                }
                Err(e) => {
                    tracing::error!("DB error looking up short code '{}': {:?}", code, e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
                }
            }
        }
    };

    // ── 3. Extract request metadata ────────────────────────────────────────
    let ip = extract_ip(&headers, addr);

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let referer = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // Parse the User-Agent string for browser / OS / device info
    let (browser, os, device_type) = parse_user_agent(user_agent.as_deref());

    // ── 4. Log the click in the background ─────────────────────────────────
    // Clone everything needed so the background task owns its data.
    // The geo lookup and DB write both happen here — never on the hot path.
    let state_bg = state.clone();
    let code_bg = code.clone();
    let ip_bg = ip.clone();
    let ua_bg = user_agent.clone();
    let ref_bg = referer.clone();
    let browser_bg = browser.clone();
    let os_bg = os.clone();
    let device_bg = device_type.clone();

    tokio::spawn(async move {
        // Resolve the link_id (needed for the INSERT into clicks).
        let link = match db::get_link_by_code(&state_bg.db, &code_bg).await {
            Ok(Some(l)) => l,
            Ok(None) => {
                tracing::warn!(
                    "Click logging: link '{}' disappeared between redirect and log",
                    code_bg
                );
                return;
            }
            Err(e) => {
                tracing::error!("Click logging DB error for '{}': {:?}", code_bg, e);
                return;
            }
        };

        // Geo-lookup: consults the in-memory cache first so that repeated
        // clicks from the same IP never trigger more than one network request.
        let (country, region, city) = if let Some(ref ip_str) = ip_bg {
            match geo::lookup(ip_str, &state_bg.geo_cache).await {
                Some(info) => (Some(info.country), Some(info.region), Some(info.city)),
                None => (None, None, None),
            }
        } else {
            (None, None, None)
        };

        let _ = db::log_click(
            &state_bg.db,
            link.id,
            ip_bg.as_deref(),
            ua_bg.as_deref(),
            ref_bg.as_deref(),
            browser_bg.as_deref(),
            os_bg.as_deref(),
            device_bg.as_deref(),
            country.as_deref(),
            region.as_deref(),
            city.as_deref(),
        )
        .await;
    });

    // ── 5. Redirect ────────────────────────────────────────────────────────
    Redirect::to(&original_url).into_response()
}

/// GET /c/:id
///
/// Track a click on a bio page link, then redirect to the actual URL.
pub async fn bio_link_click(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    let bio_link = match db_bio::get_bio_link_by_id(&state.db, id).await {
        Ok(Some(link)) => link,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Link not found").into_response();
        }
        Err(e) => {
            tracing::error!("DB error looking up bio link {}: {:?}", id, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    };

    let destination = bio_link.url.clone();
    let page_id = bio_link.page_id;

    let ip = extract_ip(&headers, addr);
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let referer = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let (browser, os, device_type) = parse_user_agent(user_agent.as_deref());

    let state_bg = state.clone();
    let ip_bg = ip;
    let ua_bg = user_agent;
    let ref_bg = referer;
    let browser_bg = browser;
    let os_bg = os;
    let device_bg = device_type;

    tokio::spawn(async move {
        let (country, region, city) = if let Some(ref ip_str) = ip_bg {
            match geo::lookup(ip_str, &state_bg.geo_cache).await {
                Some(info) => (Some(info.country), Some(info.region), Some(info.city)),
                None => (None, None, None),
            }
        } else {
            (None, None, None)
        };

        let _ = db_bio::log_bio_link_click(
            &state_bg.db,
            id,
            page_id,
            ip_bg.as_deref(),
            ua_bg.as_deref(),
            ref_bg.as_deref(),
            browser_bg.as_deref(),
            os_bg.as_deref(),
            device_bg.as_deref(),
            country.as_deref(),
            region.as_deref(),
            city.as_deref(),
        )
        .await;
    });

    Redirect::to(&destination).into_response()
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Determine the real client IP, preferring common proxy headers.
fn extract_ip(headers: &HeaderMap, addr: SocketAddr) -> Option<String> {
    // X-Forwarded-For can be a comma-separated list; take the first entry.
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = xff.split(',').next().map(str::trim) {
            if !ip.is_empty() {
                return Some(ip.to_owned());
            }
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        if !real_ip.is_empty() {
            return Some(real_ip.to_owned());
        }
    }

    Some(addr.ip().to_string())
}

/// Parse a User-Agent string using woothee and return
/// `(browser_name, os_name, device_category)`.
fn parse_user_agent(ua: Option<&str>) -> (Option<String>, Option<String>, Option<String>) {
    let ua = match ua {
        Some(s) if !s.is_empty() => s,
        _ => return (None, None, None),
    };

    let parser = Parser::new();
    match parser.parse(ua) {
        Some(result) => {
            let browser = if result.name.is_empty() || result.name == "UNKNOWN" {
                None
            } else {
                Some(result.name.to_owned())
            };

            let os = if result.os.is_empty() || result.os == "UNKNOWN" {
                None
            } else {
                Some(result.os.to_owned())
            };

            let device = if result.category.is_empty() || result.category == "UNKNOWN" {
                None
            } else {
                Some(result.category.to_owned())
            };

            (browser, os, device)
        }
        None => (None, None, None),
    }
}
