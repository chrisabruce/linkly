use crate::{
    cache::LinkCache,
    models::{AnalyticsSummary, Click, Link, LinkWithStats},
};
use chrono::NaiveDateTime;
use sqlx::SqlitePool;

type LinkStatsRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    NaiveDateTime,
    bool,
    i64,
    Option<i64>,
);

type ClickActivityRow = (
    Option<String>,
    String,
    NaiveDateTime,
    Option<String>,
    Option<String>,
    Option<String>,
);

const LINK_COLUMNS: &str =
    "id, short_code, original_url, title, description, created_at, is_active, user_id";

// ── Warm-up ────────────────────────────────────────────────────────────────

/// Load every active link into the in-memory cache at startup.
pub async fn warm_cache(pool: &SqlitePool, cache: &LinkCache) -> anyhow::Result<()> {
    let links: Vec<Link> = sqlx::query_as(&format!(
        "SELECT {LINK_COLUMNS} FROM links WHERE is_active = 1"
    ))
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
    user_id: i64,
) -> Result<Link, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO links (short_code, original_url, title, description, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(short_code)
    .bind(original_url)
    .bind(title)
    .bind(description)
    .bind(user_id)
    .execute(pool)
    .await?
    .last_insert_rowid();

    let link: Link = sqlx::query_as(&format!("SELECT {LINK_COLUMNS} FROM links WHERE id = ?1"))
        .bind(id)
        .fetch_one(pool)
        .await?;

    Ok(link)
}

/// Fetch a single active link by its short code (for public redirect, no user scoping).
pub async fn get_link_by_code(
    pool: &SqlitePool,
    short_code: &str,
) -> Result<Option<Link>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {LINK_COLUMNS} FROM links WHERE short_code = ?1 AND is_active = 1"
    ))
    .bind(short_code)
    .fetch_optional(pool)
    .await
}

/// Return all links joined with their total click counts, newest first.
/// When `user_id_filter` is Some, only return links owned by that user.
/// When None (admin), return all links.
pub async fn get_all_links_with_stats(
    pool: &SqlitePool,
    user_id_filter: Option<i64>,
) -> Result<Vec<LinkWithStats>, sqlx::Error> {
    let (where_clause, bind_val) = match user_id_filter {
        Some(uid) => ("WHERE l.user_id = ?1", Some(uid)),
        None => ("", None),
    };

    let sql = format!(
        "SELECT l.id, l.short_code, l.original_url, l.title, l.description,
                l.created_at, l.is_active, COUNT(c.id) as click_count, l.user_id
         FROM links l
         LEFT JOIN clicks c ON c.link_id = l.id
         {where_clause}
         GROUP BY l.id
         ORDER BY l.created_at DESC"
    );

    let rows: Vec<LinkStatsRow> = if let Some(uid) = bind_val {
        sqlx::query_as(&sql).bind(uid).fetch_all(pool).await?
    } else {
        sqlx::query_as(&sql).fetch_all(pool).await?
    };

    Ok(rows
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
                user_id,
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
                    user_id,
                }
            },
        )
        .collect())
}

/// Fetch a single link by its primary key (any status).
pub async fn get_link_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Link>, sqlx::Error> {
    sqlx::query_as(&format!("SELECT {LINK_COLUMNS} FROM links WHERE id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
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

/// Record a click event.
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

/// Count total short links, optionally filtered by user.
pub async fn count_links(
    pool: &SqlitePool,
    user_id_filter: Option<i64>,
) -> Result<i64, sqlx::Error> {
    match user_id_filter {
        Some(uid) => {
            let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links WHERE user_id = ?1")
                .bind(uid)
                .fetch_one(pool)
                .await?;
            Ok(count)
        }
        None => {
            let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links")
                .fetch_one(pool)
                .await?;
            Ok(count)
        }
    }
}

/// Count total short link clicks, optionally filtered by user.
pub async fn count_total_clicks(
    pool: &SqlitePool,
    user_id_filter: Option<i64>,
) -> Result<i64, sqlx::Error> {
    match user_id_filter {
        Some(uid) => {
            let (count,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM clicks c JOIN links l ON l.id = c.link_id WHERE l.user_id = ?1",
            )
            .bind(uid)
            .fetch_one(pool)
            .await?;
            Ok(count)
        }
        None => {
            let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clicks")
                .fetch_one(pool)
                .await?;
            Ok(count)
        }
    }
}

/// Top short links by click count, optionally filtered by user.
pub async fn top_links_by_clicks(
    pool: &SqlitePool,
    limit: i64,
    user_id_filter: Option<i64>,
) -> Result<Vec<LinkWithStats>, sqlx::Error> {
    let (where_clause, bind_uid) = match user_id_filter {
        Some(uid) => ("WHERE l.user_id = ?2", Some(uid)),
        None => ("", None),
    };

    let sql = format!(
        "SELECT l.id, l.short_code, l.original_url, l.title, l.description,
                l.created_at, l.is_active, COUNT(c.id) as click_count, l.user_id
         FROM links l
         LEFT JOIN clicks c ON c.link_id = l.id
         {where_clause}
         GROUP BY l.id
         ORDER BY click_count DESC
         LIMIT ?1"
    );

    let rows: Vec<LinkStatsRow> = if let Some(uid) = bind_uid {
        sqlx::query_as(&sql)
            .bind(limit)
            .bind(uid)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as(&sql).bind(limit).fetch_all(pool).await?
    };

    Ok(rows
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
                user_id,
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
                    user_id,
                }
            },
        )
        .collect())
}

/// Recent short link clicks with labels for the dashboard.
pub async fn recent_clicks_with_labels(
    pool: &SqlitePool,
    limit: i64,
    user_id_filter: Option<i64>,
) -> Result<
    Vec<(
        String,
        NaiveDateTime,
        Option<String>,
        Option<String>,
        Option<String>,
    )>,
    sqlx::Error,
> {
    let (where_clause, bind_uid) = match user_id_filter {
        Some(uid) => ("WHERE l.user_id = ?2", Some(uid)),
        None => ("", None),
    };

    let sql = format!(
        "SELECT l.title, l.short_code, c.clicked_at, c.country, c.browser, c.referer
         FROM clicks c
         JOIN links l ON l.id = c.link_id
         {where_clause}
         ORDER BY c.clicked_at DESC
         LIMIT ?1"
    );

    let rows: Vec<ClickActivityRow> = if let Some(uid) = bind_uid {
        sqlx::query_as(&sql)
            .bind(limit)
            .bind(uid)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as(&sql).bind(limit).fetch_all(pool).await?
    };

    Ok(rows
        .into_iter()
        .map(|(title, code, clicked_at, country, browser, referer)| {
            let label = title.unwrap_or(code);
            (label, clicked_at, country, browser, referer)
        })
        .collect())
}

/// Fetch full analytics for one link.
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
