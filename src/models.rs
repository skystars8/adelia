use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Board {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub subtitle: String,
    pub description: String,
    pub position: i32,
    pub threads_per_page: i16,
    pub max_pages: i16,
    pub bump_limit: i32,
    pub max_replies: i32,
    pub read_only: bool,
    pub require_approval: bool,
    #[serde(skip_serializing)]
    pub posting_password_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Post {
    pub id: i64,
    pub board_id: i64,
    pub thread_id: Option<i64>,
    pub name: String,
    pub tripcode: Option<String>,
    pub subject: String,
    pub body: String,
    pub body_html: String,
    pub created_at: DateTime<Utc>,
    pub bumped_at: DateTime<Utc>,
    pub sticky: bool,
    pub locked: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub file_original_name: Option<String>,
    pub file_path: Option<String>,
    pub thumb_path: Option<String>,
    pub file_size: Option<i64>,
    pub file_mime: Option<String>,
    pub image_width: Option<i32>,
    pub image_height: Option<i32>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<i64>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct NewsEntry {
    pub id: i64,
    pub subject: String,
    pub body: String,
    pub body_html: String,
    pub author_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Moderator {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub active: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ReportRow {
    pub id: i64,
    pub post_id: i64,
    pub thread_id: i64,
    pub reason: String,
    pub created_at: DateTime<Utc>,
    pub board_slug: String,
    pub post_body: String,
}

#[derive(Debug, Clone)]
pub struct ModeratorSession {
    pub moderator_id: i64,
    pub username: String,
    pub role: String,
    pub csrf_token: String,
}
