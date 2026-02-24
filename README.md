# Linkly

Linkly is a self-hosted URL shortener built for internal company use. You paste a long link, get a short one, share it, and then come back later to see exactly who clicked it, where they were, what browser they used, and when. That's it — no SaaS subscription, no data leaving your servers, no per-seat pricing.

It runs as a single small binary (~7 MB), uses a SQLite database that lives right next to it, and serves a clean web interface you can access from any browser. The whole thing is designed to stay out of your way.

---

## What it does

- Shorten any URL to a compact link like `https://go.yourcompany.com/abc123`
- Optionally give a link a **title**, **description**, and a **custom code** (e.g. `/q3-report`)
- Every click is tracked: timestamp, IP address, country, city, browser, OS, device type, and referrer
- The admin dashboard lists all your links with click counts at a glance
- The analytics page for each link breaks down clicks by browser, OS, device, country, and referrer with a simple bar chart — plus a full click-by-click history

---

## Requirements

- Linux, macOS, or Windows (WSL works fine)
- That's it — the binary includes everything, including the database engine

---

## Installation

### Option A — build from source

You'll need [Rust](https://rustup.rs) installed (stable toolchain). Then:

```sh
git clone https://github.com/yourcompany/linkly.git
cd linkly/server
make build
```

The release binary ends up at `server/target/release/linkly`. Copy it wherever you like.

### Option B — Docker

```sh
docker build -t linkly .
docker run -d \
  -p 8080:8080 \
  -v linkly_data:/data \
  -e ADMIN_PASSWORD="your-strong-password-here" \
  -e BASE_URL="https://go.yourcompany.com" \
  -e DATABASE_URL="sqlite:/data/linkly.db" \
  linkly
```

The `-v linkly_data:/data` flag mounts a Docker volume at `/data` so the SQLite database survives container restarts and upgrades.

---

## Setup

**1. Create your config file**

```sh
cd server
cp .env.example .env
```

Open `.env` in a text editor. At minimum you must set two things:

```
ADMIN_PASSWORD=something-strong-here
BASE_URL=https://go.yourcompany.com
```

`BASE_URL` is the public address people will see in their short links. If you're running behind a reverse proxy (nginx, Caddy, etc.), this should be the external URL, not `localhost`.

**2. Run it**

```sh
./linkly
```

Or if you're using the Makefile from the server directory:

```sh
make run
```

Linkly will create the database file automatically on first run and print something like:

```
INFO linkly: Starting Linkly on 0.0.0.0:3000
INFO linkly: Base URL: https://go.yourcompany.com
INFO linkly: Database migrations applied
INFO linkly: Cache warmed with 0 active link(s)
INFO linkly: Listening on http://0.0.0.0:3000
```

Open your browser to `https://go.yourcompany.com/admin` to access the management panel and log in with the password you set.

---

## Configuration reference

All configuration is done through the `.env` file (or real environment variables if you prefer).

| Variable | Required | Default | Description |
|---|---|---|---|
| `ADMIN_PASSWORD` | **yes** | — | Password for the admin interface |
| `BASE_URL` | no | `http://localhost:3000` | Public-facing URL of your Linkly instance, used when displaying short links. No trailing slash. |
| `ROOT_REDIRECT_URL` | no | `https://secedastudios.com` | Where visitors are sent when they hit the root URL `/`. Admins must go directly to `/admin`. |
| `DATABASE_URL` | no | `sqlite:./linkly.db` | Path to the SQLite database file |
| `HOST` | no | `0.0.0.0` | Network interface to bind to |
| `PORT` | no | `3000` | Port to listen on |
| `SESSION_DURATION_HOURS` | no | `24` | How long you stay logged in before the session expires |
| `RUST_LOG` | no | `linkly=info` | Log verbosity. Use `linkly=debug` if something is going wrong and you want more detail. |

---

## URL routing

| Path | Behaviour |
|---|---|
| `/` | Redirects visitors to `ROOT_REDIRECT_URL` |
| `/admin` | Redirects to `/admin/dashboard` |
| `/admin/login` | Login page |
| `/admin/dashboard` | Management dashboard (requires auth) |
| `/:code` | Resolves and redirects the short link |
| `/health` | Returns `200 OK` — use this for uptime checks |

Visitors who land on the root URL are immediately sent to the public site. Admins must navigate directly to `/admin`.

---

## The admin password

Linkly has no user accounts, no OAuth, and no magic links. There is one password, and whoever has it can create and delete links and see all analytics. Treat it like a shared secret for your team.

**Locally**, you set it in `.env` and that file lives only on your machine. Never commit `.env` to git — the `.gitignore` already excludes it, but it's worth knowing why: if your password ends up in your git history, anyone with access to the repo has it.

**On a server**, environment variables for production deployments should be injected at runtime by the platform or your process manager, not written to a file inside a Docker image. Pass them with `-e` flags to `docker run`, via a `docker-compose.yml` `environment` block, or through your server's secrets manager.

**Choosing a good password** — since this is an internal tool it is easy to be lazy here, but the admin panel is publicly reachable on the internet once deployed. Use something you would not be embarrassed to have guessed. A few random words strung together (`correct-horse-battery-staple` style) is easy to remember and hard to brute-force. Linkly adds a 500ms artificial delay on every failed login attempt to slow down anyone trying to guess.

---

## Running behind a reverse proxy

You almost certainly want to put Linkly behind nginx or Caddy so it gets HTTPS. Here are minimal configs for both.

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

## Running as a system service

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

It's a good idea to create a dedicated `linkly` user with no login shell and make sure the working directory is owned by that user, so the database file lands somewhere predictable and safe.

---

## Using Linkly day-to-day

### Creating a short link

1. Log in at `/admin`
2. Paste your destination URL in the **Destination URL** field
3. Optionally fill in a **Title** (shown in the dashboard), a **Description** (a note to remind yourself what the link was for — e.g. "Sent in the October newsletter"), and a **Custom code** if you want a memorable slug instead of a random one
4. Click **Shorten ↗**

Your new short link appears in the table immediately, ready to copy and share.

### Viewing analytics

Click **Analytics** next to any link. You'll see:

- **Total clicks** and **unique IPs** at the top
- Breakdown charts for browsers, operating systems, device types, referrers, and countries
- A full history table at the bottom showing every individual click with its timestamp, IP address, location (country, region, city), browser, OS, device, and where the visitor came from

Location data is looked up automatically from the visitor's IP address using a free geolocation service. Private/internal IPs (like `192.168.x.x`) are never sent to the service and will simply show no location.

### Deleting a link

Click **Delete** next to a link in the dashboard. You'll be asked to confirm. Deleting a link also removes all its click history permanently.

---

## Data and privacy

Everything — links, clicks, session tokens — lives in the single SQLite file specified by `DATABASE_URL`. There is no external database, no cloud sync, and no telemetry phoning home. The only external network call Linkly ever makes is the IP geolocation lookup for each unique visitor IP, which goes to [ip-api.com](http://ip-api.com). If you'd rather not do that, you can run Linkly in an environment with no outbound HTTP access and location data simply won't be populated.

---

## Backup

The entire state of your Linkly instance is in one file. Back it up like any other file:

```sh
cp linkly.db linkly.db.backup
```

SQLite is safe to copy while Linkly is running because it uses WAL (write-ahead logging) mode. For a cleaner snapshot you can also use the SQLite CLI:

```sh
sqlite3 linkly.db ".backup linkly.db.backup"
```

---

## Upgrading

1. Stop Linkly
2. Replace the binary with the new version
3. Start Linkly again

Database migrations run automatically on startup. Your data is never touched destructively — new migrations only add columns or tables.

---

## Troubleshooting

**"Short link not found" after visiting a link**
The link may have been deleted, or it may never have existed. Check the dashboard.

**Location shows "—" for all clicks**
Linkly couldn't reach the geolocation service. This is expected on servers with restricted outbound access. Everything else (browser, OS, referrer) still works fine.

**I forgot my admin password**
Edit your `.env` file (or update the environment variable however your platform manages them), change `ADMIN_PASSWORD` to a new value, and restart Linkly. Any existing sessions are invalidated on restart.

**Linkly won't start — "ADMIN_PASSWORD must be set"**
The server refuses to start without a password configured. Make sure your `.env` file exists and has `ADMIN_PASSWORD` set to something non-empty, or that the environment variable is injected by your process manager before the binary starts.

**The database file is getting large**
The click history is the main culprit. You can prune old clicks directly with the SQLite CLI:

```sh
sqlite3 linkly.db "DELETE FROM clicks WHERE clicked_at < datetime('now', '-6 months');"
sqlite3 linkly.db "VACUUM;"
```

---

## License

MIT — do whatever you want with it.