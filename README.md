# Adelia

[![Rust build and test](https://github.com/skystars8/adelia/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/skystars8/adelia/actions/workflows/ci.yml)
[![Rust dependency audit](https://github.com/skystars8/adelia/actions/workflows/audit.yml/badge.svg?branch=main)](https://github.com/skystars8/adelia/actions/workflows/audit.yml)

Adelia is a fast Rust/PostgreSQL discussion-board engine for hobby and educational communities. It combines the compact flow of a classic independent message board with responsive mobile/desktop layouts, static public pages, modern Rust, PostgreSQL, and a focused moderator interface.

> **Created with AI:** Adelia was designed and implemented by ChatGPT from OpenAI, working from the project owner's requirements, product direction, testing, and repeated feedback. The project intentionally gives ChatGPT clear public credit.

Adelia is a preview release. It is suitable for local development, experimentation, and careful self-hosting, but every public deployment should be tested, monitored, backed up, and kept current.

## Why Adelia

Most public reads never touch Rust or PostgreSQL. When a post or moderation action changes a board, Adelia commits coalesced publication jobs in the same PostgreSQL transaction and regenerates the affected static HTML in retrying background workers. A reverse proxy can then serve board indexes, threads, catalogs, archives, news, stylesheets, JavaScript, and uploaded media directly.

That design keeps ordinary reads inexpensive and leaves the bounded Rust/PostgreSQL path available for writes and moderation during traffic spikes.

## Features

### Public boards

- Removable-frame landing page with a persistent desktop sidebar and mobile slide-out board menu.
- Responsive board and thread layouts designed for both narrow phones and large monitors.
- Anonymous threads and replies with optional names and subjects.
- Secure keyed tripcodes through `Name##a-long-private-secret`; the secret is never stored.
- Quote links, greentext, post highlighting, quick reply, and hide/show posting forms.
- JPEG, PNG, GIF, and WebP uploads with type, byte-size, dimension, and decode-safety checks.
- Rust-generated thumbnails and click-to-expand images.
- Per-board indexes, catalogs, archives, pagination, bump limits, reply limits, locking, and sticky threads.
- Random 300×100 board banners through a no-cache redirect while the images remain cacheable static files.
- A public Options dialog with the default theme and bundled legacy stylesheet choices.
- Visitor reporting without visitor accounts or public self-deletion.

### Board controls

- Lock an entire board against new threads and replies.
- Require moderator approval before new posts become public.
- Protect posting with an optional Argon2-hashed shared board password while keeping reading public.
- Create and browse boards from the administration area.

### Moderation

- Administrator and moderator authentication with Argon2 password hashes.
- Hashed database sessions, HttpOnly/SameSite cookies, CSRF tokens, and same-origin form checks.
- Separate reports, pending-approval, recent-posts, boards, and news pages.
- Board post view with edit, delete, thread lock, and sticky controls beside each post.
- Edit any post and remove or replace its image with a newly validated upload.
- Approve moderated posts and dismiss reports.
- Moderation audit log.
- Development-only `admin` / `mod` login that is refused unless Adelia is bound to loopback and uses a local public URL.

### Reliability and privacy

- PostgreSQL transactions commit posts, moderation changes, and their durable publication jobs atomically.
- Coalescing static-publication queue with automatic retries and a visible moderator-dashboard queue count.
- Bounded connection pool, multipart/image posting work, and Argon2 password work.
- Retrying background publishers, cross-process publication locks, serialized per-board builds, and temporary-file replacement.
- Request timeouts, panic containment, compression, graceful shutdown, and health/readiness endpoints.
- Content Security Policy and other defensive HTTP headers.
- No IP addresses or IP-derived hashes stored in PostgreSQL.
- Non-identifying global, board, and thread pressure limits.
- Nginx example with access logging disabled and forwarding-address headers removed.

## Quick start

### Windows

Requirements: a current Rust toolchain, PostgreSQL, and PowerShell.

1. Install Rust from <https://rustup.rs/>.
2. Install PostgreSQL and remember the `postgres` administrator password.
3. Double-click `dev.bat`.
4. Enter the PostgreSQL administrator password on first run.
5. Open <http://127.0.0.1:8080/>.

The first run creates a dedicated `adelia_app` role, an `adelia_dev` database, a unique local role password, and a random `APP_SECRET`. The PostgreSQL administrator password is not written to disk.

Local moderator login: `admin` / `mod`.

See [Windows development](docs/WINDOWS_DEVELOPMENT.md) for troubleshooting and manual commands.

### Linux

On a Debian/Ubuntu-style development machine:

```sh
chmod +x dev.sh
./dev.sh
```

The script uses `sudo -u postgres` on first run, generates unique local secrets, creates the database, applies migrations, and starts Adelia on <http://127.0.0.1:8080/>.

See [Linux development](docs/LINUX_DEVELOPMENT.md).

### Docker

```sh
docker compose up --build -d
```

Create a Docker development administrator:

```sh
ADELIA_ADMIN_PASSWORD='choose-a-long-local-password' \
  docker compose exec -e ADELIA_ADMIN_PASSWORD app adelia admin admin
```

Then open <http://127.0.0.1:8080/> and sign in at `/mod`. The supplied Compose configuration binds only to loopback and is for development, not production.

See [Docker development](docs/DOCKER.md).

## Common commands

```sh
cargo run -- serve
cargo run -- rebuild
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

To create or reset a production administrator:

```sh
export ADELIA_ADMIN_PASSWORD='a-long-unique-password'
cargo run --release -- admin admin
unset ADELIA_ADMIN_PASSWORD
```

Database migrations run automatically before every command.

## Documentation

- [Windows development](docs/WINDOWS_DEVELOPMENT.md)
- [Linux development](docs/LINUX_DEVELOPMENT.md)
- [Docker development](docs/DOCKER.md)
- [Ubuntu VPS deployment](docs/VPS_DEPLOYMENT.md)
- [Publishing the repository on GitHub](docs/PUBLISHING.md)
- [Administration and board settings](docs/ADMINISTRATION.md)
- [Architecture and static-page lifecycle](docs/ARCHITECTURE.md)
- [Security policy](SECURITY.md)
- [Contributing and AI-assisted contributions](CONTRIBUTING.md)

## Configuration

Copy `.env.example` to `.env` for a manual setup. Never commit `.env`.

| Setting | Purpose |
| --- | --- |
| `DATABASE_URL` | PostgreSQL connection URL for the dedicated application role. |
| `APP_SECRET` | A stable random value of at least 32 characters used for session-token hashes and secure tripcodes. |
| `BIND_ADDR` | Rust listen address; default `127.0.0.1:8080`. |
| `PUBLIC_BASE_URL` | Canonical public origin, including `https://` in production. |
| `SITE_TITLE` | Public community name; the engine remains Adelia. |
| `SITE_SUBTITLE` | Short public description. |
| `DEVELOPMENT_MODE` | Enables `admin` / `mod` only on a loopback-only local configuration. Never enable in production. |
| `SECURE_COOKIES` | Must be `true` behind production HTTPS. |
| `DB_MIN_CONNECTIONS` | Minimum PostgreSQL pool size. |
| `DB_MAX_CONNECTIONS` | Maximum PostgreSQL pool size; default 20. |
| `PUBLISHER_WORKERS` | Background static-page publishers; default 2. `DB_MAX_CONNECTIONS` must be at least twice this value plus 4. |
| `POST_CONCURRENCY` | Maximum simultaneous multipart/image post operations; default 32. |
| `PASSWORD_CONCURRENCY` | Maximum simultaneous Argon2 verification and hashing operations; default 4. |
| `MAX_UPLOAD_BYTES` | Maximum uploaded-image size; default 8 MiB. |
| `MAX_BODY_CHARS` | Maximum post body length; default 20,000 characters. |
| `SESSION_HOURS` | Moderator session lifetime. |
| `GENERATED_DIR` | Static HTML output directory. |
| `UPLOAD_DIR` | Original uploads and thumbnails. |
| `TEMPLATE_DIR` | MiniJinja template directory. |
| `ASSET_DIR` | CSS, JavaScript, themes, and banner directory. |
| `RUST_LOG` | Rust tracing filter. |

Rotating `APP_SECRET` invalidates existing moderator sessions and changes secure tripcodes. Plan rotations rather than changing it casually.

## Repository layout

| Path | Contents |
| --- | --- |
| `src/` | Rust application, routes, security, media validation, and static builder. |
| `app_templates/` | Public and moderator MiniJinja templates. |
| `migrations/` | Ordered PostgreSQL migrations and generic starter content. |
| `web/assets/` | Application CSS/JavaScript, themes, images, and banners. |
| `scripts/` | Windows setup and Docker entrypoint scripts. |
| `deploy/` | Hardened systemd and Nginx examples. |
| `docs/` | Development, administration, architecture, Docker, and VPS guides. |
| `generated/` | Runtime-generated HTML; ignored by Git. |
| `data/uploads/` | Runtime uploads and thumbnails; ignored by Git. |

## Reporting problems

Public issue reports are welcome. Include the Adelia version or commit, operating system, Rust version, PostgreSQL version, browser when relevant, exact reproduction steps, expected behavior, actual behavior, and sanitized logs.

Never post database passwords, `APP_SECRET`, session cookies, private uploads, database dumps, or VPS credentials in an issue. Use the repository's private vulnerability-reporting feature for security problems; see [SECURITY.md](SECURITY.md).

There is no promise of immediate support. Reports are evidence to investigate and can be evaluated with AI assistance, tests, and reproducible examples.

## Production

Do not expose `cargo run` or the development login directly to the internet. A production installation should use:

- `DEVELOPMENT_MODE=false`;
- HTTPS and `SECURE_COOKIES=true`;
- a unique application-role password and `APP_SECRET`;
- a dedicated Linux user;
- systemd or another supervisor;
- Nginx serving static pages/media and proxying only dynamic routes;
- PostgreSQL and upload/database backups tested by restoring them;
- monitoring of health, readiness, and the publication count shown on the moderator dashboard;
- dependency updates, monitoring, and a rollback plan.

See [Ubuntu VPS deployment](docs/VPS_DEPLOYMENT.md).

## Authorship

Adelia's application code, project structure, documentation, and default generic banner were created with ChatGPT/OpenAI tools under the project owner's direction and testing. Human direction determined the product goals, behavior, visual requirements, and acceptance decisions.

AI-assisted contributions are welcome, but generated code is not exempt from review: contributors remain responsible for understanding, testing, and accurately describing their changes.

## License

Adelia's original code and original default banner are distributed under the [MIT License](LICENSE).

Bundled legacy themes and associated images retain their original vichan/Tinyboard notices and permissive terms. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and `licenses/vichan/`. Those names appear only where attribution is required.
