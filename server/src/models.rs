use chrono::NaiveDateTime;

// ── Users ─────────────────────────────────────────────────────────────────

/// A user account.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: String,
    pub is_approved: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub force_password_change: bool,
}

// ── Short Links ───────────────────────────────────────────────────────────

/// A shortened link record from the `links` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Link {
    pub id: i64,
    pub short_code: String,
    pub original_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub is_active: bool,
    pub user_id: Option<i64>,
}

/// A single click event from the `clicks` table.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct Click {
    pub id: i64,
    pub link_id: i64,
    pub clicked_at: NaiveDateTime,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device_type: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

/// A link row joined with its aggregated click count, used on the dashboard.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LinkWithStats {
    pub id: i64,
    pub short_code: String,
    pub original_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub is_active: bool,
    pub click_count: i64,
    pub user_id: Option<i64>,
}

/// Summary statistics for the analytics page of a single link.
#[derive(Debug, Clone)]
pub struct AnalyticsSummary {
    pub link: Link,
    pub total_clicks: i64,
    pub unique_ips: i64,
    pub clicks: Vec<Click>,
}

// ── Bio Pages ─────────────────────────────────────────────────────────────

/// A bio page record from the `bio_pages` table.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct BioPage {
    pub id: i64,
    pub slug: String,
    pub display_name: String,
    pub bio: String,
    pub profile_image_url: Option<String>,
    pub background_type: String,
    pub background_value: String,
    pub template_name: String,
    pub custom_css: String,
    pub email_address: Option<String>,
    pub is_published: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub user_id: Option<i64>,
}

/// A link on a bio page.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct BioLink {
    pub id: i64,
    pub page_id: i64,
    pub title: String,
    pub url: String,
    pub sort_order: i64,
    pub is_active: bool,
}

/// A social media link on a bio page.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct BioSocialLink {
    pub id: i64,
    pub page_id: i64,
    pub platform: String,
    pub url: String,
    pub sort_order: i64,
}

/// A bio page loaded with all its links, used for rendering.
#[derive(Debug, Clone)]
pub struct BioPageFull {
    pub page: BioPage,
    pub links: Vec<BioLink>,
    pub social_links: Vec<BioSocialLink>,
}

/// A click event on a bio page link.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct BioLinkClick {
    pub id: i64,
    pub bio_link_id: i64,
    pub page_id: i64,
    pub clicked_at: NaiveDateTime,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device_type: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

/// A bio page with its total click count across all links.
#[derive(Debug, Clone)]
pub struct BioPageWithClicks {
    pub slug: String,
    pub display_name: String,
    pub click_count: i64,
}

/// A bio link click with link title and page slug for display.
#[derive(Debug, Clone)]
pub struct BioLinkClickDetail {
    pub link_title: String,
    pub page_slug: String,
    pub clicked_at: NaiveDateTime,
    pub country: Option<String>,
    pub referer: Option<String>,
    pub browser: Option<String>,
}

/// A page view on a bio/links page.
#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub struct BioPageView {
    pub id: i64,
    pub page_id: i64,
    pub viewed_at: NaiveDateTime,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub device_type: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
}

/// Per-link click count for the links page analytics breakdown.
#[derive(Debug, Clone)]
pub struct BioLinkClickCount {
    pub title: String,
    pub url: String,
    pub click_count: i64,
}

/// Full analytics summary for a single links page.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BioPageAnalytics {
    pub page: BioPage,
    pub total_views: i64,
    pub unique_view_ips: i64,
    pub total_link_clicks: i64,
    pub link_click_counts: Vec<BioLinkClickCount>,
    pub views: Vec<BioPageView>,
    pub link_clicks: Vec<BioLinkClick>,
}
