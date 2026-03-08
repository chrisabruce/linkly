use crate::models::{
    BioLink, BioLinkClick, BioLinkClickCount, BioLinkClickDetail, BioPage, BioPageAnalytics,
    BioPageFull, BioPageView, BioPageWithClicks, BioSocialLink,
};
use sqlx::SqlitePool;

// ── Bio Pages ─────────────────────────────────────────────────────────────

const BIO_PAGE_COLUMNS: &str =
    "id, slug, display_name, bio, profile_image_url, background_type, background_value,
     template_name, custom_css, email_address, is_published, created_at, updated_at, user_id";

/// Fetch all bio pages, newest first.
/// When `user_id_filter` is Some, only return pages owned by that user.
/// When None (admin), return all pages.
pub async fn get_all_bio_pages(
    pool: &SqlitePool,
    user_id_filter: Option<i64>,
) -> Result<Vec<BioPage>, sqlx::Error> {
    match user_id_filter {
        Some(uid) => {
            sqlx::query_as(&format!(
                "SELECT {BIO_PAGE_COLUMNS} FROM bio_pages WHERE user_id = ?1 ORDER BY created_at DESC"
            ))
            .bind(uid)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as(&format!(
                "SELECT {BIO_PAGE_COLUMNS} FROM bio_pages ORDER BY created_at DESC"
            ))
            .fetch_all(pool)
            .await
        }
    }
}

/// Fetch a single bio page by ID.
pub async fn get_bio_page_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<BioPage>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {BIO_PAGE_COLUMNS} FROM bio_pages WHERE id = ?1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Fetch a published bio page by its slug (for public rendering).
pub async fn get_published_bio_page_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> Result<Option<BioPage>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {BIO_PAGE_COLUMNS} FROM bio_pages WHERE slug = ?1 AND is_published = 1"
    ))
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// Fetch a bio page by slug (any status, for validation purposes).
pub async fn get_bio_page_by_slug(
    pool: &SqlitePool,
    slug: &str,
) -> Result<Option<BioPage>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {BIO_PAGE_COLUMNS} FROM bio_pages WHERE slug = ?1"
    ))
    .bind(slug)
    .fetch_optional(pool)
    .await
}

/// Fetch all links for a given bio page, ordered by sort_order.
pub async fn get_bio_links(
    pool: &SqlitePool,
    page_id: i64,
) -> Result<Vec<BioLink>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, page_id, title, url, sort_order, is_active
         FROM bio_links WHERE page_id = ?1
         ORDER BY sort_order ASC",
    )
    .bind(page_id)
    .fetch_all(pool)
    .await
}

/// Fetch all social links for a given bio page, ordered by sort_order.
pub async fn get_bio_social_links(
    pool: &SqlitePool,
    page_id: i64,
) -> Result<Vec<BioSocialLink>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, page_id, platform, url, sort_order
         FROM bio_social_links WHERE page_id = ?1
         ORDER BY sort_order ASC",
    )
    .bind(page_id)
    .fetch_all(pool)
    .await
}

/// Load a full bio page with all its links and social links.
pub async fn get_bio_page_full(
    pool: &SqlitePool,
    page_id: i64,
) -> Result<Option<BioPageFull>, sqlx::Error> {
    let page = match get_bio_page_by_id(pool, page_id).await? {
        Some(p) => p,
        None => return Ok(None),
    };
    let links = get_bio_links(pool, page_id).await?;
    let social_links = get_bio_social_links(pool, page_id).await?;
    Ok(Some(BioPageFull {
        page,
        links,
        social_links,
    }))
}

/// Load a published bio page by slug with all its links.
pub async fn get_published_bio_page_full(
    pool: &SqlitePool,
    slug: &str,
) -> Result<Option<BioPageFull>, sqlx::Error> {
    let page = match get_published_bio_page_by_slug(pool, slug).await? {
        Some(p) => p,
        None => return Ok(None),
    };
    let links = get_bio_links(pool, page.id).await?;
    let social_links = get_bio_social_links(pool, page.id).await?;
    Ok(Some(BioPageFull {
        page,
        links,
        social_links,
    }))
}

/// Check if a bio page slug already exists (any status).
pub async fn bio_slug_exists(pool: &SqlitePool, slug: &str) -> Result<bool, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bio_pages WHERE slug = ?1")
        .bind(slug)
        .fetch_one(pool)
        .await?;
    Ok(row.0 > 0)
}

/// Create a new bio page. Returns the created row.
pub async fn create_bio_page(
    pool: &SqlitePool,
    slug: &str,
    display_name: &str,
    bio: &str,
    template_name: &str,
    user_id: i64,
) -> Result<BioPage, sqlx::Error> {
    let id = sqlx::query(
        "INSERT INTO bio_pages (slug, display_name, bio, template_name, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(slug)
    .bind(display_name)
    .bind(bio)
    .bind(template_name)
    .bind(user_id)
    .execute(pool)
    .await?
    .last_insert_rowid();

    // Safe to unwrap: we just inserted this row
    get_bio_page_by_id(pool, id)
        .await
        .map(|opt| opt.unwrap())
}

/// Update a bio page's core fields.
#[allow(clippy::too_many_arguments)]
pub async fn update_bio_page(
    pool: &SqlitePool,
    id: i64,
    slug: &str,
    display_name: &str,
    bio: &str,
    profile_image_url: Option<&str>,
    background_type: &str,
    background_value: &str,
    template_name: &str,
    custom_css: &str,
    email_address: Option<&str>,
    is_published: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE bio_pages SET
            slug = ?1, display_name = ?2, bio = ?3, profile_image_url = ?4,
            background_type = ?5, background_value = ?6, template_name = ?7,
            custom_css = ?8, email_address = ?9, is_published = ?10,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?11",
    )
    .bind(slug)
    .bind(display_name)
    .bind(bio)
    .bind(profile_image_url)
    .bind(background_type)
    .bind(background_value)
    .bind(template_name)
    .bind(custom_css)
    .bind(email_address)
    .bind(is_published)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a bio page and all its links (cascade).
pub async fn delete_bio_page(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query("DELETE FROM bio_pages WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected > 0)
}

// ── Bio Links CRUD ────────────────────────────────────────────────────────

/// Replace all bio links for a page (delete + re-insert).
pub async fn replace_bio_links(
    pool: &SqlitePool,
    page_id: i64,
    links: &[(String, String, i64, bool)], // (title, url, sort_order, is_active)
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM bio_links WHERE page_id = ?1")
        .bind(page_id)
        .execute(pool)
        .await?;

    for (title, url, sort_order, is_active) in links {
        sqlx::query(
            "INSERT INTO bio_links (page_id, title, url, sort_order, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(page_id)
        .bind(title)
        .bind(url)
        .bind(*sort_order)
        .bind(*is_active)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Replace all social links for a page (delete + re-insert).
pub async fn replace_bio_social_links(
    pool: &SqlitePool,
    page_id: i64,
    links: &[(String, String, i64)], // (platform, url, sort_order)
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM bio_social_links WHERE page_id = ?1")
        .bind(page_id)
        .execute(pool)
        .await?;

    for (platform, url, sort_order) in links {
        sqlx::query(
            "INSERT INTO bio_social_links (page_id, platform, url, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(page_id)
        .bind(platform)
        .bind(url)
        .bind(*sort_order)
        .execute(pool)
        .await?;
    }
    Ok(())
}

// ── Bio Link Clicks ──────────────────────────────────────────────────────

/// Fetch a single bio link by ID.
pub async fn get_bio_link_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<BioLink>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, page_id, title, url, sort_order, is_active
         FROM bio_links WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Record a click on a bio page link.
#[allow(clippy::too_many_arguments)]
pub async fn log_bio_link_click(
    pool: &SqlitePool,
    bio_link_id: i64,
    page_id: i64,
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
        "INSERT INTO bio_link_clicks
             (bio_link_id, page_id, ip_address, user_agent, referer, browser, os, device_type,
              country, region, city)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )
    .bind(bio_link_id)
    .bind(page_id)
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

/// Count total bio link clicks, optionally filtered by user.
pub async fn count_total_bio_link_clicks(
    pool: &SqlitePool,
    user_id_filter: Option<i64>,
) -> Result<i64, sqlx::Error> {
    match user_id_filter {
        Some(uid) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM bio_link_clicks blc
                 JOIN bio_pages bp ON bp.id = blc.page_id
                 WHERE bp.user_id = ?1",
            )
            .bind(uid)
            .fetch_one(pool)
            .await
        }
        None => sqlx::query_scalar("SELECT COUNT(*) FROM bio_link_clicks")
            .fetch_one(pool)
            .await,
    }
}

/// Count total bio pages, optionally filtered by user.
pub async fn count_bio_pages(
    pool: &SqlitePool,
    user_id_filter: Option<i64>,
) -> Result<i64, sqlx::Error> {
    match user_id_filter {
        Some(uid) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM bio_pages WHERE user_id = ?1")
                .bind(uid)
                .fetch_one(pool)
                .await
        }
        None => sqlx::query_scalar("SELECT COUNT(*) FROM bio_pages")
            .fetch_one(pool)
            .await,
    }
}

/// Top bio pages by click count, optionally filtered by user.
pub async fn top_bio_pages_by_clicks(
    pool: &SqlitePool,
    limit: i64,
    user_id_filter: Option<i64>,
) -> Result<Vec<BioPageWithClicks>, sqlx::Error> {
    let (where_clause, bind_uid) = match user_id_filter {
        Some(uid) => ("WHERE bp.user_id = ?2", Some(uid)),
        None => ("", None),
    };

    let sql = format!(
        "SELECT bp.slug, bp.display_name, COUNT(blc.id) as click_count
         FROM bio_pages bp
         LEFT JOIN bio_link_clicks blc ON blc.page_id = bp.id
         {where_clause}
         GROUP BY bp.id
         ORDER BY click_count DESC
         LIMIT ?1"
    );

    let rows: Vec<(String, String, i64)> = if let Some(uid) = bind_uid {
        sqlx::query_as(&sql).bind(limit).bind(uid).fetch_all(pool).await?
    } else {
        sqlx::query_as(&sql).bind(limit).fetch_all(pool).await?
    };

    Ok(rows
        .into_iter()
        .map(|(slug, display_name, click_count)| BioPageWithClicks {
            slug,
            display_name,
            click_count,
        })
        .collect())
}

/// Recent bio link clicks with details for the dashboard, optionally filtered by user.
pub async fn recent_bio_link_clicks(
    pool: &SqlitePool,
    limit: i64,
    user_id_filter: Option<i64>,
) -> Result<Vec<BioLinkClickDetail>, sqlx::Error> {
    let (where_clause, bind_uid) = match user_id_filter {
        Some(uid) => ("WHERE bp.user_id = ?2", Some(uid)),
        None => ("", None),
    };

    let sql = format!(
        "SELECT bl.title, bp.slug, blc.clicked_at, blc.country, blc.referer, blc.browser
         FROM bio_link_clicks blc
         JOIN bio_links bl ON bl.id = blc.bio_link_id
         JOIN bio_pages bp ON bp.id = blc.page_id
         {where_clause}
         ORDER BY blc.clicked_at DESC
         LIMIT ?1"
    );

    let rows: Vec<(String, String, chrono::NaiveDateTime, Option<String>, Option<String>, Option<String>)> =
        if let Some(uid) = bind_uid {
            sqlx::query_as(&sql).bind(limit).bind(uid).fetch_all(pool).await?
        } else {
            sqlx::query_as(&sql).bind(limit).fetch_all(pool).await?
        };

    Ok(rows
        .into_iter()
        .map(|(link_title, page_slug, clicked_at, country, referer, browser)| {
            BioLinkClickDetail {
                link_title,
                page_slug,
                clicked_at,
                country,
                referer,
                browser,
            }
        })
        .collect())
}

// ── Bio Page Views ───────────────────────────────────────────────────────

/// Record a page view on a bio/links page.
#[allow(clippy::too_many_arguments)]
pub async fn log_bio_page_view(
    pool: &SqlitePool,
    page_id: i64,
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
        "INSERT INTO bio_page_views
             (page_id, ip_address, user_agent, referer, browser, os, device_type,
              country, region, city)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(page_id)
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

// ── Bio Page Analytics ───────────────────────────────────────────────────

/// Fetch full analytics for a single links page.
pub async fn get_bio_page_analytics(
    pool: &SqlitePool,
    page_id: i64,
) -> Result<Option<BioPageAnalytics>, sqlx::Error> {
    let page = match get_bio_page_by_id(pool, page_id).await? {
        Some(p) => p,
        None => return Ok(None),
    };

    let total_views: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bio_page_views WHERE page_id = ?1")
            .bind(page_id)
            .fetch_one(pool)
            .await?;

    let unique_view_ips: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT ip_address) FROM bio_page_views
         WHERE page_id = ?1 AND ip_address IS NOT NULL",
    )
    .bind(page_id)
    .fetch_one(pool)
    .await?;

    let total_link_clicks: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bio_link_clicks WHERE page_id = ?1")
            .bind(page_id)
            .fetch_one(pool)
            .await?;

    // Per-link click counts
    let click_rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT bl.title, bl.url, COUNT(blc.id) as click_count
         FROM bio_links bl
         LEFT JOIN bio_link_clicks blc ON blc.bio_link_id = bl.id
         WHERE bl.page_id = ?1
         GROUP BY bl.id
         ORDER BY click_count DESC",
    )
    .bind(page_id)
    .fetch_all(pool)
    .await?;

    let link_click_counts: Vec<BioLinkClickCount> = click_rows
        .into_iter()
        .map(|(title, url, click_count)| BioLinkClickCount {
            title,
            url,
            click_count,
        })
        .collect();

    // Recent page views
    let views: Vec<BioPageView> = sqlx::query_as(
        "SELECT id, page_id, viewed_at, ip_address, user_agent,
                referer, browser, os, device_type, country, region, city
         FROM bio_page_views
         WHERE page_id = ?1
         ORDER BY viewed_at DESC
         LIMIT 500",
    )
    .bind(page_id)
    .fetch_all(pool)
    .await?;

    // Recent link clicks
    let link_clicks: Vec<BioLinkClick> = sqlx::query_as(
        "SELECT id, bio_link_id, page_id, clicked_at, ip_address, user_agent,
                referer, browser, os, device_type, country, region, city
         FROM bio_link_clicks
         WHERE page_id = ?1
         ORDER BY clicked_at DESC
         LIMIT 500",
    )
    .bind(page_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(BioPageAnalytics {
        page,
        total_views,
        unique_view_ips,
        total_link_clicks,
        link_click_counts,
        views,
        link_clicks,
    }))
}
