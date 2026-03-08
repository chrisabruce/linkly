-- Track clicks on links within bio/links pages
CREATE TABLE IF NOT EXISTS bio_link_clicks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    bio_link_id INTEGER NOT NULL REFERENCES bio_links(id) ON DELETE CASCADE,
    page_id     INTEGER NOT NULL REFERENCES bio_pages(id) ON DELETE CASCADE,
    clicked_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
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

CREATE INDEX idx_bio_link_clicks_bio_link_id ON bio_link_clicks(bio_link_id);
CREATE INDEX idx_bio_link_clicks_page_id ON bio_link_clicks(page_id);
CREATE INDEX idx_bio_link_clicks_clicked_at ON bio_link_clicks(clicked_at);
