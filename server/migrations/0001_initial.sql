-- Linkly — Initial Schema

-- Links table: stores all short links
CREATE TABLE IF NOT EXISTS links (
    id           INTEGER  PRIMARY KEY AUTOINCREMENT,
    short_code   TEXT     NOT NULL UNIQUE,
    original_url TEXT     NOT NULL,
    title        TEXT,
    description  TEXT,
    created_at   TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    is_active    INTEGER  NOT NULL DEFAULT 1
);

-- Clicks table: one row per click event
CREATE TABLE IF NOT EXISTS clicks (
    id          INTEGER  PRIMARY KEY AUTOINCREMENT,
    link_id     INTEGER  NOT NULL REFERENCES links(id) ON DELETE CASCADE,
    clicked_at  TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
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

-- Indexes for fast lookups
CREATE INDEX IF NOT EXISTS idx_links_short_code   ON links(short_code);
CREATE INDEX IF NOT EXISTS idx_links_is_active    ON links(is_active);
CREATE INDEX IF NOT EXISTS idx_clicks_link_id     ON clicks(link_id);
CREATE INDEX IF NOT EXISTS idx_clicks_clicked_at  ON clicks(clicked_at);
