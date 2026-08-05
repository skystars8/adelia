pub mod moderator;
pub mod public;

use axum::{
    Router,
    routing::{get, post},
};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", get(public::health))
        .route("/readyz", get(public::ready))
        .route("/banner/{board}", get(public::random_banner))
        .route("/post", post(public::create_post))
        .route("/actions", post(public::post_actions))
        .merge(moderator::router())
}
