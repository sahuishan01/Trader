mod api;
mod auth;
mod db;
mod engine;
mod market;
mod models;
mod state;
mod strategy;
mod ws;

use std::time::Duration;

use axum::http::{HeaderValue, Method};
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

use state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app_state = AppState::new().await;

    // Live market data feed + strategy engine.
    tokio::spawn(market::run(app_state.clone()));

    // Periodic portfolio snapshot per active account so unrealized P&L /
    // equity move with the market even when no trade has just fired.
    {
        let state = app_state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(1000));
            loop {
                interval.tick().await;
                let prices = state.current_prices().await;
                for account in state.active_accounts().await {
                    account.broadcast_portfolio(&prices).await;
                }
            }
        });
    }

    // Credentialed cross-origin requests (the session cookie) can't use a
    // wildcard origin -- the frontend dev server's origin must be named
    // explicitly, with allow_credentials on.
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:5173".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE])
        .allow_credentials(true);

    let app = Router::new()
        .route("/api/auth/login", post(auth::post_login))
        .route("/api/auth/logout", post(auth::post_logout))
        .route("/api/auth/me", get(auth::get_me))
        .route("/api/state", get(api::get_state))
        .route("/api/trades", get(api::get_trades))
        .route("/api/trade", post(api::post_manual_trade))
        .route("/api/bot/toggle", post(api::post_bot_toggle))
        .route("/api/strategy/config", post(api::post_strategy_config))
        .route("/api/symbols", get(api::get_symbols).post(api::post_symbols))
        .route("/api/reset", post(api::post_reset))
        .route("/ws", get(ws::ws_handler))
        .layer(cors)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8787").await.unwrap();
    tracing::info!("trader-backend listening on http://0.0.0.0:8787");
    axum::serve(listener, app).await.unwrap();
}
