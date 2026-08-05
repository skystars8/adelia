# Ubuntu VPS deployment

This reference layout runs Adelia behind Nginx on a single Ubuntu VPS:

```text
Internet → HTTPS/Nginx → static HTML, assets, and media
                     └→ Rust on 127.0.0.1:8080 for writes, moderation, banners, and health
                                      └→ local PostgreSQL
```

Replace `community.example.org`, passwords, and paths for your deployment. Read the entire guide before executing commands.

## 1. Prepare DNS and the host

Point the domain's A/AAAA records at the VPS. Update Ubuntu and install required packages:

```sh
sudo apt update
sudo apt upgrade
sudo apt install build-essential pkg-config libssl-dev postgresql postgresql-client nginx certbot python3-certbot-nginx
```

Install Rust for the build account from <https://rustup.rs/> or build the release binary on a compatible trusted machine.

Enable a basic firewall without locking out SSH:

```sh
sudo ufw allow OpenSSH
sudo ufw allow 'Nginx Full'
sudo ufw enable
```

Keep PostgreSQL and Rust bound to loopback. Only Nginx should listen publicly.

## 2. Create the service account and directories

```sh
sudo useradd --system --home /opt/adelia --shell /usr/sbin/nologin adelia
sudo install -d -o root -g root -m 0755 /opt/adelia
sudo install -d -o adelia -g adelia -m 0750 /opt/adelia/generated
sudo install -d -o adelia -g adelia -m 0750 /opt/adelia/data/uploads
```

The application account has no interactive login.

## 3. Create PostgreSQL credentials

Generate a URL-safe password:

```sh
openssl rand -hex 32
```

Open PostgreSQL:

```sh
sudo -u postgres psql
```

Run these statements after replacing the password:

```sql
CREATE ROLE adelia_app LOGIN PASSWORD 'replace-with-the-generated-password';
CREATE DATABASE adelia OWNER adelia_app;
\q
```

Do not use the PostgreSQL `postgres` role in Adelia's configuration.

## 4. Build and install Adelia

From a clean checkout:

```sh
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --locked --release
```

Install the binary and runtime files:

```sh
sudo install -o root -g root -m 0755 target/release/adelia /opt/adelia/adelia
sudo cp -a app_templates /opt/adelia/
sudo cp -a web /opt/adelia/
sudo cp -a deploy /opt/adelia/
sudo chown -R root:root /opt/adelia/app_templates /opt/adelia/web /opt/adelia/deploy
```

Rust embeds database migrations in the executable. Templates and web assets remain external so they can be served and updated independently.

## 5. Create the production environment

Generate a separate application secret:

```sh
openssl rand -hex 48
```

Create `/opt/adelia/.env`:

```dotenv
DATABASE_URL=postgres://adelia_app:replace-with-database-password@127.0.0.1:5432/adelia
APP_SECRET=replace-with-the-48-byte-random-value
BIND_ADDR=127.0.0.1:8080
PUBLIC_BASE_URL=https://community.example.org
DEVELOPMENT_MODE=false
SITE_TITLE=My Community
SITE_SUBTITLE=Independent discussion for our community
SECURE_COOKIES=true
DB_MIN_CONNECTIONS=2
DB_MAX_CONNECTIONS=20
MAX_UPLOAD_BYTES=8388608
MAX_BODY_CHARS=20000
SESSION_HOURS=12
GENERATED_DIR=generated
UPLOAD_DIR=data/uploads
TEMPLATE_DIR=app_templates
ASSET_DIR=web/assets
RUST_LOG=adelia=info,tower_http=warn
```

Protect it:

```sh
sudo chown root:adelia /opt/adelia/.env
sudo chmod 0640 /opt/adelia/.env
```

Keep `APP_SECRET` stable. Rotating it invalidates sessions and changes future secure tripcodes.

## 6. Create the administrator

Use an interactive shell variable so the password is not written into the command history:

```sh
cd /opt/adelia
read -rsp 'Adelia administrator password: ' ADELIA_ADMIN_PASSWORD
echo
export ADELIA_ADMIN_PASSWORD
sudo --preserve-env=ADELIA_ADMIN_PASSWORD -u adelia ./adelia admin admin
unset ADELIA_ADMIN_PASSWORD
```

Use a unique password of at least 12 characters.

## 7. Install systemd

```sh
sudo install -o root -g root -m 0644 deploy/adelia.service /etc/systemd/system/adelia.service
sudo systemctl daemon-reload
sudo systemctl enable --now adelia
sudo systemctl status adelia
```

The service:

- restarts after failure;
- uses the unprivileged `adelia` account;
- receives SIGTERM for graceful shutdown;
- restricts filesystem access;
- permits writes only to generated pages and uploads.

Verify Rust directly:

```sh
curl --fail http://127.0.0.1:8080/healthz
curl --fail http://127.0.0.1:8080/readyz
```

## 8. Install Nginx

Copy `deploy/nginx.conf` to `/etc/nginx/sites-available/adelia` and replace every occurrence of `community.example.org` with the real domain.

```sh
sudo install -o root -g root -m 0644 deploy/nginx.conf /etc/nginx/sites-available/adelia
sudo ln -s /etc/nginx/sites-available/adelia /etc/nginx/sites-enabled/adelia
sudo nginx -t
sudo systemctl reload nginx
```

Remove the default site if it conflicts, then validate again before reloading.

The example:

- serves `generated/`, `web/assets/`, and `data/uploads/` directly;
- proxies posts, reports, banners, moderation, health, and missing static files;
- disables access logs;
- clears common client-address forwarding headers;
- repeats defensive browser headers at the edge.

## 9. Enable HTTPS

After HTTP works and DNS has propagated:

```sh
sudo certbot --nginx -d community.example.org
```

Confirm certificate renewal:

```sh
sudo certbot renew --dry-run
```

Do not enable `SECURE_COOKIES=true` until the public site is actually accessed through HTTPS; do not leave it false afterward.

## 10. Operational checks

```sh
systemctl is-active adelia
curl --fail https://community.example.org/healthz
curl --fail https://community.example.org/readyz
journalctl -u adelia --since today
```

The reference Nginx configuration suppresses access logs to avoid retaining client-address records. Rust does not store client addresses. Review VPS-provider, CDN, firewall, monitoring, and backup systems separately because they exist outside Adelia's privacy boundary.

## Backups

Back up PostgreSQL and uploads together. A simple starting point:

```sh
sudo install -d -o root -g root -m 0700 /var/backups/adelia
sudo -u postgres pg_dump --format=custom adelia | sudo tee /var/backups/adelia/database.dump >/dev/null
sudo tar -C /opt/adelia -czf /var/backups/adelia/uploads.tar.gz data/uploads
```

Store encrypted/off-host copies according to the community's needs. Back up `.env` separately in a secrets-safe location. Generated HTML can be recreated:

```sh
sudo -u adelia sh -c 'cd /opt/adelia && ./adelia rebuild'
```

Practice restoring the database and uploads onto another machine. A backup that has never been restored is unproven.

## Upgrades

1. Read release notes and migration changes.
2. Back up the database and uploads.
3. Build and test the new release.
4. Keep the previous binary available for application rollback.
5. Stop Adelia.
6. replace the binary, templates, and assets;
7. start Adelia and check `readyz`, public pages, posting, and moderation.

```sh
sudo systemctl stop adelia
sudo install -o root -g root -m 0755 target/release/adelia /opt/adelia/adelia
sudo cp -a app_templates /opt/adelia/
sudo cp -a web /opt/adelia/
sudo chown -R root:root /opt/adelia/app_templates /opt/adelia/web
sudo systemctl start adelia
curl --fail http://127.0.0.1:8080/readyz
```

Database migrations run when the new binary starts. An old binary may not understand a migrated schema, so rollback planning must include a database restore when a release contains incompatible migrations.

## Traffic spikes

The first scaling advantage is already built in: Nginx serves existing public reads without acquiring PostgreSQL connections. Before changing limits:

- measure Nginx throughput, disk latency, Rust CPU/memory, pool utilization, PostgreSQL locks, and static rebuild duration;
- cache static/media paths at an upstream layer only after verifying privacy and invalidation behavior;
- keep write limits and the database pool bounded;
- raise PostgreSQL capacity based on measurements rather than expected visitor counts;
- arrange upstream denial-of-service protection if the community becomes a likely target.

Load-test a staging deployment, not the production community.
