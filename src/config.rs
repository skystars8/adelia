use std::{env, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub public_base_url: String,
    pub site_title: String,
    pub site_subtitle: String,
    pub app_secret: String,
    pub generated_dir: PathBuf,
    pub upload_dir: PathBuf,
    pub template_dir: PathBuf,
    pub asset_dir: PathBuf,
    pub max_upload_bytes: usize,
    pub max_body_chars: usize,
    pub db_min_connections: u32,
    pub db_max_connections: u32,
    pub publisher_workers: usize,
    pub post_concurrency: usize,
    pub password_concurrency: usize,
    pub development_mode: bool,
    pub secure_cookies: bool,
    pub session_lifetime: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = required("DATABASE_URL")?;
        let bind_addr: SocketAddr = value("BIND_ADDR", "127.0.0.1:8080")
            .parse()
            .context("BIND_ADDR must be an IP address and port")?;
        let public_base_url = value("PUBLIC_BASE_URL", "http://127.0.0.1:8080")
            .trim_end_matches('/')
            .to_owned();
        let development_mode = boolean("DEVELOPMENT_MODE", false)?;
        if development_mode {
            let public_url = url::Url::parse(&public_base_url)
                .context("PUBLIC_BASE_URL must be an absolute URL")?;
            let local_host = matches!(
                public_url.host_str(),
                Some("127.0.0.1" | "localhost" | "::1")
            );
            if !bind_addr.ip().is_loopback() || !local_host {
                bail!(
                    "DEVELOPMENT_MODE is only allowed with a loopback bind address and localhost PUBLIC_BASE_URL"
                );
            }
        }
        let app_secret = required("APP_SECRET")?;
        if app_secret.len() < 32 {
            bail!("APP_SECRET must contain at least 32 characters");
        }

        let db_min_connections = parsed("DB_MIN_CONNECTIONS", 2)?;
        let db_max_connections = parsed("DB_MAX_CONNECTIONS", 20)?;
        if db_min_connections > db_max_connections || db_max_connections == 0 {
            bail!("DB connection limits are invalid");
        }
        let publisher_workers = parsed("PUBLISHER_WORKERS", 2usize)?;
        if !(1..=16).contains(&publisher_workers) {
            bail!("PUBLISHER_WORKERS must be between 1 and 16");
        }
        let post_concurrency = parsed("POST_CONCURRENCY", 32usize)?;
        if !(1..=1024).contains(&post_concurrency) {
            bail!("POST_CONCURRENCY must be between 1 and 1024");
        }
        let password_concurrency = parsed("PASSWORD_CONCURRENCY", 4usize)?;
        if !(1..=64).contains(&password_concurrency) {
            bail!("PASSWORD_CONCURRENCY must be between 1 and 64");
        }
        let required_connections = u32::try_from(publisher_workers)?
            .saturating_mul(2)
            .saturating_add(4);
        if db_max_connections < required_connections {
            bail!(
                "DB_MAX_CONNECTIONS must be at least (PUBLISHER_WORKERS * 2) + 4 ({required_connections})"
            );
        }

        Ok(Self {
            database_url,
            bind_addr,
            public_base_url,
            site_title: value("SITE_TITLE", "Adelia"),
            site_subtitle: value(
                "SITE_SUBTITLE",
                "Independent discussion for hobby and educational communities",
            ),
            app_secret,
            generated_dir: PathBuf::from(value("GENERATED_DIR", "generated")),
            upload_dir: PathBuf::from(value("UPLOAD_DIR", "data/uploads")),
            template_dir: PathBuf::from(value("TEMPLATE_DIR", "app_templates")),
            asset_dir: PathBuf::from(value("ASSET_DIR", "web/assets")),
            max_upload_bytes: parsed("MAX_UPLOAD_BYTES", 8 * 1024 * 1024)?,
            max_body_chars: parsed("MAX_BODY_CHARS", 20_000)?,
            db_min_connections,
            db_max_connections,
            publisher_workers,
            post_concurrency,
            password_concurrency,
            development_mode,
            secure_cookies: boolean("SECURE_COOKIES", false)?,
            session_lifetime: Duration::from_secs(parsed("SESSION_HOURS", 12u64)? * 3600),
        })
    }
}

fn required(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is required; copy .env.example to .env"))
}

fn value(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn parsed<T>(name: &str, default: T) -> Result<T>
where
    T: FromStr + ToString,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse()
        .with_context(|| format!("{name} has an invalid value"))
}

fn boolean(name: &str, default: bool) -> Result<bool> {
    match env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => bail!("{name} must be true or false"),
    }
}
