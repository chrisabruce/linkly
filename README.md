# Linkly

Linkly is a self-hosted URL shortener and link-in-bio page builder. Shorten URLs, track clicks with detailed analytics, and create customizable profile pages — all from a single binary backed by SQLite. No SaaS subscription, no data leaving your servers, no per-seat pricing.

It runs as a single small binary, uses a SQLite database that lives right next to it, and serves a clean web interface built with [Pico CSS](https://picocss.com).

---

## Features

### URL Shortening
- Shorten any URL to a compact link like `https://go.yourcompany.com/abc123`
- Optionally set a **title**, **description**, and **custom code** (e.g. `/q3-report`)
- Real-time custom code validation via [Datastar](https://data-star.dev)
- In-memory link cache for fast redirects

### Link-in-Bio Pages
- Create Linktree-style profile pages at `https://go.yourcompany.com/your-slug`
- Five built-in templates: Minimal, Bold, Rounded, Glass, and Neon
- Add links, social media icons, profile images, and custom CSS
- Background customization: solid colors, gradients, or Unsplash photos
- S3-compatible image uploads for profile pictures
- Per-page analytics with click tracking on individual links

### Analytics
- Every click is tracked: timestamp, IP, country, city, browser, OS, device type, and referrer
- Dashboard overview with top links, top bio pages, and recent activity
- Per-link analytics with breakdown charts for browser, OS, device, country, and referrer
- Bio page analytics with page views and per-link click counts
- IP geolocation via [ip-api.com](http://ip-api.com) (optional — works without it)

### Multi-User System
- JWT-based authentication with role-based access control (admin / user)
- Self-registration with admin approval workflow
- Admins can create users directly and optionally force a password change on first login
- Users see only their own links and pages; admins see everything
- Ownership tracking on all links and bio pages
- Argon2id password hashing

### Customization
- Configurable application title via `APP_TITLE` env var — rebrand to anything you like

---

## Requirements

- Linux, macOS, or Windows (WSL works fine)
- [Rust](https://rustup.rs) stable toolchain (to build from source), or Docker
- That's it — the binary includes everything, including the database engine

---

## Installation

### Option A — Build from source

```sh
git clone https://github.com/yourcompany/linkly.git
cd linkly/server
cp .env.example .env
# Edit .env with your settings (see Configuration below)
make build
```

The release binary ends up at `server/target/release/linkly`. Copy it wherever you like.

### Option B — Docker

```sh
docker build -t linkly .
docker run -d \
  -p 8080:8080 \
  -v linkly_data:/data \
  -e JWT_SECRET="a-long-random-secret" \
  -e SEED_ADMIN_EMAIL="admin@example.com" \
  -e SEED_ADMIN_PASSWORD="your-strong-password" \
  -e BASE_URL="https://go.yourcompany.com" \
  -e DATABASE_URL="sqlite:/data/linkly.db" \
  linkly
```

The `-v linkly_data:/data` flag mounts a Docker volume so the SQLite database survives container restarts.

---

## Quick Start

**1. Create your config file**

```sh
cd server
cp .env.example .env
```

Open `.env` and set at minimum:

```
JWT_SECRET=a-long-random-secret-change-this
BASE_URL=https://go.yourcompany.com
```

Optionally seed an admin account (created automatically on first run):

```
SEED_ADMIN_EMAIL=admin@example.com
SEED_ADMIN_PASSWORD=changeme
```

If you skip the seed admin, the first user to register at `/admin/register` automatically becomes the admin.

**2. Run it**

```sh
./linkly
# or from the server directory:
make run
```

Linkly creates the database and runs migrations automatically:

```
INFO linkly: Starting Linkly on 0.0.0.0:3000
INFO linkly: Base URL: https://go.yourcompany.com
INFO linkly: Database migrations applied
INFO linkly: Cache warmed with 0 active link(s)
INFO linkly: Listening on http://0.0.0.0:3000
```

**3. Log in**

Navigate to `https://go.yourcompany.com/admin` and sign in with your seed admin credentials, or register a new account at `/admin/register`.

---

## Configuration

All configuration is done through environment variables (typically via a `.env` file).

### Required

| Variable | Default | Description |
|---|---|---|
| `JWT_SECRET` | — | Secret key for signing authentication tokens. Use a long random string. |

### Application

| Variable | Default | Description |
|---|---|---|
| `APP_TITLE` | `Linkly` | Application name displayed in the nav bar, page titles, and footer. |
| `BASE_URL` | `http://localhost:3000` | Public-facing URL for generating short links. No trailing slash. |
| `ROOT_REDIRECT_URL` | — | Where visitors are sent when they hit `/`. Admins go directly to `/admin`. |
| `DATABASE_URL` | `sqlite:./linkly.db` | Path to the SQLite database file. |
| `HOST` | `0.0.0.0` | Network interface to bind to. |
| `PORT` | `3000` | Port to listen on. |

### Authentication

| Variable | Default | Description |
|---|---|---|
| `SEED_ADMIN_EMAIL` | — | Email for the seed admin account (created on startup if it doesn't exist). |
| `SEED_ADMIN_PASSWORD` | — | Password for the seed admin. Also accepts `ADMIN_PASSWORD` for backward compatibility. |
| `SESSION_DURATION_HOURS` | `24` | How long auth tokens remain valid. |

### S3 Storage (optional — enables image uploads)

| Variable | Description |
|---|---|
| `S3_BUCKET` | S3 bucket name |
| `S3_REGION` | S3 region (e.g. `us-east-1`) |
| `S3_ENDPOINT` | S3 endpoint URL (use this for S3-compatible services like MinIO or RustFS) |
| `S3_ACCESS_KEY` | S3 access key |
| `S3_SECRET_KEY` | S3 secret key |

All five S3 variables must be set to enable image uploads.

### Unsplash (optional — enables background image search)

| Variable | Description |
|---|---|
| `UNSPLASH_ACCESS_KEY` | Your Unsplash API access key |

### Logging

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `linkly=info,tower_http=info` | Log verbosity. Use `linkly=debug` for more detail. |

---

## URL Routing

| Path | Behaviour |
|---|---|
| `/` | Redirects to `ROOT_REDIRECT_URL` |
| `/health` | Returns `200 OK` (for uptime checks) |
| `/:code` | Resolves and redirects a short link |
| `/admin` | Redirects to `/admin/dashboard` |
| `/admin/login` | Login page |
| `/admin/register` | Self-registration (requires admin approval) |
| `/admin/dashboard` | Analytics overview |
| `/admin/short-links` | Manage short links |
| `/admin/links/:id/analytics` | Per-link analytics |
| `/admin/bio` | Manage link-in-bio pages |
| `/admin/bio/new` | Create a new bio page |
| `/admin/bio/:id/edit` | Edit a bio page |
| `/admin/bio/:id/analytics` | Bio page analytics |
| `/admin/users` | User management (admin only) |
| `/admin/change-password` | Change your password |

---

## User Management

### Roles

- **Admin** — can see all links and pages across all users, manage user accounts, approve registrations, and promote/demote users.
- **User** — can only see and manage their own links and pages.

### First User Setup

You have two options for creating the first admin:

1. **Seed admin** (recommended): Set `SEED_ADMIN_EMAIL` and `SEED_ADMIN_PASSWORD` in your `.env`. The account is created on startup if it doesn't already exist.
2. **Self-registration**: If no seed admin is configured, the first user to register at `/admin/register` automatically becomes an admin and is auto-approved.

### Adding Users

- **Admin creates users**: From `/admin/users`, admins can create accounts with a specific role, set approval status, and optionally check "Force password change on login" to require the user to set their own password.
- **Self-registration**: Users can register at `/admin/register`. Their account is created in a "pending" state and must be approved by an admin before they can log in.

### Force Password Change

When an admin creates a user with "Force password change" enabled, the user is redirected to a password change form immediately after login and cannot access any other page until they set a new password.

---

## Link-in-Bio Pages

Bio pages are customizable profile pages (similar to Linktree) served at `/:slug`. Each page includes:

- Display name and bio text
- Profile image (uploaded via S3 or pasted URL)
- Background (solid color, gradient, or Unsplash photo)
- Ordered list of links (each individually toggleable)
- Social media icons (Twitter/X, Instagram, GitHub, LinkedIn, YouTube, TikTok, and more)
- Email contact link
- Custom CSS for advanced styling
- Published/draft toggle

Five built-in templates control the visual style: **Minimal**, **Bold**, **Rounded**, **Glass**, and **Neon**.

---

## Running Behind a Reverse Proxy

You almost certainly want HTTPS in production. Here are minimal configs for common reverse proxies.

**Caddy** (recommended — handles certificates automatically):

```
go.yourcompany.com {
    reverse_proxy localhost:3000
}
```

**nginx:**

```nginx
server {
    listen 443 ssl;
    server_name go.yourcompany.com;

    # ... your ssl_certificate lines here ...

    location / {
        proxy_pass http://localhost:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

The `X-Forwarded-For` header is important — Linkly reads it to get the real visitor IP for analytics. Without it, every click will appear to come from `127.0.0.1`.

---

## Running as a System Service

**systemd (Linux)**

Create `/etc/systemd/system/linkly.service`:

```ini
[Unit]
Description=Linkly URL shortener
After=network.target

[Service]
Type=simple
User=linkly
WorkingDirectory=/opt/linkly
EnvironmentFile=/opt/linkly/.env
ExecStart=/opt/linkly/linkly
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Then:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now linkly
```

---

## Makefile Targets

Run these from the `server/` directory:

| Target | Description |
|---|---|
| `make build` | Compile a release binary |
| `make run` | Build and run the release binary |
| `make dev` | Run in debug mode with verbose logging |
| `make setup` | Create `.env` from `.env.example` |
| `make check` | Type-check without building |
| `make fmt` | Format source code |
| `make lint` | Run clippy with warnings as errors |
| `make test` | Run the test suite |
| `make clean` | Remove build artifacts |
| `make install` | Install the binary to `/usr/local/bin` |

---

## Data and Privacy

Everything — links, clicks, users, sessions — lives in the single SQLite file specified by `DATABASE_URL`. There is no external database, no cloud sync, and no telemetry. The only external network calls Linkly makes are:

- **IP geolocation** via [ip-api.com](http://ip-api.com) for each unique visitor IP (optional — location data simply won't appear if the service is unreachable)
- **Unsplash API** if configured, only when an admin searches for background images
- **S3 uploads** if configured, only when an admin uploads a profile image

---

## Backup

The entire state of your Linkly instance is in one file:

```sh
cp linkly.db linkly.db.backup
```

SQLite is safe to copy while Linkly is running (WAL mode). For a cleaner snapshot:

```sh
sqlite3 linkly.db ".backup linkly.db.backup"
```

---

## Upgrading

1. Stop Linkly
2. Replace the binary with the new version
3. Start Linkly again

Database migrations run automatically on startup. Migrations only add columns or tables — your data is never touched destructively.

---

## Troubleshooting

**"Short link not found" after visiting a link**
The link may have been deleted, or it may never have existed. Check the dashboard.

**Location shows "—" for all clicks**
Linkly couldn't reach the geolocation service. This is expected on servers with restricted outbound access. Browser, OS, and referrer data still works.

**I forgot my password**
If you have `SEED_ADMIN_EMAIL` and `SEED_ADMIN_PASSWORD` set, update the password value in `.env` and the seed account will be recreated on next startup (only if the email doesn't already exist — you may need to delete the user from the DB first). Alternatively, ask another admin to reset your account from the Users page.

**The database file is getting large**
Click history is the main culprit. Prune old data with:

```sh
sqlite3 linkly.db "DELETE FROM clicks WHERE clicked_at < datetime('now', '-6 months');"
sqlite3 linkly.db "DELETE FROM bio_link_clicks WHERE clicked_at < datetime('now', '-6 months');"
sqlite3 linkly.db "DELETE FROM bio_page_views WHERE viewed_at < datetime('now', '-6 months');"
sqlite3 linkly.db "VACUUM;"
```

---

## Tech Stack

- **Backend:** Rust with [Axum](https://github.com/tokio-rs/axum) 0.7
- **Database:** SQLite via [SQLx](https://github.com/launchbadge/sqlx) 0.7 (with embedded migrations)
- **Templates:** [Askama](https://github.com/djc/askama) 0.12
- **Frontend:** [Pico CSS](https://picocss.com) 2 + [Datastar](https://data-star.dev) 1.0
- **Auth:** JWT ([jsonwebtoken](https://github.com/Keats/jsonwebtoken) 9) + [Argon2id](https://github.com/RustCrypto/password-hashes) password hashing
- **Storage:** [rust-s3](https://github.com/durch/rust-s3) for S3-compatible image uploads

---

## License

MIT — do whatever you want with it.
