use std::{path::Path, sync::Arc, time::Duration};

use adelia::{
    AppState, config::Config, publisher, rate_limit::RateLimiter, render::Templates, routes,
    security::password_hash, static_builder::StaticBuilder,
};
use anyhow::{Context, Result, bail};
use axum::{
    Router,
    body::Body,
    extract::{DefaultBodyLimit, Request},
    http::{HeaderName, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::Response,
};
use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    catch_panic::CatchPanicLayer, compression::CompressionLayer,
    sensitive_headers::SetSensitiveRequestHeadersLayer, services::ServeDir, timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "adelia", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Serve,
    Rebuild,
    Admin {
        #[arg(default_value = "admin")]
        username: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("adelia=info,tower_http=info")),
        )
        .compact()
        .init();

    let cli = Cli::parse();
    let config = Arc::new(Config::from_env()?);
    tokio::fs::create_dir_all(&config.generated_dir).await?;
    tokio::fs::create_dir_all(&config.upload_dir).await?;

    let pool = PgPoolOptions::new()
        .min_connections(config.db_min_connections)
        .max_connections(config.db_max_connections)
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_secs(600))
        .connect(&config.database_url)
        .await
        .context("could not connect to PostgreSQL")?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("could not apply database migrations")?;
    if config.development_mode {
        ensure_development_admin(&pool).await?;
    }

    let templates = Arc::new(Templates::load(config.template_dir.clone())?);
    let builder = Arc::new(StaticBuilder::new(
        pool.clone(),
        config.clone(),
        templates.clone(),
    ));

    match cli.command.unwrap_or(Command::Serve) {
        Command::Admin { username } => create_admin(&pool, &username).await,
        Command::Rebuild => {
            builder.rebuild_all().await?;
            tracing::info!("all static pages rebuilt");
            Ok(())
        }
        Command::Serve => {
            if !config.generated_dir.join("index.html").is_file() {
                builder
                    .rebuild_all()
                    .await
                    .context("initial static build failed")?;
            }
            serve(pool, config, templates, builder).await
        }
    }
}

async fn create_admin(pool: &sqlx::PgPool, username: &str) -> Result<()> {
    let username = username.trim();
    if username.is_empty() || username.len() > 64 {
        bail!("administrator username must contain 1 to 64 characters");
    }
    let password = std::env::var("ADELIA_ADMIN_PASSWORD")
        .context("set ADELIA_ADMIN_PASSWORD for this command")?;
    if password.chars().count() < 12 {
        bail!("administrator password must contain at least 12 characters");
    }
    let hash = tokio::task::spawn_blocking(move || password_hash(&password))
        .await
        .context("password worker stopped")??;
    sqlx::query(
        "INSERT INTO moderators (username, password_hash, role) VALUES ($1,$2,'admin') ON CONFLICT (username) DO UPDATE SET password_hash = EXCLUDED.password_hash, role = 'admin', active = TRUE",
    )
    .bind(username)
    .bind(hash)
    .execute(pool)
    .await?;
    tracing::info!(username, "administrator created or updated");
    Ok(())
}

async fn ensure_development_admin(pool: &sqlx::PgPool) -> Result<()> {
    let hash = tokio::task::spawn_blocking(|| password_hash("mod"))
        .await
        .context("password worker stopped")??;
    sqlx::query(
        "INSERT INTO moderators (username, password_hash, role) VALUES ('admin',$1,'admin') ON CONFLICT (username) DO UPDATE SET password_hash = EXCLUDED.password_hash, role = 'admin', active = TRUE",
    )
    .bind(hash)
    .execute(pool)
    .await?;
    tracing::info!("local development moderator is admin / mod");
    Ok(())
}

async fn serve(
    pool: sqlx::PgPool,
    config: Arc<Config>,
    templates: Arc<Templates>,
    builder: Arc<StaticBuilder>,
) -> Result<()> {
    let banner_files = Arc::new(load_banner_files(&config.asset_dir).await?);
    let publisher_notify = Arc::new(tokio::sync::Notify::new());
    let state = AppState::new(
        pool.clone(),
        config.clone(),
        templates,
        builder.clone(),
        Arc::new(RateLimiter::default()),
        banner_files,
        publisher_notify.clone(),
    );
    let sensitive = [header::AUTHORIZATION, header::COOKIE];
    let app = Router::new()
        .merge(routes::router())
        .nest_service("/assets", ServeDir::new(&config.asset_dir))
        .nest_service("/media", ServeDir::new(&config.upload_dir))
        .fallback_service(
            ServeDir::new(&config.generated_dir).append_index_html_on_directories(true),
        )
        .layer(DefaultBodyLimit::max(config.max_upload_bytes + 1024 * 1024))
        .layer(SetSensitiveRequestHeadersLayer::new(sensitive))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(CompressionLayer::new())
        .layer(CatchPanicLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(security_headers))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("could not listen on {}", config.bind_addr))?;
    let released_claims = publisher::release_claims(&pool).await?;
    if released_claims > 0 {
        tracing::warn!(
            released_claims,
            "released publication jobs left claimed by an earlier process"
        );
    }
    let publisher_workers = publisher::spawn_workers(
        config.publisher_workers,
        pool,
        builder,
        publisher_notify.clone(),
    );
    publisher_notify.notify_waiters();
    tracing::info!(
        workers = config.publisher_workers,
        "static publishers started"
    );
    tracing::info!(address = %config.bind_addr, "Adelia is ready");
    let server_result = axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await;
    for worker in publisher_workers {
        worker.abort();
    }
    server_result?;
    Ok(())
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("SAMEORIGIN"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-src 'self'; frame-ancestors 'self'; form-action 'self'; base-uri 'self'; object-src 'none'",
        ),
    );
    let cache = if path.starts_with("/banner/") {
        "no-store"
    } else if path.starts_with("/media/") {
        "public, max-age=31536000, immutable"
    } else if path.starts_with("/assets/") {
        "public, max-age=3600"
    } else {
        "no-cache"
    };
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

async fn load_banner_files(asset_dir: &Path) -> Result<Vec<String>> {
    let banner_dir = asset_dir.join("banners");
    let mut entries = match tokio::fs::read_dir(&banner_dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(path = %banner_dir.display(), "no board banner directory found");
            return Ok(Vec::new());
        }
        Err(error) => return Err(error).context("could not read the board banner directory"),
    };
    let mut files = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let Some(filename) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let extension = Path::new(&filename)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
            files.push(filename);
        }
    }
    files.sort_unstable();
    tracing::info!(count = files.len(), "board banners indexed");
    Ok(files)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("could not install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("could not install terminate handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown requested");
}
