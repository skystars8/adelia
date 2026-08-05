use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use dashmap::DashMap;
use minijinja::context;
use serde::Serialize;
use sqlx::PgPool;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    config::Config,
    models::{Board, NewsEntry, Post},
    render::Templates,
};

#[derive(Clone)]
pub struct StaticBuilder {
    pool: PgPool,
    config: Arc<Config>,
    templates: Arc<Templates>,
    board_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
    home_lock: Arc<Mutex<()>>,
}

#[derive(Debug, Serialize)]
struct SiteView<'a> {
    title: &'a str,
    subtitle: &'a str,
    base_url: &'a str,
}

#[derive(Debug, Serialize)]
struct BoardLink {
    slug: String,
    title: String,
    subtitle: String,
    description: String,
    url: String,
    catalog_url: String,
    archive_url: String,
}

#[derive(Debug, Serialize)]
struct ImageView {
    original_name: String,
    file_url: String,
    thumb_url: String,
    file_size: String,
    mime: String,
    width: i32,
    height: i32,
}

#[derive(Debug, Serialize)]
struct PostView {
    id: i64,
    name: String,
    tripcode: Option<String>,
    subject: String,
    body_html: String,
    created_at: String,
    created_at_iso: String,
    sticky: bool,
    locked: bool,
    image: Option<ImageView>,
}

#[derive(Debug, Serialize)]
struct ThreadView {
    op: PostView,
    replies: Vec<PostView>,
    url: String,
    reply_count: i64,
    image_count: i64,
    omitted_count: i64,
    bumped_at_unix: i64,
    created_at_unix: i64,
}

#[derive(Debug, Serialize)]
struct PageLink {
    number: usize,
    url: String,
    selected: bool,
}

#[derive(Debug, Serialize)]
struct ArchiveThread {
    id: i64,
    subject: String,
    excerpt: String,
    created_at: String,
    url: String,
}

impl StaticBuilder {
    pub fn new(pool: PgPool, config: Arc<Config>, templates: Arc<Templates>) -> Self {
        Self {
            pool,
            config,
            templates,
            board_locks: Arc::new(DashMap::new()),
            home_lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn rebuild_all(&self) -> Result<()> {
        self.rebuild_home().await?;
        let boards = self.boards().await?;
        for board in boards {
            self.rebuild_board(&board.slug).await?;
            let thread_ids = sqlx::query_scalar::<_, i64>(
                "SELECT id FROM posts WHERE board_id = $1 AND thread_id IS NULL AND archived_at IS NULL AND approved_at IS NOT NULL ORDER BY sticky DESC, bumped_at DESC LIMIT $2",
            )
            .bind(board.id)
            .bind(i64::from(board.threads_per_page) * i64::from(board.max_pages))
            .fetch_all(&self.pool)
            .await?;
            for thread_id in thread_ids {
                self.rebuild_thread(&board.slug, thread_id).await?;
            }
        }
        Ok(())
    }

    pub async fn rebuild_home(&self) -> Result<()> {
        let _guard = self.home_lock.lock().await;
        let boards = self.boards().await?;
        let board_links = boards.iter().map(board_link).collect::<Vec<_>>();
        let site = self.site_view();
        let landing = self.templates.render(
            "landing.html",
            context! { site => site, boards => board_links },
        )?;
        self.write("index.html", &landing).await?;

        let news = sqlx::query_as::<_, NewsEntry>(
            "SELECT id, subject, body, body_html, author_name, created_at FROM news WHERE published ORDER BY created_at DESC LIMIT 50",
        )
        .fetch_all(&self.pool)
        .await?;
        let site = self.site_view();
        let board_links = boards.iter().map(board_link).collect::<Vec<_>>();
        let news_page = self.templates.render(
            "news.html",
            context! { site => site, boards => board_links, news => news },
        )?;
        self.write("news.html", &news_page).await
    }

    pub async fn rebuild_board(&self, slug: &str) -> Result<()> {
        let lock = self.board_lock(slug);
        let _guard = lock.lock().await;
        let board = self.board(slug).await?;
        let boards = self.boards().await?;
        let board_links = boards.iter().map(board_link).collect::<Vec<_>>();
        let total_threads = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM posts WHERE board_id = $1 AND thread_id IS NULL AND archived_at IS NULL AND approved_at IS NOT NULL",
        )
        .bind(board.id)
        .fetch_one(&self.pool)
        .await?;
        let per_page = i64::from(board.threads_per_page);
        let page_count = (((total_threads + per_page - 1) / per_page).max(1) as usize)
            .min(board.max_pages as usize);

        for page_index in 0..page_count {
            let ops = sqlx::query_as::<_, Post>(
                "SELECT * FROM posts WHERE board_id = $1 AND thread_id IS NULL AND archived_at IS NULL AND approved_at IS NOT NULL ORDER BY sticky DESC, bumped_at DESC, id DESC LIMIT $2 OFFSET $3",
            )
            .bind(board.id)
            .bind(per_page)
            .bind(page_index as i64 * per_page)
            .fetch_all(&self.pool)
            .await?;
            let mut threads = Vec::with_capacity(ops.len());
            for op in ops {
                threads.push(self.thread_summary(&board, op).await?);
            }
            let pages = (0..page_count)
                .map(|index| PageLink {
                    number: index + 1,
                    url: board_page_url(&board.slug, index),
                    selected: index == page_index,
                })
                .collect::<Vec<_>>();
            let site = self.site_view();
            let board_view = board_link(&board);
            let html = self.templates.render(
                "board.html",
                context! {
                    site => site,
                    board => board_view,
                    boards => board_links,
                    threads => threads,
                    pages => pages,
                    current_page => page_index + 1,
                    read_only => board.read_only,
                    require_approval => board.require_approval,
                    posting_password_required => board.posting_password_hash.is_some(),
                },
            )?;
            let file = if page_index == 0 {
                format!("{}/index.html", board.slug)
            } else {
                format!("{}/{}.html", board.slug, page_index + 1)
            };
            self.write(&file, &html).await?;
        }
        for page_number in (page_count + 1).max(2)..=board.max_pages as usize {
            let stale_page = self
                .config
                .generated_dir
                .join(&board.slug)
                .join(format!("{page_number}.html"));
            if stale_page.is_file() {
                tokio::fs::remove_file(stale_page).await?;
            }
        }
        self.rebuild_catalog_locked(&board, &boards).await?;
        self.rebuild_archive_locked(&board, &boards).await
    }

    pub async fn rebuild_thread(&self, slug: &str, thread_id: i64) -> Result<()> {
        let lock = self.board_lock(slug);
        let _guard = lock.lock().await;
        let board = self.board(slug).await?;
        let op = sqlx::query_as::<_, Post>(
            "SELECT * FROM posts WHERE id = $1 AND board_id = $2 AND thread_id IS NULL AND approved_at IS NOT NULL",
        )
        .bind(thread_id)
        .bind(board.id)
        .fetch_one(&self.pool)
        .await?;
        let replies = sqlx::query_as::<_, Post>(
            "SELECT * FROM posts WHERE thread_id = $1 AND approved_at IS NOT NULL ORDER BY id",
        )
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await?;
        let reply_views = replies.iter().map(post_view).collect::<Vec<_>>();
        let boards = self.boards().await?;
        let site = self.site_view();
        let board_view = board_link(&board);
        let board_links = boards.iter().map(board_link).collect::<Vec<_>>();
        let op_view = post_view(&op);
        let html = self.templates.render(
            "thread.html",
            context! {
                site => site,
                board => board_view,
                boards => board_links,
                op => op_view,
                replies => reply_views,
                reply_count => replies.len(),
                read_only => board.read_only || op.locked || op.archived_at.is_some(),
                require_approval => board.require_approval,
                posting_password_required => board.posting_password_hash.is_some(),
            },
        )?;
        self.write(&format!("{slug}/res/{thread_id}.html"), &html)
            .await
    }

    async fn rebuild_catalog_locked(&self, board: &Board, boards: &[Board]) -> Result<()> {
        let ops = sqlx::query_as::<_, Post>(
            "SELECT * FROM posts WHERE board_id = $1 AND thread_id IS NULL AND archived_at IS NULL AND approved_at IS NOT NULL ORDER BY sticky DESC, bumped_at DESC, id DESC LIMIT $2",
        )
        .bind(board.id)
        .bind(i64::from(board.threads_per_page) * i64::from(board.max_pages))
        .fetch_all(&self.pool)
        .await?;
        let mut threads = Vec::with_capacity(ops.len());
        for op in ops {
            let (reply_count, image_count) = self.counts(op.id).await?;
            threads.push(ThreadView {
                replies: Vec::new(),
                url: format!("/{}/res/{}.html", board.slug, op.id),
                reply_count,
                image_count: image_count + i64::from(op.file_path.is_some()),
                omitted_count: 0,
                bumped_at_unix: op.bumped_at.timestamp(),
                created_at_unix: op.created_at.timestamp(),
                op: post_view(&op),
            });
        }
        let site = self.site_view();
        let board_view = board_link(board);
        let board_links = boards.iter().map(board_link).collect::<Vec<_>>();
        let html = self.templates.render(
            "catalog.html",
            context! { site => site, board => board_view, boards => board_links, threads => threads },
        )?;
        self.write(&format!("{}/catalog.html", board.slug), &html)
            .await
    }

    async fn rebuild_archive_locked(&self, board: &Board, boards: &[Board]) -> Result<()> {
        let archived = sqlx::query_as::<_, Post>(
            "SELECT * FROM posts WHERE board_id = $1 AND thread_id IS NULL AND archived_at IS NOT NULL AND approved_at IS NOT NULL ORDER BY archived_at DESC LIMIT 500",
        )
        .bind(board.id)
        .fetch_all(&self.pool)
        .await?;
        let archived = archived
            .into_iter()
            .map(|post| ArchiveThread {
                id: post.id,
                subject: if post.subject.is_empty() {
                    "No subject".to_owned()
                } else {
                    post.subject
                },
                excerpt: excerpt(&post.body, 160),
                created_at: format_time(post.created_at),
                url: format!("/{}/res/{}.html", board.slug, post.id),
            })
            .collect::<Vec<_>>();
        let site = self.site_view();
        let board_view = board_link(board);
        let board_links = boards.iter().map(board_link).collect::<Vec<_>>();
        let html = self.templates.render(
            "archive.html",
            context! { site => site, board => board_view, boards => board_links, threads => archived },
        )?;
        self.write(&format!("{}/archive.html", board.slug), &html)
            .await
    }

    async fn thread_summary(&self, board: &Board, op: Post) -> Result<ThreadView> {
        let (reply_count, image_count) = self.counts(op.id).await?;
        let replies = sqlx::query_as::<_, Post>(
            "SELECT * FROM (SELECT * FROM posts WHERE thread_id = $1 AND approved_at IS NOT NULL ORDER BY id DESC LIMIT 5) recent ORDER BY id",
        )
        .bind(op.id)
        .fetch_all(&self.pool)
        .await?;
        Ok(ThreadView {
            replies: replies.iter().map(post_view).collect(),
            url: format!("/{}/res/{}.html", board.slug, op.id),
            reply_count,
            image_count: image_count + i64::from(op.file_path.is_some()),
            omitted_count: (reply_count - replies.len() as i64).max(0),
            bumped_at_unix: op.bumped_at.timestamp(),
            created_at_unix: op.created_at.timestamp(),
            op: post_view(&op),
        })
    }

    async fn counts(&self, thread_id: i64) -> Result<(i64, i64)> {
        sqlx::query_as::<_, (i64, i64)>(
            "SELECT COUNT(*)::BIGINT, COUNT(file_path)::BIGINT FROM posts WHERE thread_id = $1 AND approved_at IS NOT NULL",
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn remove_thread_page(&self, slug: &str, thread_id: i64) {
        let path = self
            .config
            .generated_dir
            .join(slug)
            .join("res")
            .join(format!("{thread_id}.html"));
        let _ = tokio::fs::remove_file(path).await;
    }

    async fn boards(&self) -> Result<Vec<Board>> {
        sqlx::query_as::<_, Board>("SELECT * FROM boards ORDER BY position, slug")
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn board(&self, slug: &str) -> Result<Board> {
        sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE slug = $1")
            .bind(slug)
            .fetch_one(&self.pool)
            .await
            .with_context(|| format!("board /{slug}/ does not exist"))
    }

    fn board_lock(&self, slug: &str) -> Arc<Mutex<()>> {
        self.board_locks
            .entry(slug.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn site_view(&self) -> SiteView<'_> {
        SiteView {
            title: &self.config.site_title,
            subtitle: &self.config.site_subtitle,
            base_url: &self.config.public_base_url,
        }
    }

    async fn write(&self, relative: &str, contents: &str) -> Result<()> {
        let target = self.config.generated_dir.join(relative);
        let parent = target.parent().context("generated file has no parent")?;
        tokio::fs::create_dir_all(parent).await?;
        let temporary = temporary_path(&target);
        tokio::fs::write(&temporary, contents).await?;
        if let Err(first_error) = tokio::fs::rename(&temporary, &target).await {
            if target.exists() {
                tokio::fs::remove_file(&target).await?;
                tokio::fs::rename(&temporary, &target).await?;
            } else {
                return Err(first_error.into());
            }
        }
        Ok(())
    }
}

fn post_view(post: &Post) -> PostView {
    PostView {
        id: post.id,
        name: post.name.clone(),
        tripcode: post.tripcode.clone(),
        subject: post.subject.clone(),
        body_html: post.body_html.clone(),
        created_at: format_time(post.created_at),
        created_at_iso: post.created_at.to_rfc3339(),
        sticky: post.sticky,
        locked: post.locked,
        image: post.file_path.as_ref().map(|path| ImageView {
            original_name: post
                .file_original_name
                .clone()
                .unwrap_or_else(|| "image".to_owned()),
            file_url: format!("/media/{path}"),
            thumb_url: format!("/media/{}", post.thumb_path.as_deref().unwrap_or(path)),
            file_size: human_size(post.file_size.unwrap_or_default()),
            mime: post.file_mime.clone().unwrap_or_default(),
            width: post.image_width.unwrap_or_default(),
            height: post.image_height.unwrap_or_default(),
        }),
    }
}

fn board_link(board: &Board) -> BoardLink {
    BoardLink {
        slug: board.slug.clone(),
        title: board.title.clone(),
        subtitle: board.subtitle.clone(),
        description: board.description.clone(),
        url: format!("/{}/", board.slug),
        catalog_url: format!("/{}/catalog.html", board.slug),
        archive_url: format!("/{}/archive.html", board.slug),
    }
}

fn board_page_url(slug: &str, page_index: usize) -> String {
    if page_index == 0 {
        format!("/{slug}/")
    } else {
        format!("/{slug}/{}.html", page_index + 1)
    }
}

fn format_time(time: chrono::DateTime<chrono::Utc>) -> String {
    time.format("%m/%d/%y(%a)%H:%M:%S UTC").to_string()
}

fn human_size(size: i64) -> String {
    if size >= 1024 * 1024 {
        format!("{:.2} MiB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.1} KiB", size as f64 / 1024.0)
    } else {
        format!("{size} B")
    }
}

fn excerpt(value: &str, limit: usize) -> String {
    let mut output = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        output.push('…');
    }
    output
}

fn temporary_path(path: &Path) -> std::path::PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("html");
    path.with_extension(format!("{extension}.{}.tmp", Uuid::new_v4().simple()))
}
