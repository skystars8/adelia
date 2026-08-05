pub mod config;
pub mod error;
pub mod media;
pub mod models;
pub mod rate_limit;
pub mod render;
pub mod routes;
pub mod security;
pub mod static_builder;

use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    config::Config, rate_limit::RateLimiter, render::Templates, static_builder::StaticBuilder,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub templates: Arc<Templates>,
    pub builder: Arc<StaticBuilder>,
    pub rate_limiter: Arc<RateLimiter>,
    pub banner_files: Arc<Vec<String>>,
}
