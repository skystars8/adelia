# Windows development

This guide creates an isolated local PostgreSQL database and runs Adelia directly with Cargo.

## Requirements

- 64-bit Windows 10 or 11
- [Rust installed through rustup](https://rustup.rs/)
- PostgreSQL with its command-line tools
- PowerShell 5.1 or newer

Confirm Rust before starting:

```powershell
rustc --version
cargo --version
```

The setup script searches the highest numbered installation under `C:\Program Files\PostgreSQL`.

## First run

From File Explorer, double-click `dev.bat`. From PowerShell, the equivalent is:

```powershell
Set-Location C:\path\to\Adelia
.\dev.bat
```

On the first run:

1. `scripts/setup-windows.ps1` asks for the PostgreSQL `postgres` administrator password.
2. It creates or updates the loopback-only `adelia_app` login with a newly generated password.
3. It creates `adelia_dev` if needed.
4. It writes an ignored `.env` containing the application-role password and a random `APP_SECRET`.
5. Adelia applies migrations and creates its static pages.
6. `dev.bat` starts the Rust development server.

The PostgreSQL administrator password exists only in the setup process environment and is removed before the script exits.

Open <http://127.0.0.1:8080/>. Local moderator login is `admin` / `mod`.

Stop the server with Ctrl+C.

## Later runs

`dev.bat` sees the existing `.env` and starts Adelia without asking for the PostgreSQL administrator password again.

`run.bat` is a compatibility alias for `dev.bat`. `postgres.bat` reruns the database/rebuild check without starting the persistent server.

## Work cycle

```powershell
cargo fmt --all
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo run -- serve
```

Templates are loaded when Adelia starts. Restart after changing templates or Rust code. Run this after changing static display data:

```powershell
cargo run -- rebuild
```

## Creating a non-development administrator

When `DEVELOPMENT_MODE=false`, create an administrator with a temporary environment variable:

```powershell
$env:ADELIA_ADMIN_PASSWORD = 'choose-a-unique-password-with-12-or-more-characters'
cargo run -- admin admin
Remove-Item Env:ADELIA_ADMIN_PASSWORD
```

Do not put the administrator password in `.env`.

## Changing the local site name

Edit only these values in `.env`:

```dotenv
SITE_TITLE=My Community
SITE_SUBTITLE=A short description
```

Restart Adelia. The engine and executable remain named Adelia.

## Troubleshooting

### Cargo is not found

Install Rust from rustup, close all terminals, and open a new PowerShell window so the updated PATH is available.

### PostgreSQL is not found

Confirm PostgreSQL is installed under `C:\Program Files\PostgreSQL\<version>\bin`. If it is installed elsewhere, edit the discovery section at the top of `scripts/setup-windows.ps1` or create the database manually using `.env.example`.

### Authentication fails during setup

The requested password is the PostgreSQL `postgres` administrator password, not the Adelia moderator password.

### Port 8080 is occupied

Find the listener:

```powershell
Get-NetTCPConnection -LocalPort 8080 -State Listen
```

Stop the known conflicting development process or choose another loopback port in both `BIND_ADDR` and `PUBLIC_BASE_URL`.

### The database configuration changed

Do not casually delete `.env`: it contains the password for the existing `adelia_app` role and the stable secret used by sessions and secure tripcodes. Back it up before changing credentials.

### A build works but pages look old

```powershell
cargo run -- rebuild
```

Then hard-refresh the browser. Static pages under `generated/` are runtime output and are intentionally not committed.
