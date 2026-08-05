pub mod config;
pub mod error;
pub mod media;
pub mod models;
pub mod publisher;
pub mod rate_limit;
pub mod render;
pub mod routes;
pub mod security;
pub mod static_builder;

use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};

use crate::{
    config::Config,
    error::{AppError, AppResult},
    rate_limit::RateLimiter,
    render::Templates,
    static_builder::StaticBuilder,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub templates: Arc<Templates>,
    pub builder: Arc<StaticBuilder>,
    pub rate_limiter: Arc<RateLimiter>,
    pub banner_files: Arc<Vec<String>>,
    pub publisher_notify: Arc<Notify>,
    post_slots: Arc<Semaphore>,
    password_slots: Arc<Semaphore>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: PgPool,
        config: Arc<Config>,
        templates: Arc<Templates>,
        builder: Arc<StaticBuilder>,
        rate_limiter: Arc<RateLimiter>,
        banner_files: Arc<Vec<String>>,
        publisher_notify: Arc<Notify>,
    ) -> Self {
        let post_slots = Arc::new(Semaphore::new(config.post_concurrency));
        let password_slots = Arc::new(Semaphore::new(config.password_concurrency));
        Self {
            pool,
            config,
            templates,
            builder,
            rate_limiter,
            banner_files,
            publisher_notify,
            post_slots,
            password_slots,
        }
    }

    pub fn post_permit(&self) -> AppResult<OwnedSemaphorePermit> {
        self.post_slots.clone().try_acquire_owned().map_err(|_| {
            AppError::too_many("The server is busy. Please try posting again shortly.")
        })
    }

    pub fn password_permit(&self) -> AppResult<OwnedSemaphorePermit> {
        self.password_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::too_many("The server is busy. Please try again shortly."))
    }
}
