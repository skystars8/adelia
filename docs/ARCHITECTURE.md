# Architecture

Adelia separates inexpensive public reads from transactional writes.

## Components

```text
Browser
  ├─ static GET ───────────────> Nginx or Rust ServeDir
  │                               ├─ generated HTML
  │                               ├─ CSS and vanilla JavaScript
  │                               └─ uploaded images and thumbnails
  │
  └─ writes, banners, /mod ────> Axum
                                  ├─ validation and security checks
                                  ├─ PostgreSQL transaction
                                  ├─ media storage
                                  └─ targeted static regeneration
```

Rust can serve every path during development. In production, the supplied Nginx configuration serves existing HTML/assets/media directly and proxies only dynamic routes or missing-file fallbacks.

## Runtime directories

- `app_templates/` contains MiniJinja templates loaded at process startup.
- `web/assets/` contains application CSS, vanilla JavaScript, themes, and banners.
- `data/uploads/` contains UUID-named originals and thumbnails.
- `generated/` contains the landing page, news, board pages, threads, catalogs, and archives.

Uploads and generated HTML are deliberately outside the Rust binary so they can be backed up, served by Nginx, and inspected.

## Static page lifecycle

At startup, Adelia:

1. reads configuration;
2. opens the bounded PostgreSQL pool;
3. applies embedded migrations;
4. loads templates;
5. rebuilds public pages;
6. binds the HTTP listener.

A successful post or moderation action commits its database transaction before rebuilding the affected pages. Per-board build locks serialize overlapping regeneration. Pages are rendered to temporary files and replaced, preventing readers from receiving half-written HTML.

The board builder produces:

- paginated board indexes;
- complete thread pages;
- a catalog;
- an archive index;
- landing-page board navigation;
- news pages.

Archived threads remain readable and become locked against replies.

## Dynamic routes

The principal dynamic surfaces are:

- `POST /post` for threads and replies;
- `POST /actions` for visitor reports;
- `/mod` and its authenticated moderation routes;
- `GET /banner/<board>` for random banner redirects;
- `GET /healthz` and `GET /readyz`;
- static-file fallback during direct Rust development.

The public delete-password feature is intentionally absent. Visitors report content; moderators and administrators delete it.

## Database model

PostgreSQL stores:

- boards and their posting controls;
- posts, thread relationships, status, metadata, and media paths;
- moderator accounts and expiring sessions;
- reports;
- news;
- moderation log entries;
- migration history.

Foreign keys cascade thread/reply and report cleanup. Transactions protect related changes such as approval, deletion, bumping, and archiving.

No address or address-derived columns remain in the active schema.

## Media safety

Adelia accepts JPEG, PNG, GIF, and WebP. Upload processing:

1. applies a request/body byte limit;
2. checks detected image format rather than trusting the filename;
3. reads dimensions before full decode;
4. rejects unsafe dimensions and decoded-pixel totals;
5. decodes with the Rust image library;
6. writes a UUID-named original and generated thumbnail.

Uploaded files are served from a separate URL prefix with content sniffing disabled.

## Authentication and request security

- Moderator passwords and optional board passwords use Argon2.
- Session identifiers are random; only keyed hashes are stored.
- Session cookies are HttpOnly, SameSite=Strict, and optionally Secure.
- Moderator writes require a CSRF token.
- Public and moderator form writes reject cross-site origins.
- The response stack supplies CSP, frame, MIME-sniffing, referrer, and permissions headers.
- Sensitive request headers are marked for tracing layers.

## Pressure handling

Ordinary static reads do not acquire a database connection. Dynamic work is bounded by:

- a fixed PostgreSQL pool;
- a short connection-acquisition timeout;
- request/body timeouts;
- non-identifying global, board, and thread write limits;
- per-board build serialization;
- upload size and image complexity limits.

This is designed to degrade writes before unbounded work consumes the host. It is not a substitute for load testing, operating-system limits, database monitoring, backups, or upstream denial-of-service protection.

## JavaScript

Public behavior uses small dependency-free scripts:

- remembered display name;
- stylesheet selection;
- frame/sidebar state;
- post form toggle and quick reply;
- quote insertion and hash highlighting;
- image expansion;
- catalog sorting and sizing;
- form confirmations.

No jQuery is required. Moderator pages use the dedicated application theme and do not load public theme substitutions.

## Privacy boundary

Adelia does not store client IP addresses or IP-derived hashes. The reference Nginx configuration disables access logs and clears common address-forwarding headers.

Operators must separately audit their VPS provider, CDN, firewall, reverse proxy, database, crash reporting, monitoring, and backups. Application-level privacy cannot prevent infrastructure outside Adelia from recording network metadata.

## Scaling direction

For a single community:

1. serve HTML/assets/media directly through Nginx;
2. measure CPU, memory, disk latency, PostgreSQL connections, locks, and regeneration time;
3. place cache/CDN capacity in front of static and media paths when needed;
4. scale PostgreSQL and write capacity only from measurements.

Do not increase `DB_MAX_CONNECTIONS` merely because traffic increased. Static traffic should not need those connections, and an oversized pool can overload PostgreSQL.
