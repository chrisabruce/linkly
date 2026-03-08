-- Linkly — Bio Pages (Linktree-style profile pages)

-- Bio pages: one row per profile page
CREATE TABLE IF NOT EXISTS bio_pages (
    id                INTEGER  PRIMARY KEY AUTOINCREMENT,
    slug              TEXT     NOT NULL UNIQUE,
    display_name      TEXT     NOT NULL,
    bio               TEXT     NOT NULL DEFAULT '',
    profile_image_url TEXT,
    background_type   TEXT     NOT NULL DEFAULT 'color',
    background_value  TEXT     NOT NULL DEFAULT '#ffffff',
    template_name     TEXT     NOT NULL DEFAULT 'minimal',
    custom_css        TEXT     NOT NULL DEFAULT '',
    email_address     TEXT,
    is_published      INTEGER  NOT NULL DEFAULT 0,
    created_at        TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at        TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Bio links: regular links displayed on the page
CREATE TABLE IF NOT EXISTS bio_links (
    id          INTEGER  PRIMARY KEY AUTOINCREMENT,
    page_id     INTEGER  NOT NULL REFERENCES bio_pages(id) ON DELETE CASCADE,
    title       TEXT     NOT NULL,
    url         TEXT     NOT NULL,
    sort_order  INTEGER  NOT NULL DEFAULT 0,
    is_active   INTEGER  NOT NULL DEFAULT 1
);

-- Bio social links: platform-specific social media links with icons
CREATE TABLE IF NOT EXISTS bio_social_links (
    id          INTEGER  PRIMARY KEY AUTOINCREMENT,
    page_id     INTEGER  NOT NULL REFERENCES bio_pages(id) ON DELETE CASCADE,
    platform    TEXT     NOT NULL,
    url         TEXT     NOT NULL,
    sort_order  INTEGER  NOT NULL DEFAULT 0
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_bio_pages_slug       ON bio_pages(slug);
CREATE INDEX IF NOT EXISTS idx_bio_links_page_id    ON bio_links(page_id);
CREATE INDEX IF NOT EXISTS idx_bio_social_page_id   ON bio_social_links(page_id);
