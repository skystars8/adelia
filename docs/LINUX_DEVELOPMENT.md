# Linux development

This guide targets Debian, Ubuntu, and similar distributions. Other distributions need equivalent Rust, PostgreSQL client/server, OpenSSL, compiler, and linker packages.

## Install prerequisites

```sh
sudo apt update
sudo apt install build-essential pkg-config libssl-dev postgresql postgresql-client openssl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

Confirm the tools:

```sh
rustc --version
cargo --version
psql --version
```

## First run

From the repository:

```sh
chmod +x dev.sh run.sh scripts/docker-entrypoint.sh
./dev.sh
```

On first run, `dev.sh`:

1. generates a unique `adelia_app` PostgreSQL password and `APP_SECRET`;
2. uses `sudo -u postgres` to create or update the role;
3. creates `adelia_dev` when absent;
4. writes `.env` with permission-restricting umask;
5. applies migrations and builds static pages;
6. starts Adelia on loopback.

Open <http://127.0.0.1:8080/>. Local moderator login is `admin` / `mod`.

Later runs reuse `.env` and start immediately. Stop with Ctrl+C.

## Development commands

```sh
cargo fmt --all
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run -- rebuild
cargo run -- serve
```

## Manual database setup

If the machine does not use `sudo -u postgres`, create a login and database with the administration method appropriate for that PostgreSQL installation:

```sql
CREATE ROLE adelia_app LOGIN PASSWORD 'replace-with-a-long-random-password';
CREATE DATABASE adelia_dev OWNER adelia_app;
```

Then:

```sh
cp .env.example .env
chmod 600 .env
```

Replace `DATABASE_URL` and `APP_SECRET`. Generate an application secret with:

```sh
openssl rand -hex 48
```

Never commit `.env`.

## Creating an administrator

For a non-development configuration:

```sh
export ADELIA_ADMIN_PASSWORD='choose-a-unique-password-with-12-or-more-characters'
cargo run -- admin admin
unset ADELIA_ADMIN_PASSWORD
```

## WSL

Adelia works in WSL, but PostgreSQL must be reachable from the environment where `cargo run` executes. The simplest arrangement is to install and run PostgreSQL inside the same WSL distribution. Use the Windows guide instead if both Rust and PostgreSQL are native Windows applications.

## Development versus production

`dev.sh` deliberately uses `DEVELOPMENT_MODE=true` and loopback addresses. It is not a VPS deployment script. Use the [VPS guide](VPS_DEPLOYMENT.md) for HTTPS, systemd, Nginx, a production administrator, backups, and `DEVELOPMENT_MODE=false`.
