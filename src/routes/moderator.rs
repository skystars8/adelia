use std::time::Duration;

use anyhow::Context;
use axum::{
    Form, Router,
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    AppState,
    error::{AppError, AppResult},
    media::{SavedUpload, remove_upload, save_image},
    models::{Board, Moderator, ModeratorSession, NewsEntry, ReportRow},
    security::{
        clean_text, cookie_value, enforce_same_origin, format_post_body, keyed_hash, password_hash,
        password_matches, random_token,
    },
};

const SESSION_COOKIE: &str = "adelia_mod";

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
pub struct CsrfForm {
    csrf: String,
    return_to: Option<String>,
}

#[derive(Deserialize)]
pub struct PostActionForm {
    csrf: String,
    return_to: Option<String>,
}

#[derive(Deserialize)]
pub struct BoardForm {
    csrf: String,
    slug: String,
    title: String,
    subtitle: String,
    description: String,
}

#[derive(Deserialize)]
pub struct BoardSettingsForm {
    csrf: String,
    read_only: Option<String>,
    require_approval: Option<String>,
    posting_password_enabled: Option<String>,
    #[serde(default)]
    posting_password: String,
}

#[derive(Debug, Serialize)]
struct BoardSettingsView {
    id: i64,
    slug: String,
    title: String,
    description: String,
    threads_per_page: i16,
    max_pages: i16,
    read_only: bool,
    require_approval: bool,
    posting_password_required: bool,
}

impl From<&Board> for BoardSettingsView {
    fn from(board: &Board) -> Self {
        Self {
            id: board.id,
            slug: board.slug.clone(),
            title: board.title.clone(),
            description: board.description.clone(),
            threads_per_page: board.threads_per_page,
            max_pages: board.max_pages,
            read_only: board.read_only,
            require_approval: board.require_approval,
            posting_password_required: board.posting_password_hash.is_some(),
        }
    }
}

#[derive(Deserialize)]
pub struct NewsForm {
    csrf: String,
    subject: String,
    body: String,
}

#[derive(Default)]
pub struct EditPostForm {
    csrf: String,
    name: String,
    subject: String,
    body: String,
    remove_image: Option<String>,
    replacement_image: Option<(String, Vec<u8>)>,
}

#[derive(Default, Deserialize)]
pub struct RecentPostsQuery {
    limit: Option<i64>,
}

#[derive(Debug, FromRow)]
struct ModerationTarget {
    id: i64,
    thread_id: Option<i64>,
    board_slug: String,
    file_path: Option<String>,
    thumb_path: Option<String>,
}

#[derive(Debug, FromRow, Serialize)]
struct RecentPost {
    id: i64,
    root_thread_id: i64,
    board_slug: String,
    name: String,
    tripcode: Option<String>,
    subject: String,
    body: String,
    created_at: chrono::DateTime<chrono::Utc>,
    is_thread: bool,
    locked: bool,
    sticky: bool,
    archived: bool,
    file_original_name: Option<String>,
    file_path: Option<String>,
    thumb_path: Option<String>,
    approved: bool,
}

#[derive(Debug, FromRow, Serialize)]
struct EditablePost {
    id: i64,
    root_thread_id: i64,
    board_slug: String,
    name: String,
    tripcode: Option<String>,
    subject: String,
    body: String,
    is_thread: bool,
    file_original_name: Option<String>,
    file_path: Option<String>,
    thumb_path: Option<String>,
    approved: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/mod/login", get(login_page).post(login))
        .route("/mod/logout", post(logout))
        .route("/mod", get(dashboard))
        .route("/mod/reports", get(reports_page))
        .route("/mod/pending", get(pending_posts_page))
        .route("/mod/threads", get(recent_posts_page))
        .route("/mod/boards", get(boards_page).post(create_board))
        .route("/mod/boards/{slug}", get(board_posts_page))
        .route("/mod/boards/{id}/settings", post(update_board_settings))
        .route("/mod/news", get(news_page).post(create_news))
        .route("/mod/reports/{id}/dismiss", post(dismiss_report))
        .route("/mod/posts/{id}/delete", post(delete_post))
        .route("/mod/posts/{id}/approve", post(approve_post))
        .route("/mod/posts/{id}/edit", get(edit_post_page).post(edit_post))
        .route("/mod/threads/{id}/toggle-lock", post(toggle_lock))
        .route("/mod/threads/{id}/toggle-sticky", post(toggle_sticky))
}

async fn login_page(State(state): State<AppState>) -> AppResult<Html<String>> {
    Ok(Html(state.templates.render(
        "moderator/login.html",
        context! { site_title => state.config.site_title, error => "", development_mode => state.config.development_mode },
    )?))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> AppResult<Response> {
    enforce_same_origin(&headers)?;
    let username = clean_text(&form.username, 64);
    let global_allowed =
        state
            .rate_limiter
            .check("mod-login:global".to_owned(), 200, Duration::from_secs(600));
    let username_allowed = state.rate_limiter.check(
        format!("mod-login-user:{}", username.to_ascii_lowercase()),
        10,
        Duration::from_secs(600),
    );
    if !global_allowed || !username_allowed {
        return Err(AppError::too_many(
            "Too many login attempts. Try again later.",
        ));
    }
    let moderator = sqlx::query_as::<_, Moderator>(
        "SELECT id, username, password_hash, role, active FROM moderators WHERE username = $1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await?;
    let valid = if let Some(moderator) = moderator.as_ref() {
        let password = form.password.clone();
        let hash = moderator.password_hash.clone();
        moderator.active
            && tokio::task::spawn_blocking(move || password_matches(&password, &hash))
                .await
                .unwrap_or(false)
    } else {
        false
    };
    if !valid {
        let html = state.templates.render(
            "moderator/login.html",
            context! { site_title => state.config.site_title, error => "Invalid username or password.", development_mode => state.config.development_mode },
        )?;
        return Ok((axum::http::StatusCode::UNAUTHORIZED, Html(html)).into_response());
    }
    let moderator = moderator.expect("validated moderator exists");
    let token = random_token(32);
    let token_hash = keyed_hash(&state.config.app_secret, token.as_bytes());
    let csrf = random_token(24);
    let expires_at = chrono::Utc::now()
        + chrono::Duration::from_std(state.config.session_lifetime)
            .context("invalid session lifetime")?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("DELETE FROM moderator_sessions WHERE expires_at <= NOW()")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO moderator_sessions (token_hash, moderator_id, csrf_token, expires_at) VALUES ($1,$2,$3,$4)")
        .bind(token_hash)
        .bind(moderator.id)
        .bind(csrf)
        .bind(expires_at)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("UPDATE moderators SET last_login_at = NOW() WHERE id = $1")
        .bind(moderator.id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    let mut response = Redirect::to("/mod").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(
            &token,
            state.config.session_lifetime.as_secs(),
            state.config.secure_cookies,
        ))
        .context("invalid session cookie")?,
    );
    Ok(response)
}

async fn dashboard(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let boards = sqlx::query_as::<_, Board>("SELECT * FROM boards ORDER BY position, slug")
        .fetch_all(&state.pool)
        .await?;
    let totals = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT (SELECT COUNT(*) FROM posts WHERE approved_at IS NOT NULL), (SELECT COUNT(*) FROM posts WHERE thread_id IS NULL AND approved_at IS NOT NULL), (SELECT COUNT(*) FROM reports WHERE status = 'open'), (SELECT COUNT(*) FROM posts WHERE approved_at IS NULL)",
    )
    .fetch_one(&state.pool)
    .await?;
    let html = state.templates.render(
        "moderator/dashboard.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => "dashboard",
            boards => boards,
            post_count => totals.0,
            thread_count => totals.1,
            report_count => totals.2,
            pending_count => totals.3,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn reports_page(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let reports = sqlx::query_as::<_, ReportRow>(
        "SELECT r.id, r.post_id, COALESCE(p.thread_id, p.id) AS thread_id, r.reason, r.created_at, b.slug AS board_slug, p.body AS post_body FROM reports r JOIN posts p ON p.id = r.post_id JOIN boards b ON b.id = p.board_id WHERE r.status = 'open' ORDER BY r.created_at DESC LIMIT 200",
    )
    .fetch_all(&state.pool)
    .await?;
    let html = state.templates.render(
        "moderator/reports.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => "reports",
            reports => reports,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn recent_posts_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RecentPostsQuery>,
) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let limit = match query.limit.unwrap_or(50) {
        25 => 25,
        100 => 100,
        _ => 50,
    };
    let posts = sqlx::query_as::<_, RecentPost>(
        "SELECT p.id, COALESCE(p.thread_id, p.id) AS root_thread_id, b.slug AS board_slug, p.name, p.tripcode, p.subject, LEFT(p.body, 1000) AS body, p.created_at, p.thread_id IS NULL AS is_thread, p.locked, p.sticky, p.archived_at IS NOT NULL AS archived, p.file_original_name, p.file_path, p.thumb_path, TRUE AS approved FROM posts p JOIN boards b ON b.id = p.board_id WHERE p.approved_at IS NOT NULL ORDER BY p.created_at DESC, p.id DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    let html = state.templates.render(
        "moderator/threads.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => "threads",
            posts => posts,
            limit => limit,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn pending_posts_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let posts = sqlx::query_as::<_, RecentPost>(
        "SELECT p.id, COALESCE(p.thread_id, p.id) AS root_thread_id, b.slug AS board_slug, p.name, p.tripcode, p.subject, LEFT(p.body, 1000) AS body, p.created_at, p.thread_id IS NULL AS is_thread, p.locked, p.sticky, p.archived_at IS NOT NULL AS archived, p.file_original_name, p.file_path, p.thumb_path, FALSE AS approved FROM posts p JOIN boards b ON b.id = p.board_id WHERE p.approved_at IS NULL ORDER BY p.created_at, p.id LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    let html = state.templates.render(
        "moderator/pending.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => "pending",
            posts => posts,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn boards_page(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let boards = sqlx::query_as::<_, Board>("SELECT * FROM boards ORDER BY position, slug")
        .fetch_all(&state.pool)
        .await?;
    let boards = boards
        .iter()
        .map(BoardSettingsView::from)
        .collect::<Vec<_>>();
    let html = state.templates.render(
        "moderator/boards.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => "boards",
            boards => boards,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn board_posts_page(
    State(state): State<AppState>,
    Path(board_slug): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let board = sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE slug = $1")
        .bind(&board_slug)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("That board does not exist."))?;
    let posts = sqlx::query_as::<_, RecentPost>(
        "SELECT p.id, COALESCE(p.thread_id, p.id) AS root_thread_id, b.slug AS board_slug, p.name, p.tripcode, p.subject, LEFT(p.body, 5000) AS body, p.created_at, p.thread_id IS NULL AS is_thread, p.locked, p.sticky, p.archived_at IS NOT NULL AS archived, p.file_original_name, p.file_path, p.thumb_path, p.approved_at IS NOT NULL AS approved FROM posts p JOIN boards b ON b.id = p.board_id WHERE b.id = $1 ORDER BY COALESCE(p.thread_id, p.id) DESC, p.thread_id NULLS FIRST, p.id LIMIT 500",
    )
    .bind(board.id)
    .fetch_all(&state.pool)
    .await?;
    let posting_password_required = board.posting_password_hash.is_some();
    let html = state.templates.render(
        "moderator/board.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => "boards",
            board => board,
            posting_password_required => posting_password_required,
            posts => posts,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn news_page(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let news = sqlx::query_as::<_, NewsEntry>(
        "SELECT id, subject, body, body_html, author_name, created_at FROM news ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(&state.pool)
    .await?;
    let html = state.templates.render(
        "moderator/news.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => "news",
            news => news,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn edit_post_page(
    State(state): State<AppState>,
    Path(post_id): Path<i64>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let Some(session) = page_session(&state, &headers).await? else {
        return Ok(Redirect::to("/mod/login").into_response());
    };
    let post = editable_post(&state, post_id)
        .await?
        .ok_or_else(|| AppError::not_found("That post does not exist."))?;
    let active = if post.approved { "threads" } else { "pending" };
    let html = state.templates.render(
        "moderator/edit_post.html",
        context! {
            site_title => state.config.site_title,
            username => &session.username,
            role => &session.role,
            csrf => &session.csrf_token,
            active => active,
            post => post,
            max_body_chars => state.config.max_body_chars,
        },
    )?;
    Ok(Html(html).into_response())
}

async fn edit_post(
    State(state): State<AppState>,
    Path(post_id): Path<i64>,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    let form = parse_edit_post(multipart, state.config.max_upload_bytes).await?;
    verify_csrf(&session, &form.csrf)?;

    let name = clean_text(&form.name, 35);
    let name = if name.is_empty() {
        "Anonymous".to_owned()
    } else {
        name
    };
    let subject = clean_text(&form.subject, 100);
    let body = clean_text(&form.body, state.config.max_body_chars);
    let body_html = format_post_body(&body);
    let initial = editable_post(&state, post_id)
        .await?
        .ok_or_else(|| AppError::not_found("That post does not exist."))?;
    let replacement = match form.replacement_image {
        Some((filename, bytes)) => Some(
            save_image(&state.config, &initial.board_slug, &filename, bytes)
                .await
                .map_err(|error| AppError::bad_request(error.to_string()))?,
        ),
        None => None,
    };
    let remove_image = replacement.is_none() && checkbox_checked(form.remove_image.as_deref());

    let transaction_result = async {
        let mut transaction = state.pool.begin().await?;
        let target = sqlx::query_as::<_, EditablePost>(
            "SELECT p.id, COALESCE(p.thread_id, p.id) AS root_thread_id, b.slug AS board_slug, p.name, p.tripcode, p.subject, p.body, p.thread_id IS NULL AS is_thread, p.file_original_name, p.file_path, p.thumb_path, p.approved_at IS NOT NULL AS approved FROM posts p JOIN boards b ON b.id = p.board_id WHERE p.id = $1 FOR UPDATE OF p",
        )
        .bind(post_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| AppError::not_found("That post does not exist."))?;

        if body.is_empty()
            && replacement.is_none()
            && (target.file_path.is_none() || remove_image)
        {
            return Err(AppError::bad_request(
                "A post must keep either a comment or an image.",
            ));
        }

        if let Some(upload) = replacement.as_ref() {
            sqlx::query(
                "UPDATE posts SET name = $1, subject = $2, body = $3, body_html = $4, file_original_name = $5, file_path = $6, thumb_path = $7, file_size = $8, file_mime = $9, image_width = $10, image_height = $11 WHERE id = $12",
            )
            .bind(&name)
            .bind(&subject)
            .bind(&body)
            .bind(&body_html)
            .bind(&upload.original_name)
            .bind(&upload.file_path)
            .bind(&upload.thumb_path)
            .bind(upload.file_size)
            .bind(&upload.mime)
            .bind(upload.width)
            .bind(upload.height)
            .bind(post_id)
            .execute(&mut *transaction)
            .await?;
        } else if remove_image {
            sqlx::query(
                "UPDATE posts SET name = $1, subject = $2, body = $3, body_html = $4, file_original_name = NULL, file_path = NULL, thumb_path = NULL, file_size = NULL, file_mime = NULL, image_width = NULL, image_height = NULL WHERE id = $5",
            )
            .bind(&name)
            .bind(&subject)
            .bind(&body)
            .bind(&body_html)
            .bind(post_id)
            .execute(&mut *transaction)
            .await?;
        } else {
            sqlx::query(
                "UPDATE posts SET name = $1, subject = $2, body = $3, body_html = $4 WHERE id = $5",
            )
            .bind(&name)
            .bind(&subject)
            .bind(&body)
            .bind(&body_html)
            .bind(post_id)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok::<_, AppError>(target)
    }
    .await;

    let target = match transaction_result {
        Ok(target) => target,
        Err(error) => {
            cleanup_replacement(&state, replacement.as_ref()).await;
            return Err(error);
        }
    };
    if remove_image || replacement.is_some() {
        remove_upload(
            &state.config.upload_dir,
            target.file_path.as_deref(),
            target.thumb_path.as_deref(),
        )
        .await;
    }
    if target.approved {
        state
            .builder
            .rebuild_thread(&target.board_slug, target.root_thread_id)
            .await
            .context("could not rebuild edited thread")?;
        state
            .builder
            .rebuild_board(&target.board_slug)
            .await
            .context("could not rebuild edited board")?;
    }
    log_action(
        &state,
        session.moderator_id,
        "edit post",
        &format!("/{}/ post {post_id}", target.board_slug),
    )
    .await?;

    let destination = if target.approved {
        format!(
            "/{}/res/{}.html#{post_id}",
            target.board_slug, target.root_thread_id
        )
    } else {
        "/mod/pending".to_owned()
    };
    Ok(Redirect::to(&destination))
}

async fn editable_post(state: &AppState, post_id: i64) -> AppResult<Option<EditablePost>> {
    sqlx::query_as::<_, EditablePost>(
        "SELECT p.id, COALESCE(p.thread_id, p.id) AS root_thread_id, b.slug AS board_slug, p.name, p.tripcode, p.subject, p.body, p.thread_id IS NULL AS is_thread, p.file_original_name, p.file_path, p.thumb_path, p.approved_at IS NOT NULL AS approved FROM posts p JOIN boards b ON b.id = p.board_id WHERE p.id = $1",
    )
    .bind(post_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(Into::into)
}

async fn parse_edit_post(
    mut multipart: Multipart,
    max_upload_bytes: usize,
) -> AppResult<EditPostForm> {
    let mut form = EditPostForm::default();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("The edit form could not be read."))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        if name == "replacement_image" {
            let filename = field.file_name().unwrap_or("image").to_owned();
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::bad_request("The replacement image could not be read."))?;
            if bytes.len() > max_upload_bytes {
                return Err(AppError::bad_request("The replacement image is too large."));
            }
            if !bytes.is_empty() {
                form.replacement_image = Some((filename, bytes.to_vec()));
            }
            continue;
        }
        let value = field
            .text()
            .await
            .map_err(|_| AppError::bad_request("An edit field could not be read."))?;
        match name.as_str() {
            "csrf" => form.csrf = value,
            "name" => form.name = value,
            "subject" => form.subject = value,
            "body" => form.body = value,
            "remove_image" => form.remove_image = Some(value),
            _ => {}
        }
    }
    Ok(form)
}

async fn cleanup_replacement(state: &AppState, upload: Option<&SavedUpload>) {
    if let Some(upload) = upload {
        remove_upload(
            &state.config.upload_dir,
            Some(&upload.file_path),
            Some(&upload.thumb_path),
        )
        .await;
    }
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> AppResult<Response> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        let token_hash = keyed_hash(&state.config.app_secret, token.as_bytes());
        sqlx::query("DELETE FROM moderator_sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&state.pool)
            .await?;
    }
    let mut response = Redirect::to("/mod/login").into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("adelia_mod=; Path=/mod; HttpOnly; SameSite=Strict; Max-Age=0"),
    );
    Ok(response)
}

async fn dismiss_report(
    State(state): State<AppState>,
    Path(report_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    let changed = sqlx::query("UPDATE reports SET status = 'dismissed', resolved_at = NOW(), resolved_by = $1 WHERE id = $2 AND status = 'open'")
        .bind(session.moderator_id)
        .bind(report_id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if changed == 0 {
        return Err(AppError::not_found("That open report does not exist."));
    }
    log_action(
        &state,
        session.moderator_id,
        "dismiss report",
        &format!("report {report_id}"),
    )
    .await?;
    Ok(Redirect::to("/mod/reports"))
}

async fn approve_post(
    State(state): State<AppState>,
    Path(post_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;

    let mut transaction = state.pool.begin().await?;
    let target = sqlx::query_as::<_, EditablePost>(
        "SELECT p.id, COALESCE(p.thread_id, p.id) AS root_thread_id, b.slug AS board_slug, p.name, p.tripcode, p.subject, p.body, p.thread_id IS NULL AS is_thread, p.file_original_name, p.file_path, p.thumb_path, p.approved_at IS NOT NULL AS approved FROM posts p JOIN boards b ON b.id = p.board_id WHERE p.id = $1 FOR UPDATE OF p",
    )
    .bind(post_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| AppError::not_found("That pending post does not exist."))?;
    if target.approved {
        return Err(AppError::bad_request("That post is already approved."));
    }
    let board = sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE slug = $1 FOR UPDATE")
        .bind(&target.board_slug)
        .fetch_one(&mut *transaction)
        .await?;
    sqlx::query("UPDATE posts SET approved_at = NOW(), approved_by = $1 WHERE id = $2")
        .bind(session.moderator_id)
        .bind(post_id)
        .execute(&mut *transaction)
        .await?;

    if !target.is_thread {
        let reply_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM posts WHERE thread_id = $1 AND approved_at IS NOT NULL",
        )
        .bind(target.root_thread_id)
        .fetch_one(&mut *transaction)
        .await?;
        if reply_count - 1 < i64::from(board.bump_limit) {
            sqlx::query("UPDATE posts SET bumped_at = NOW() WHERE id = $1 AND archived_at IS NULL")
                .bind(target.root_thread_id)
                .execute(&mut *transaction)
                .await?;
        }
    }

    let archived_ids = if target.is_thread {
        let active_limit = i64::from(board.threads_per_page) * i64::from(board.max_pages);
        sqlx::query_scalar::<_, i64>(
            "WITH overflow AS (SELECT id FROM posts WHERE board_id = $1 AND thread_id IS NULL AND archived_at IS NULL AND approved_at IS NOT NULL ORDER BY sticky DESC, bumped_at DESC, id DESC OFFSET $2) UPDATE posts SET archived_at = NOW(), locked = TRUE WHERE id IN (SELECT id FROM overflow) RETURNING id",
        )
        .bind(board.id)
        .bind(active_limit)
        .fetch_all(&mut *transaction)
        .await?
    } else {
        Vec::new()
    };
    transaction.commit().await?;

    state
        .builder
        .rebuild_thread(&target.board_slug, target.root_thread_id)
        .await
        .context("could not publish approved post")?;
    for archived_id in archived_ids {
        if archived_id != target.root_thread_id {
            state
                .builder
                .rebuild_thread(&target.board_slug, archived_id)
                .await
                .context("could not refresh archived thread")?;
        }
    }
    state
        .builder
        .rebuild_board(&target.board_slug)
        .await
        .context("could not refresh board after approval")?;
    log_action(
        &state,
        session.moderator_id,
        "approve post",
        &format!("/{}/ post {post_id}", target.board_slug),
    )
    .await?;
    let destination = moderator_return(form.return_to.as_deref(), Some(&target.board_slug));
    Ok(Redirect::to(&destination))
}

async fn delete_post(
    State(state): State<AppState>,
    Path(post_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<PostActionForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    let mut transaction = state.pool.begin().await?;
    let target = sqlx::query_as::<_, ModerationTarget>(
        "SELECT p.id, p.thread_id, b.slug AS board_slug, p.file_path, p.thumb_path FROM posts p JOIN boards b ON b.id = p.board_id WHERE p.id = $1 FOR UPDATE OF p",
    )
    .bind(post_id)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or_else(|| AppError::not_found("That post does not exist."))?;
    let files = if target.thread_id.is_none() {
        sqlx::query_as::<_, ModerationTarget>(
            "SELECT p.id, p.thread_id, b.slug AS board_slug, p.file_path, p.thumb_path FROM posts p JOIN boards b ON b.id = p.board_id WHERE p.id = $1 OR p.thread_id = $1 ORDER BY p.id FOR UPDATE OF p",
        )
        .bind(post_id)
        .fetch_all(&mut *transaction)
        .await?
    } else {
        vec![ModerationTarget {
            id: target.id,
            thread_id: target.thread_id,
            board_slug: target.board_slug.clone(),
            file_path: target.file_path.clone(),
            thumb_path: target.thumb_path.clone(),
        }]
    };
    sqlx::query("DELETE FROM posts WHERE id = $1")
        .bind(post_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    for file in &files {
        remove_upload(
            &state.config.upload_dir,
            file.file_path.as_deref(),
            file.thumb_path.as_deref(),
        )
        .await;
    }
    let thread_id = target.thread_id.unwrap_or(target.id);
    if target.thread_id.is_none() {
        state
            .builder
            .remove_thread_page(&target.board_slug, thread_id)
            .await;
    } else {
        state
            .builder
            .rebuild_thread(&target.board_slug, thread_id)
            .await
            .context("could not rebuild moderated thread")?;
    }
    state
        .builder
        .rebuild_board(&target.board_slug)
        .await
        .context("could not rebuild moderated board")?;
    log_action(
        &state,
        session.moderator_id,
        "delete post",
        &format!("post {post_id}"),
    )
    .await?;
    let destination = moderator_return(form.return_to.as_deref(), Some(&target.board_slug));
    Ok(Redirect::to(&destination))
}

async fn toggle_lock(
    State(state): State<AppState>,
    Path(thread_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    let board_slug = sqlx::query_scalar::<_, String>(
        "UPDATE posts p SET locked = NOT p.locked FROM boards b WHERE p.id = $1 AND p.thread_id IS NULL AND p.approved_at IS NOT NULL AND b.id = p.board_id RETURNING b.slug",
    )
    .bind(thread_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("That thread does not exist."))?;
    state.builder.rebuild_thread(&board_slug, thread_id).await?;
    state.builder.rebuild_board(&board_slug).await?;
    log_action(
        &state,
        session.moderator_id,
        "toggle thread lock",
        &format!("thread {thread_id}"),
    )
    .await?;
    let destination = moderator_return(form.return_to.as_deref(), Some(&board_slug));
    Ok(Redirect::to(&destination))
}

async fn toggle_sticky(
    State(state): State<AppState>,
    Path(thread_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CsrfForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    let board_slug = sqlx::query_scalar::<_, String>(
        "UPDATE posts p SET sticky = NOT p.sticky FROM boards b WHERE p.id = $1 AND p.thread_id IS NULL AND p.approved_at IS NOT NULL AND b.id = p.board_id RETURNING b.slug",
    )
    .bind(thread_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("That thread does not exist."))?;
    state.builder.rebuild_thread(&board_slug, thread_id).await?;
    state.builder.rebuild_board(&board_slug).await?;
    log_action(
        &state,
        session.moderator_id,
        "toggle thread sticky",
        &format!("thread {thread_id}"),
    )
    .await?;
    let destination = moderator_return(form.return_to.as_deref(), Some(&board_slug));
    Ok(Redirect::to(&destination))
}

async fn update_board_settings(
    State(state): State<AppState>,
    Path(board_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<BoardSettingsForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    let read_only = checkbox_checked(form.read_only.as_deref());
    let require_approval = checkbox_checked(form.require_approval.as_deref());
    let posting_password_enabled = checkbox_checked(form.posting_password_enabled.as_deref());
    let board = sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE id = $1")
        .bind(board_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("That board does not exist."))?;
    let posting_password_hash = if !posting_password_enabled {
        None
    } else if form.posting_password.trim().is_empty() {
        Some(board.posting_password_hash.clone().ok_or_else(|| {
            AppError::bad_request("Enter a shared password when enabling protected posting.")
        })?)
    } else {
        let posting_password = form.posting_password.trim();
        let password_chars = posting_password.chars().count();
        if password_chars < 12 {
            return Err(AppError::bad_request(
                "A shared board password must contain at least 12 characters.",
            ));
        }
        if password_chars > 128 {
            return Err(AppError::bad_request(
                "A shared board password cannot exceed 128 characters.",
            ));
        }
        let posting_password = posting_password.to_owned();
        Some(
            tokio::task::spawn_blocking(move || password_hash(&posting_password))
                .await
                .context("board password hashing task failed")??,
        )
    };
    let slug = sqlx::query_scalar::<_, String>(
        "UPDATE boards SET read_only = $1, require_approval = $2, posting_password_hash = $3, updated_at = NOW() WHERE id = $4 RETURNING slug",
    )
    .bind(read_only)
    .bind(require_approval)
    .bind(posting_password_hash.as_deref())
    .bind(board_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("That board does not exist."))?;
    state.builder.rebuild_board(&slug).await?;
    let thread_ids = sqlx::query_scalar::<_, i64>(
        "SELECT p.id FROM posts p JOIN boards b ON b.id = p.board_id WHERE b.slug = $1 AND p.thread_id IS NULL AND p.approved_at IS NOT NULL ORDER BY p.id",
    )
    .bind(&slug)
    .fetch_all(&state.pool)
    .await?;
    for thread_id in thread_ids {
        state.builder.rebuild_thread(&slug, thread_id).await?;
    }
    log_action(
        &state,
        session.moderator_id,
        "update board settings",
        &format!("/{slug}/ locked={read_only} approval_required={require_approval} posting_password_required={posting_password_enabled}"),
    )
    .await?;
    Ok(Redirect::to("/mod/boards"))
}

async fn create_board(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<BoardForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    if session.role != "admin" {
        return Err(AppError::forbidden(
            "Only an administrator can create boards.",
        ));
    }
    let slug = form.slug.trim().to_ascii_lowercase();
    if slug.is_empty()
        || slug.len() > 32
        || !slug.chars().enumerate().all(|(index, character)| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || (index > 0 && matches!(character, '_' | '-'))
        })
    {
        return Err(AppError::bad_request(
            "Board slugs may contain lowercase letters, numbers, dashes, and underscores.",
        ));
    }
    let title = clean_text(&form.title, 100);
    if title.is_empty() {
        return Err(AppError::bad_request("A board title is required."));
    }
    let subtitle = clean_text(&form.subtitle, 200);
    let description = clean_text(&form.description, 2000);
    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM boards WHERE slug = $1)")
            .bind(&slug)
            .fetch_one(&state.pool)
            .await?;
    if exists {
        return Err(AppError::bad_request(
            "A board with that slug already exists.",
        ));
    }
    sqlx::query(
        "INSERT INTO boards (slug, title, subtitle, description, position) VALUES ($1,$2,$3,$4,COALESCE((SELECT MAX(position) + 10 FROM boards), 10))",
    )
    .bind(&slug)
    .bind(&title)
    .bind(&subtitle)
    .bind(&description)
    .execute(&state.pool)
    .await?;
    state.builder.rebuild_board(&slug).await?;
    state.builder.rebuild_home().await?;
    log_action(
        &state,
        session.moderator_id,
        "create board",
        &format!("/{slug}/"),
    )
    .await?;
    Ok(Redirect::to("/mod/boards"))
}

async fn create_news(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<NewsForm>,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let session = require_session(&state, &headers).await?;
    verify_csrf(&session, &form.csrf)?;
    let subject = clean_text(&form.subject, 150);
    let body = clean_text(&form.body, 10_000);
    if subject.is_empty() || body.is_empty() {
        return Err(AppError::bad_request(
            "News needs both a subject and a body.",
        ));
    }
    let body_html = format_post_body(&body);
    sqlx::query("INSERT INTO news (subject, body, body_html, author_id, author_name) VALUES ($1,$2,$3,$4,$5)")
        .bind(&subject)
        .bind(&body)
        .bind(&body_html)
        .bind(session.moderator_id)
        .bind(&session.username)
        .execute(&state.pool)
        .await?;
    state.builder.rebuild_home().await?;
    log_action(&state, session.moderator_id, "publish news", &subject).await?;
    Ok(Redirect::to("/mod/news"))
}

fn moderator_return(value: Option<&str>, board_slug: Option<&str>) -> String {
    match value {
        Some("reports") => "/mod/reports".to_owned(),
        Some("pending") => "/mod/pending".to_owned(),
        Some("board") => board_slug
            .map(|slug| format!("/mod/boards/{slug}"))
            .unwrap_or_else(|| "/mod/boards".to_owned()),
        _ => "/mod/threads".to_owned(),
    }
}

fn checkbox_checked(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("on") || value == "1"
    })
}

async fn page_session(
    state: &AppState,
    headers: &HeaderMap,
) -> AppResult<Option<ModeratorSession>> {
    match require_session(state, headers).await {
        Ok(session) => Ok(Some(session)),
        Err(AppError::Public { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

async fn require_session(state: &AppState, headers: &HeaderMap) -> AppResult<ModeratorSession> {
    let token = cookie_value(headers, SESSION_COOKIE)
        .ok_or_else(|| AppError::forbidden("Moderator sign-in is required."))?;
    let token_hash = keyed_hash(&state.config.app_secret, token.as_bytes());
    let row = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT m.id, m.username, m.role, s.csrf_token FROM moderator_sessions s JOIN moderators m ON m.id = s.moderator_id WHERE s.token_hash = $1 AND s.expires_at > NOW() AND m.active",
    )
    .bind(token_hash)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::forbidden("The moderator session has expired. Sign in again."))?;
    Ok(ModeratorSession {
        moderator_id: row.0,
        username: row.1,
        role: row.2,
        csrf_token: row.3,
    })
}

fn verify_csrf(session: &ModeratorSession, provided: &str) -> AppResult<()> {
    if session.csrf_token == provided {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "The form expired. Reload the moderator page and try again.",
        ))
    }
}

fn session_cookie(token: &str, max_age: u64, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/mod; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    )
}

async fn log_action(
    state: &AppState,
    moderator_id: i64,
    action: &str,
    detail: &str,
) -> AppResult<()> {
    sqlx::query("INSERT INTO moderation_log (moderator_id, action, detail) VALUES ($1,$2,$3)")
        .bind(moderator_id)
        .bind(action)
        .bind(detail)
        .execute(&state.pool)
        .await?;
    Ok(())
}
