use chrono::NaiveDateTime;

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
pub struct LinkWithStats {
    pub id: i64,
    pub short_code: String,
    pub original_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created_at: NaiveDateTime,
    pub is_active: bool,
    pub click_count: i64,
}

/// Summary statistics for the analytics page of a single link.
#[derive(Debug, Clone)]
pub struct AnalyticsSummary {
    pub link: Link,
    pub total_clicks: i64,
    pub unique_ips: i64,
    pub clicks: Vec<Click>,
}
