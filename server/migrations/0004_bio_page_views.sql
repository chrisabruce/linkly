-- Track page views on bio/links pages (separate from link clicks)
CREATE TABLE IF NOT EXISTS bio_page_views (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    page_id     INTEGER NOT NULL REFERENCES bio_pages(id) ON DELETE CASCADE,
    viewed_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    ip_address  TEXT,
    user_agent  TEXT,
    referer     TEXT,
    browser     TEXT,
    os          TEXT,
    device_type TEXT,
    country     TEXT,
    region      TEXT,
    city        TEXT
);

CREATE INDEX idx_bio_page_views_page_id ON bio_page_views(page_id);
CREATE INDEX idx_bio_page_views_viewed_at ON bio_page_views(viewed_at);
