use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use tokio::{sync::Notify, task::JoinHandle};

use crate::static_builder::StaticBuilder;

const HOME_TARGET: &str = "home";
const HOME_LOCK_KEY: i64 = -4_141_444_541;
const CLAIM_TIMEOUT: &str = "5 minutes";

#[derive(Debug, FromRow)]
struct PublicationJob {
    target: String,
    job_kind: String,
    board_id: Option<i64>,
    thread_id: Option<i64>,
    generation: i64,
    attempts: i32,
}

pub fn board_target(board_id: i64) -> String {
    format!("board:{board_id}")
}

pub fn thread_target(board_id: i64, thread_id: i64) -> String {
    format!("thread:{board_id}:{thread_id}")
}

pub async fn enqueue_home(transaction: &mut Transaction<'_, Postgres>) -> Result<()> {
    enqueue(transaction, HOME_TARGET, "home", None, None).await
}

pub async fn enqueue_board(
    transaction: &mut Transaction<'_, Postgres>,
    board_id: i64,
) -> Result<()> {
    enqueue(
        transaction,
        &board_target(board_id),
        "board",
        Some(board_id),
        None,
    )
    .await
}

pub async fn enqueue_thread(
    transaction: &mut Transaction<'_, Postgres>,
    board_id: i64,
    thread_id: i64,
) -> Result<()> {
    enqueue(
        transaction,
        &thread_target(board_id, thread_id),
        "thread",
        Some(board_id),
        Some(thread_id),
    )
    .await
}

pub async fn enqueue_board_and_threads(
    transaction: &mut Transaction<'_, Postgres>,
    board_id: i64,
) -> Result<()> {
    enqueue_board(transaction, board_id).await?;
    let thread_ids = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM posts WHERE board_id = $1 AND thread_id IS NULL AND approved_at IS NOT NULL",
    )
    .bind(board_id)
    .fetch_all(&mut **transaction)
    .await?;
    for thread_id in thread_ids {
        enqueue_thread(transaction, board_id, thread_id).await?;
    }
    Ok(())
}

async fn enqueue(
    transaction: &mut Transaction<'_, Postgres>,
    target: &str,
    job_kind: &str,
    board_id: Option<i64>,
    thread_id: Option<i64>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO publication_jobs
            (target, job_kind, board_id, thread_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (target) DO UPDATE SET
            generation = publication_jobs.generation + 1,
            queued_at = NOW(),
            available_at = LEAST(publication_jobs.available_at, NOW()),
            last_error = NULL
        "#,
    )
    .bind(target)
    .bind(job_kind)
    .bind(board_id)
    .bind(thread_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub async fn is_pending(pool: &PgPool, target: &str) -> Result<bool> {
    let pending = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM publication_jobs WHERE target = $1)",
    )
    .bind(target)
    .fetch_one(pool)
    .await?;
    Ok(pending)
}

pub async fn pending_count(pool: &PgPool) -> Result<i64> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM publication_jobs")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn release_claims(pool: &PgPool) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE publication_jobs SET claimed_at = NULL, claimed_generation = NULL WHERE claimed_at IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub fn spawn_workers(
    count: usize,
    pool: PgPool,
    builder: Arc<StaticBuilder>,
    notify: Arc<Notify>,
) -> Vec<JoinHandle<()>> {
    (0..count)
        .map(|worker_id| {
            tokio::spawn(worker_loop(
                worker_id,
                pool.clone(),
                builder.clone(),
                notify.clone(),
            ))
        })
        .collect()
}

async fn worker_loop(
    worker_id: usize,
    pool: PgPool,
    builder: Arc<StaticBuilder>,
    notify: Arc<Notify>,
) {
    loop {
        match claim(&pool).await {
            Ok(Some(job)) => {
                let result = process(&pool, &builder, &job).await;
                match result {
                    Ok(()) => {
                        if let Err(error) = complete(&pool, &job).await {
                            tracing::error!(worker_id, error = ?error, "could not complete publication job");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            worker_id,
                            target = %job.target,
                            error = ?error,
                            "static publication failed; it will be retried"
                        );
                        if let Err(reschedule_error) = fail(&pool, &job, &error).await {
                            tracing::error!(
                                worker_id,
                                error = ?reschedule_error,
                                "could not reschedule publication job"
                            );
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
            Ok(None) => {
                tokio::select! {
                    () = notify.notified() => {},
                    () = tokio::time::sleep(Duration::from_secs(1)) => {},
                }
            }
            Err(error) => {
                tracing::error!(worker_id, error = ?error, "could not claim publication job");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn claim(pool: &PgPool) -> Result<Option<PublicationJob>> {
    let mut transaction = pool.begin().await?;
    let job = sqlx::query_as::<_, PublicationJob>(
        r#"
        WITH candidate AS (
            SELECT target
            FROM publication_jobs
            WHERE available_at <= NOW()
              AND (claimed_at IS NULL OR claimed_at < NOW() - $1::INTERVAL)
            ORDER BY queued_at, target
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        UPDATE publication_jobs AS jobs
        SET claimed_at = NOW(),
            claimed_generation = jobs.generation,
            attempts = jobs.attempts + 1
        FROM candidate
        WHERE jobs.target = candidate.target
        RETURNING jobs.target, jobs.job_kind, jobs.board_id, jobs.thread_id,
                  jobs.generation, jobs.attempts
        "#,
    )
    .bind(CLAIM_TIMEOUT)
    .fetch_optional(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(job)
}

async fn process(pool: &PgPool, builder: &StaticBuilder, job: &PublicationJob) -> Result<()> {
    let lock_key = job.board_id.unwrap_or(HOME_LOCK_KEY);
    let mut lock_connection = pool
        .acquire()
        .await
        .context("could not acquire publication lock connection")?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(lock_key)
        .execute(&mut *lock_connection)
        .await?;

    let result = process_locked(pool, builder, job).await;
    if let Err(error) = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(lock_key)
        .execute(&mut *lock_connection)
        .await
    {
        tracing::warn!(target = %job.target, error = ?error, "could not explicitly release publication lock");
    }
    result
}

async fn process_locked(
    pool: &PgPool,
    builder: &StaticBuilder,
    job: &PublicationJob,
) -> Result<()> {
    match job.job_kind.as_str() {
        "home" => builder.rebuild_home().await,
        "board" => {
            let board_id = job.board_id.context("board publication job has no board")?;
            let slug = board_slug(pool, board_id).await?;
            builder.rebuild_board(&slug).await
        }
        "thread" => {
            let board_id = job
                .board_id
                .context("thread publication job has no board")?;
            let thread_id = job
                .thread_id
                .context("thread publication job has no thread")?;
            let slug = board_slug(pool, board_id).await?;
            builder.rebuild_thread(&slug, thread_id).await
        }
        kind => bail!("unknown publication job kind {kind}"),
    }
}

async fn board_slug(pool: &PgPool, board_id: i64) -> Result<String> {
    sqlx::query_scalar::<_, String>("SELECT slug FROM boards WHERE id = $1")
        .bind(board_id)
        .fetch_optional(pool)
        .await?
        .with_context(|| format!("board {board_id} no longer exists"))
}

async fn complete(pool: &PgPool, job: &PublicationJob) -> Result<()> {
    let result = sqlx::query(
        "DELETE FROM publication_jobs WHERE target = $1 AND claimed_generation = $2 AND generation = $2",
    )
    .bind(&job.target)
    .bind(job.generation)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        sqlx::query(
            r#"
            UPDATE publication_jobs
            SET claimed_at = NULL,
                claimed_generation = NULL,
                attempts = 0,
                available_at = NOW()
            WHERE target = $1 AND claimed_generation = $2
            "#,
        )
        .bind(&job.target)
        .bind(job.generation)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn fail(pool: &PgPool, job: &PublicationJob, error: &anyhow::Error) -> Result<()> {
    let delay_seconds = retry_delay_seconds(job.attempts);
    let error_text = format!("{error:#}");
    sqlx::query(
        r#"
        UPDATE publication_jobs
        SET claimed_at = NULL,
            claimed_generation = NULL,
            attempts = CASE WHEN generation = $2 THEN attempts ELSE 0 END,
            available_at = CASE
                WHEN generation = $2 THEN NOW() + make_interval(secs => $3)
                ELSE NOW()
            END,
            last_error = LEFT($4, 2000)
        WHERE target = $1 AND claimed_generation = $2
        "#,
    )
    .bind(&job.target)
    .bind(job.generation)
    .bind(delay_seconds)
    .bind(error_text)
    .execute(pool)
    .await?;
    Ok(())
}

fn retry_delay_seconds(attempts: i32) -> i32 {
    let exponent = attempts.clamp(1, 9) as u32;
    2_i32.pow(exponent).min(300)
}

#[cfg(test)]
mod tests {
    use super::{retry_delay_seconds, thread_target};

    #[test]
    fn retry_delay_is_bounded() {
        assert_eq!(retry_delay_seconds(0), 2);
        assert_eq!(retry_delay_seconds(1), 2);
        assert_eq!(retry_delay_seconds(2), 4);
        assert_eq!(retry_delay_seconds(8), 256);
        assert_eq!(retry_delay_seconds(30), 300);
    }

    #[test]
    fn thread_targets_are_stable_and_distinct() {
        assert_eq!(thread_target(12, 34), "thread:12:34");
        assert_ne!(thread_target(12, 34), thread_target(12, 35));
        assert_ne!(thread_target(12, 34), thread_target(13, 34));
    }
}
