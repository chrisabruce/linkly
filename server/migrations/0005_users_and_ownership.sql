-- Multi-user support: users table and ownership columns

CREATE TABLE IF NOT EXISTS users (
    id            INTEGER  PRIMARY KEY AUTOINCREMENT,
    email         TEXT     NOT NULL UNIQUE,
    display_name  TEXT     NOT NULL,
    password_hash TEXT     NOT NULL,
    role          TEXT     NOT NULL DEFAULT 'user',
    is_approved   INTEGER  NOT NULL DEFAULT 0,
    created_at    TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT     NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Add user_id to links (nullable for backward compat with existing rows)
ALTER TABLE links ADD COLUMN user_id INTEGER REFERENCES users(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_links_user_id ON links(user_id);

-- Add user_id to bio_pages (nullable for backward compat with existing rows)
ALTER TABLE bio_pages ADD COLUMN user_id INTEGER REFERENCES users(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_bio_pages_user_id ON bio_pages(user_id);
