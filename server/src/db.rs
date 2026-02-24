use crate::{
    cache::LinkCache,
    models::{AnalyticsSummary, Click, Link, LinkWithStats},
};
use sqlx::SqlitePool;

// ── Warm-up ────────────────────────────────────────────────────────────────

/// Load every active link into the in-memory cache at startup.
pub async fn warm_cache(pool: &SqlitePool, cache: &LinkCache) -> anyhow::Result<()> {
    let links: Vec<Link> = sqlx::query_as(
        "SELECT id, short_code, original_url, title, description, created_at, is_active
         FROM links WHERE is_active = 1",
    )
    .fetch_all(pool)
    .await?;

    let count = links.len();
    for link in links {
        cache.set(link.short_code, link.original_url);
    }

    tracing::info!("Cache warmed with {} active link(s)", count);
    Ok(())
}

// ── Links ──────────────────────────────────────────────────────────────────

/// Insert a new link and return the newly created row.
pub async fn create_link(
    pool: &SqlitePool,
    short_code: &str,
    original_url: &str,
    title: Option<&str>,
    description: Option<&str>,
) -> Result<Link, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO links (short_code, original_url, title, description) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(short_code)
    .bind(original_url)
    .bind(title)
    .bind(description)
    .execute(pool)
    .await?
    .last_insert_rowid();

    let link: Link = sqlx::query_as(
        "SELECT id, short_code, original_url, title, description, created_at, is_active
         FROM links WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;

    Ok(link)
}

/// Fetch a single active link by its short code.
pub async fn get_link_by_code(
    pool: &SqlitePool,
    short_code: &str,
) -> Result<Option<Link>, sqlx::Error> {
    let link: Option<Link> = sqlx::query_as(
        "SELECT id, short_code, original_url, title, description, created_at, is_active
         FROM links WHERE short_code = ?1 AND is_active = 1",
    )
    .bind(short_code)
    .fetch_optional(pool)
    .await?;

    Ok(link)
}

/// Return all links joined with their total click counts, newest first.
pub async fn get_all_links_with_stats(
    pool: &SqlitePool,
) -> Result<Vec<LinkWithStats>, sqlx::Error> {
    let rows: Vec<(
        i64,
        String,
        String,
        Option<String>,
        Option<String>,
        chrono::NaiveDateTime,
        bool,
        i64,
    )> = sqlx::query_as(
        "SELECT l.id,
                    l.short_code,
                    l.original_url,
                    l.title,
                    l.description,
                    l.created_at,
                    l.is_active,
                    COUNT(c.id) as click_count
             FROM links l
             LEFT JOIN clicks c ON c.link_id = l.id
             GROUP BY l.id
             ORDER BY l.created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    let result = rows
        .into_iter()
        .map(
            |(
                id,
                short_code,
                original_url,
                title,
                description,
                created_at,
                is_active,
                click_count,
            )| {
                LinkWithStats {
                    id,
                    short_code,
                    original_url,
                    title,
                    description,
                    created_at,
                    is_active,
                    click_count,
                }
            },
        )
        .collect();

    Ok(result)
}

/// Fetch a single link by its primary key (any status).
pub async fn get_link_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Link>, sqlx::Error> {
    let link: Option<Link> = sqlx::query_as(
        "SELECT id, short_code, original_url, title, description, created_at, is_active
         FROM links WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(link)
}

/// Permanently delete a link (cascades to clicks via FK).
pub async fn delete_link(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM links WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();

    Ok(affected > 0)
}

// ── Clicks ─────────────────────────────────────────────────────────────────

/// Record a click event. Designed to be called from a spawned background task
/// so that the HTTP redirect is never blocked by the analytics write.
#[allow(clippy::too_many_arguments)]
pub async fn log_click(
    pool: &SqlitePool,
    link_id: i64,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    referer: Option<&str>,
    browser: Option<&str>,
    os: Option<&str>,
    device_type: Option<&str>,
    country: Option<&str>,
    region: Option<&str>,
    city: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO clicks
             (link_id, ip_address, user_agent, referer, browser, os, device_type,
              country, region, city)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(link_id)
    .bind(ip_address)
    .bind(user_agent)
    .bind(referer)
    .bind(browser)
    .bind(os)
    .bind(device_type)
    .bind(country)
    .bind(region)
    .bind(city)
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetch full analytics for one link: the link row, aggregate counts, and
/// the 500 most-recent individual click events.
pub async fn get_analytics(
    pool: &SqlitePool,
    link_id: i64,
) -> Result<Option<AnalyticsSummary>, sqlx::Error> {
    let link = match get_link_by_id(pool, link_id).await? {
        Some(l) => l,
        None => return Ok(None),
    };

    let total_clicks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM clicks WHERE link_id = ?1")
        .bind(link_id)
        .fetch_one(pool)
        .await?;

    let unique_ips: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT ip_address) FROM clicks
         WHERE link_id = ?1 AND ip_address IS NOT NULL",
    )
    .bind(link_id)
    .fetch_one(pool)
    .await?;

    let clicks: Vec<Click> = sqlx::query_as(
        "SELECT id, link_id, clicked_at, ip_address, user_agent,
                referer, browser, os, device_type, country, region, city
         FROM clicks
         WHERE link_id = ?1
         ORDER BY clicked_at DESC
         LIMIT 500",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(AnalyticsSummary {
        link,
        total_clicks,
        unique_ips,
        clicks,
    }))
}
