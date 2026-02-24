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

### Option A — download a pre-built binary

Grab the latest release for your platform from the [Releases](../../releases) page, put it somewhere on your PATH, and you're done.

### Option B — build from source

You'll need [Rust](https://rustup.rs) installed (stable toolchain). Then:

```sh
git clone https://github.com/yourcompany/linkly.git
cd linkly/server
make build
```

The release binary ends up at `server/target/release/linkly`. Copy it wherever you like.

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

Open your browser to `http://localhost:3000` (or whatever your `BASE_URL` is) and log in with the password you set.

---

## Configuration reference

All configuration is done through the `.env` file (or real environment variables if you prefer).

| Variable | Required | Default | Description |
|---|---|---|---|
| `ADMIN_PASSWORD` | **yes** | — | Password for the admin interface |
| `BASE_URL` | no | `http://localhost:3000` | Public-facing URL of your Linkly instance, used when displaying short links. No trailing slash. |
| `DATABASE_URL` | no | `sqlite:./linkly.db` | Path to the SQLite database file |
| `HOST` | no | `0.0.0.0` | Network interface to bind to |
| `PORT` | no | `3000` | Port to listen on |
| `SESSION_DURATION_HOURS` | no | `24` | How long you stay logged in before the session expires |
| `RUST_LOG` | no | `linkly=info` | Log verbosity. Use `linkly=debug` if something is going wrong and you want more detail. |

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

## Deploying to fly.io

fly.io is a good fit for Linkly — you get a real public URL with HTTPS out of the box, a persistent volume for the database, and the whole thing runs on their smallest shared VM for a few dollars a month.

**Prerequisites**

- A [fly.io account](https://fly.io)
- The `flyctl` CLI: `curl -L https://fly.io/install.sh | sh`

**1. Log in**

```sh
fly auth login
```

**2. Register the app**

Run this from the root of the repo (where `fly.toml` lives). The `--no-deploy` flag registers the app name without trying to build anything yet.

```sh
fly launch --no-deploy
```

When asked if you want to tweak the settings, say yes and set your app name — this becomes the subdomain, e.g. `my-linkly.fly.dev`. Then open `fly.toml` and update the `app` and `primary_region` fields to match.

**3. Create the persistent volume**

Linkly uses SQLite, so the database file needs to survive deploys. This creates a 1 GB volume (more than enough for years of click data):

```sh
fly volumes create linkly_data --region ord --size 1
```

Replace `ord` with whatever region you set in `fly.toml`.

**4. Set your secrets**

These are stored encrypted on fly.io and injected at runtime — they never appear in the image or in `fly.toml`:

```sh
fly secrets set ADMIN_PASSWORD="your-strong-password-here"
fly secrets set BASE_URL="https://your-app-name.fly.dev"
```

If you're using a custom domain instead of `.fly.dev`, set `BASE_URL` to that instead.

**5. Deploy**

```sh
fly deploy
```

The first build will take a few minutes while Rust compiles everything. Subsequent deploys are faster because Docker layers are cached on fly's build machines.

**6. Open it**

```sh
fly open
```

Or just navigate to `https://your-app-name.fly.dev` in your browser.

---

**Custom domain**

If you want `go.yourcompany.com` instead of a `.fly.dev` URL:

```sh
fly certs add go.yourcompany.com
```

fly.io will give you a DNS record to add. Once DNS propagates and the certificate issues, update your `BASE_URL` secret:

```sh
fly secrets set BASE_URL="https://go.yourcompany.com"
fly deploy
```

---

**Viewing logs**

```sh
fly logs
```

---

**A note on scaling**

Keep Linkly at exactly one machine. SQLite does not support multiple concurrent writers, so running two instances at once will cause database errors. The `fly.toml` is already configured with `min_machines_running = 1` to keep one machine warm at all times and prevent cold-start delays on redirects.

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

1. Log in to the admin dashboard
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
Edit your `.env` file, change `ADMIN_PASSWORD`, and restart Linkly. Any existing sessions will be invalidated automatically on restart.

**The database file is getting large**
The click history is the main culprit. You can prune old clicks directly with the SQLite CLI:

```sh
sqlite3 linkly.db "DELETE FROM clicks WHERE clicked_at < datetime('now', '-6 months');"
sqlite3 linkly.db "VACUUM;"
```

---

## License

MIT — do whatever you want with it.