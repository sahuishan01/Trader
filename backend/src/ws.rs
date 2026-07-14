use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;

use crate::auth::AuthUser;
use crate::state::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, user_id))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, user_id: String) {
    let account = state.account_for(&user_id).await;
    let mut market_rx = state.market_tx.subscribe();
    let mut account_rx = account.tx.subscribe();

    // Prime the freshly connected client with a full snapshot so the
    // dashboard doesn't have to wait for the next tick to populate.
    let prices = state.current_prices().await;
    let snapshot = account.engine.read().await.snapshot(&prices);
    if !send_event(&mut socket, &crate::models::ServerEvent::Portfolio(snapshot)).await {
        return;
    }
    if !send_event(&mut socket, &crate::models::ServerEvent::Strategy(account.strategy_status().await)).await {
        return;
    }
    let mut enabled: Vec<String> = account.enabled_symbols.read().await.iter().cloned().collect();
    enabled.sort();
    if !send_event(&mut socket, &crate::models::ServerEvent::EnabledSymbols(enabled)).await {
        return;
    }

    loop {
        tokio::select! {
            event = market_rx.recv() => {
                match event {
                    Ok(event) => {
                        if !send_event(&mut socket, &event).await {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            event = account_rx.recv() => {
                match event {
                    Ok(event) => {
                        if !send_event(&mut socket, &event).await {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

async fn send_event(socket: &mut WebSocket, event: &crate::models::ServerEvent) -> bool {
    match serde_json::to_string(event) {
        Ok(json) => socket.send(Message::Text(json)).await.is_ok(),
        Err(_) => true, // serialization failure isn't a connection failure
    }
}
