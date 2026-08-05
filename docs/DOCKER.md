# Docker development

The supplied `compose.yml` is a self-contained local development environment. It builds Adelia, starts PostgreSQL, keeps database/uploads/generated pages in named volumes, and publishes the site only on `127.0.0.1:8080`.

It uses visible development credentials and `SECURE_COOKIES=false`. Do not deploy this Compose file to a public server.

## Requirements

- Docker Desktop on Windows/macOS, or Docker Engine with the Compose plugin on Linux
- At least 2 GB of free memory for the first Rust build

## Start

```sh
docker compose up --build -d
docker compose ps
```

Wait until both services are healthy, then open <http://127.0.0.1:8080/>.

## Create the first administrator

The Docker configuration uses `DEVELOPMENT_MODE=false` because the application listens on the container network. Create an administrator explicitly.

Linux/macOS:

```sh
export ADELIA_ADMIN_PASSWORD='choose-a-long-local-password'
docker compose exec --user adelia -e ADELIA_ADMIN_PASSWORD app adelia admin admin
unset ADELIA_ADMIN_PASSWORD
```

PowerShell:

```powershell
$env:ADELIA_ADMIN_PASSWORD = 'choose-a-long-local-password'
docker compose exec --user adelia -e ADELIA_ADMIN_PASSWORD app adelia admin admin
Remove-Item Env:ADELIA_ADMIN_PASSWORD
```

Sign in at <http://127.0.0.1:8080/mod>.

## Logs and health

```sh
docker compose ps
docker compose logs --tail=200 app
docker compose logs --tail=200 database
```

Application endpoints:

- `/healthz` confirms that the process is alive.
- `/readyz` confirms that PostgreSQL is reachable.

## Stop and restart

```sh
docker compose stop
docker compose start
```

Stop and remove containers while preserving named volumes:

```sh
docker compose down
```

## Rebuild after source changes

```sh
docker compose up --build -d
```

The Rust build is cached in Docker layers when its inputs have not changed.

## Data

Compose creates three named volumes:

- `adelia_database-data` for PostgreSQL;
- `adelia_uploaded-media` for original images and thumbnails;
- `adelia_generated-pages` for generated HTML.

The exact prefix may vary with the Compose project name.

### Completely reset local Docker data

The following command permanently removes the local Docker database, posts, moderator accounts, uploads, and generated pages:

```sh
docker compose down --volumes
```

Use it only when intentionally starting over. It cannot be undone without a backup.

## Why the container starts as root

The entrypoint briefly creates and corrects ownership on mounted runtime directories, then immediately uses `gosu` to run Adelia as the unprivileged `adelia` user. The application process itself does not run as root.

## Production

The image can be a starting point for a custom deployment, but the supplied Compose configuration is intentionally local. Production requires unique secrets, TLS, secure cookies, restricted networking, backups, monitoring, and a deliberate reverse-proxy configuration. The documented and tested reference production layout is [Ubuntu with systemd and Nginx](VPS_DEPLOYMENT.md).
