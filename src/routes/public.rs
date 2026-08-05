use std::time::Duration;

use anyhow::Context;
use axum::{
    body::Bytes,
    extract::{Multipart, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use rand::seq::SliceRandom;
use serde::Serialize;

use crate::{
    AppState,
    error::{AppError, AppResult},
    media::{SavedUpload, remove_upload, save_image},
    models::Board,
    security::{
        clean_text, enforce_same_origin, format_post_body, password_matches, secure_trip_identity,
    },
};

#[derive(Default)]
struct IncomingPost {
    board: String,
    thread: Option<i64>,
    name: String,
    subject: String,
    body: String,
    board_password: String,
    website: String,
    upload: Option<(String, Vec<u8>)>,
}

#[derive(Serialize)]
struct Health<'a> {
    status: &'a str,
    version: &'a str,
}

pub async fn health() -> impl IntoResponse {
    axum::Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn ready(State(state): State<AppState>) -> Response {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(1) => (StatusCode::OK, "ready").into_response(),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response(),
    }
}

pub async fn random_banner(
    State(state): State<AppState>,
    Path(board_slug): Path<String>,
) -> AppResult<Response> {
    if board_slug.is_empty()
        || board_slug.len() > 32
        || !board_slug
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return Err(AppError::not_found("That board banner does not exist."));
    }
    let filename = state
        .banner_files
        .choose(&mut rand::thread_rng())
        .ok_or_else(|| AppError::not_found("No board banners are installed."))?;
    let encoded = url::form_urlencoded::byte_serialize(filename.as_bytes()).collect::<String>();
    let mut response = Redirect::temporary(&format!("/assets/banners/{encoded}")).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn enforce_board_posting_password(
    state: &AppState,
    board: &Board,
    submitted: &str,
) -> AppResult<()> {
    let Some(hash) = board.posting_password_hash.as_ref() else {
        return Ok(());
    };
    let submitted = submitted.trim();
    if submitted.chars().count() > 128 {
        return Err(AppError::forbidden("The board password is incorrect."));
    }
    if !state.rate_limiter.check(
        format!("board-password:{}", board.slug),
        120,
        Duration::from_secs(60),
    ) {
        return Err(AppError::too_many(
            "Too many password attempts for this board. Wait a moment and try again.",
        ));
    }

    let submitted = submitted.to_owned();
    let hash = hash.clone();
    let valid = tokio::task::spawn_blocking(move || password_matches(&submitted, &hash))
        .await
        .unwrap_or(false);
    if !valid {
        return Err(AppError::forbidden("The board password is incorrect."));
    }
    Ok(())
}

pub async fn create_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    if !state
        .rate_limiter
        .check("post:global".to_owned(), 2_000, Duration::from_secs(60))
    {
        return Err(AppError::too_many(
            "The site is receiving too many posts. Wait a moment and try again.",
        ));
    }
    let incoming = parse_post(multipart, state.config.max_upload_bytes).await?;
    if !incoming.website.is_empty() {
        return Err(AppError::bad_request("The post could not be accepted."));
    }

    let board = sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE slug = $1")
        .bind(&incoming.board)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("That board does not exist."))?;
    if board.read_only {
        return Err(AppError::forbidden("This board is currently read-only."));
    }
    enforce_board_posting_password(&state, &board, &incoming.board_password).await?;

    if !state.rate_limiter.check(
        format!("post-board:{}", board.slug),
        600,
        Duration::from_secs(60),
    ) {
        return Err(AppError::too_many(
            "This board is receiving too many posts. Wait a moment and try again.",
        ));
    }
    let (scope_key, scope_limit, scope_window) = match incoming.thread {
        Some(thread_id) => (
            format!("post-thread:{thread_id}"),
            300,
            Duration::from_secs(60),
        ),
        None => (
            format!("new-thread-board:{}", board.slug),
            120,
            Duration::from_secs(600),
        ),
    };
    if !state
        .rate_limiter
        .check(scope_key, scope_limit, scope_window)
    {
        return Err(AppError::too_many(
            "This discussion is receiving too many new posts. Wait a moment and try again.",
        ));
    }

    create_validated_post(state, board, incoming).await
}

async fn parse_post(mut multipart: Multipart, max_upload_bytes: usize) -> AppResult<IncomingPost> {
    let mut post = IncomingPost::default();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("The submitted form could not be read."))?
    {
        let name = field.name().unwrap_or_default().to_owned();
        if name == "file" {
            let filename = field.file_name().unwrap_or("image").to_owned();
            let bytes = field
                .bytes()
                .await
                .map_err(|_| AppError::bad_request("The uploaded image could not be read."))?;
            if bytes.len() > max_upload_bytes {
                return Err(AppError::bad_request("The uploaded image is too large."));
            }
            if !bytes.is_empty() {
                post.upload = Some((filename, bytes.to_vec()));
            }
            continue;
        }
        let value = field
            .text()
            .await
            .map_err(|_| AppError::bad_request("A submitted field could not be read."))?;
        match name.as_str() {
            "board" => post.board = value,
            "thread" => post.thread = value.parse().ok(),
            "name" => post.name = value,
            "subject" => post.subject = value,
            "body" => post.body = value,
            "board_password" => post.board_password = value,
            "website" => post.website = value,
            _ => {}
        }
    }
    Ok(post)
}

async fn create_validated_post(
    state: AppState,
    board: Board,
    incoming: IncomingPost,
) -> AppResult<Redirect> {
    let (name, tripcode) = secure_trip_identity(&state.config.app_secret, &incoming.name)?;
    let subject = clean_text(&incoming.subject, 100);
    let body = clean_text(&incoming.body, state.config.max_body_chars);
    if body.is_empty() && incoming.upload.is_none() {
        return Err(AppError::bad_request("Write a comment or attach an image."));
    }
    let body_html = format_post_body(&body);
    let saved_upload = match incoming.upload {
        Some((filename, bytes)) => Some(
            save_image(&state.config, &board.slug, &filename, bytes)
                .await
                .map_err(|error| AppError::bad_request(error.to_string()))?,
        ),
        None => None,
    };

    let transaction_result = async {
        let mut transaction = state.pool.begin().await?;
        let live_settings = sqlx::query_as::<_, (bool, bool, Option<String>)>(
            "SELECT read_only, require_approval, posting_password_hash FROM boards WHERE id = $1 FOR SHARE",
        )
        .bind(board.id)
        .fetch_one(&mut *transaction)
        .await?;
        if live_settings.0 {
            return Err(AppError::forbidden("This board is currently locked."));
        }
        if live_settings.2.as_deref() != board.posting_password_hash.as_deref() {
            return Err(AppError::forbidden(
                "Board posting settings changed. Reload the page and try again.",
            ));
        }
        let require_approval = live_settings.1;
        let (thread_id, reply_count) = if let Some(thread_id) = incoming.thread {
            let status = sqlx::query_as::<_, (bool, Option<chrono::DateTime<chrono::Utc>>)>(
                "SELECT locked, archived_at FROM posts WHERE id = $1 AND board_id = $2 AND thread_id IS NULL AND approved_at IS NOT NULL FOR UPDATE",
            )
            .bind(thread_id)
            .bind(board.id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| AppError::not_found("That thread does not exist."))?;
            if status.0 || status.1.is_some() {
                return Err(AppError::forbidden("That thread is locked or archived."));
            }
            let reply_count =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM posts WHERE thread_id = $1")
                    .bind(thread_id)
                    .fetch_one(&mut *transaction)
                    .await?;
            if reply_count >= i64::from(board.max_replies) {
                return Err(AppError::forbidden("That thread has reached its reply limit."));
            }
            (Some(thread_id), reply_count)
        } else {
            (None, 0)
        };

        let upload = saved_upload.as_ref();
        let post_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO posts (board_id, thread_id, name, tripcode, subject, body, body_html, file_original_name, file_path, thumb_path, file_size, file_mime, image_width, image_height, approved_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,CASE WHEN $15 THEN NULL ELSE NOW() END) RETURNING id",
        )
        .bind(board.id)
        .bind(thread_id)
        .bind(&name)
        .bind(tripcode.as_deref())
        .bind(&subject)
        .bind(&body)
        .bind(&body_html)
        .bind(upload.map(|item| item.original_name.as_str()))
        .bind(upload.map(|item| item.file_path.as_str()))
        .bind(upload.map(|item| item.thumb_path.as_str()))
        .bind(upload.map(|item| item.file_size))
        .bind(upload.map(|item| item.mime.as_str()))
        .bind(upload.map(|item| item.width))
        .bind(upload.map(|item| item.height))
        .bind(require_approval)
        .fetch_one(&mut *transaction)
        .await?;

        let root_thread_id = thread_id.unwrap_or(post_id);
        if !require_approval && thread_id.is_some() && reply_count < i64::from(board.bump_limit)
        {
            sqlx::query("UPDATE posts SET bumped_at = NOW() WHERE id = $1")
                .bind(root_thread_id)
                .execute(&mut *transaction)
                .await?;
        }

        let archived_ids = if thread_id.is_none() && !require_approval {
            let active_limit = i64::from(board.threads_per_page) * i64::from(board.max_pages);
            sqlx::query_scalar::<_, i64>(
                "WITH overflow AS (SELECT id FROM posts WHERE board_id = $1 AND thread_id IS NULL AND archived_at IS NULL ORDER BY sticky DESC, bumped_at DESC, id DESC OFFSET $2) UPDATE posts SET archived_at = NOW(), locked = TRUE WHERE id IN (SELECT id FROM overflow) RETURNING id",
            )
            .bind(board.id)
            .bind(active_limit)
            .fetch_all(&mut *transaction)
            .await?
        } else {
            Vec::new()
        };
        transaction.commit().await?;
        Ok::<_, AppError>((post_id, root_thread_id, archived_ids, require_approval))
    }
    .await;

    let (post_id, thread_id, archived_ids, require_approval) = match transaction_result {
        Ok(result) => result,
        Err(error) => {
            cleanup_saved(&state, saved_upload.as_ref()).await;
            return Err(error);
        }
    };

    if require_approval {
        return Ok(Redirect::to(&format!("/{}/?submitted=pending", board.slug)));
    }

    state
        .builder
        .rebuild_thread(&board.slug, thread_id)
        .await
        .context("could not rebuild thread")?;
    for archived_id in archived_ids {
        if archived_id != thread_id {
            state
                .builder
                .rebuild_thread(&board.slug, archived_id)
                .await
                .context("could not refresh archived thread")?;
        }
    }
    state
        .builder
        .rebuild_board(&board.slug)
        .await
        .context("could not rebuild board")?;
    Ok(Redirect::to(&format!(
        "/{}/res/{}.html#{}",
        board.slug, thread_id, post_id
    )))
}

async fn cleanup_saved(state: &AppState, upload: Option<&SavedUpload>) {
    if let Some(upload) = upload {
        remove_upload(
            &state.config.upload_dir,
            Some(&upload.file_path),
            Some(&upload.thumb_path),
        )
        .await;
    }
}

pub async fn post_actions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Redirect> {
    enforce_same_origin(&headers)?;
    let mut board_slug = String::new();
    let mut action = String::new();
    let mut reason = String::new();
    let mut post_ids = Vec::new();
    for (key, value) in url::form_urlencoded::parse(&body) {
        match key.as_ref() {
            "board" => board_slug = value.into_owned(),
            "action" => action = value.into_owned(),
            "reason" => reason = value.into_owned(),
            "post_id" => {
                if let Ok(id) = value.parse::<i64>() {
                    post_ids.push(id);
                }
            }
            _ => {}
        }
    }
    post_ids.sort_unstable();
    post_ids.dedup();
    if post_ids.is_empty() || post_ids.len() > 25 {
        return Err(AppError::bad_request("Select between one and 25 posts."));
    }
    let board = sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE slug = $1")
        .bind(&board_slug)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("That board does not exist."))?;
    match action.as_str() {
        "report" => {
            let global_allowed =
                state
                    .rate_limiter
                    .check("report:global".to_owned(), 300, Duration::from_secs(60));
            let board_allowed = state.rate_limiter.check(
                format!("report-board:{}", board.slug),
                100,
                Duration::from_secs(60),
            );
            if !global_allowed || !board_allowed {
                return Err(AppError::too_many("The report limit has been reached."));
            }
            let reason = clean_text(&reason, 300);
            if reason.is_empty() {
                return Err(AppError::bad_request(
                    "Give the moderators a reason for the report.",
                ));
            }
            let mut transaction = state.pool.begin().await?;
            let found = sqlx::query_scalar::<_, i64>(
                "SELECT id FROM posts WHERE board_id = $1 AND id = ANY($2) AND approved_at IS NOT NULL FOR KEY SHARE",
            )
            .bind(board.id)
            .bind(&post_ids)
            .fetch_all(&mut *transaction)
            .await?;
            if found.len() != post_ids.len() {
                return Err(AppError::not_found(
                    "One or more selected posts no longer exist.",
                ));
            }
            for post_id in post_ids {
                sqlx::query(
                    "INSERT INTO reports (post_id, reason) VALUES ($1,$2) ON CONFLICT DO NOTHING",
                )
                .bind(post_id)
                .bind(&reason)
                .execute(&mut *transaction)
                .await?;
            }
            transaction.commit().await?;
        }
        _ => return Err(AppError::bad_request("Choose report.")),
    }

    Ok(Redirect::to(&format!("/{}/", board.slug)))
}
