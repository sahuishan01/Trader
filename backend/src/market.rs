use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::models::{MarketTick, ServerEvent, SUPPORTED_SYMBOLS};
use crate::state::AppState;
use crate::strategy;

/// Connects to Binance's public combined market-data stream (no API key
/// required) and keeps reconnecting with a backoff if the connection drops.
pub async fn run(state: Arc<AppState>) {
    loop {
        if let Err(e) = run_once(state.clone()).await {
            warn!("binance stream error: {e}, reconnecting in 3s");
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn run_once(state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    let streams = SUPPORTED_SYMBOLS
        .iter()
        .flat_map(|s| {
            let lower = s.to_lowercase();
            vec![format!("{lower}@bookTicker"), format!("{lower}@trade")]
        })
        .collect::<Vec<_>>()
        .join("/");
    let url = format!("wss://stream.binance.com:9443/stream?streams={streams}");

    info!("connecting to binance: {url}");
    let (ws_stream, _) = connect_async(&url).await?;
    info!("binance stream connected");
    let (_write, mut read) = ws_stream.split();

    let mut last_broadcast_ms: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();

    while let Some(msg) = read.next().await {
        let msg = msg?;
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };
        let envelope: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                error!("failed to parse binance message: {e}");
                continue;
            }
        };
        let stream_name = envelope.get("stream").and_then(|s| s.as_str()).unwrap_or("");
        let data = match envelope.get("data") {
            Some(d) => d,
            None => continue,
        };

        if stream_name.ends_with("@bookTicker") {
            handle_book_ticker(&state, data).await;
        } else if stream_name.ends_with("@trade") {
            handle_trade(&state, data, &mut last_broadcast_ms).await;
        }
    }

    Ok(())
}

async fn handle_book_ticker(state: &Arc<AppState>, data: &Value) {
    let symbol = data.get("s").and_then(|v| v.as_str()).unwrap_or_default();
    let bid: f64 = parse_f64(data.get("b"));
    let bid_qty: f64 = parse_f64(data.get("B"));
    let ask: f64 = parse_f64(data.get("a"));
    let ask_qty: f64 = parse_f64(data.get("A"));
    if symbol.is_empty() || bid <= 0.0 || ask <= 0.0 {
        return;
    }

    for account in state.active_accounts().await {
        strategy::on_book_update(&account, symbol, bid, ask, bid_qty, ask_qty).await;
    }

    let mut market = state.market.write().await;
    let entry = market.entry(symbol.to_string()).or_insert_with(|| MarketTick {
        symbol: symbol.to_string(),
        price: (bid + ask) / 2.0,
        bid,
        ask,
        bid_qty,
        ask_qty,
        timestamp: Utc::now().timestamp_millis(),
    });
    entry.bid = bid;
    entry.ask = ask;
    entry.bid_qty = bid_qty;
    entry.ask_qty = ask_qty;
}

async fn handle_trade(
    state: &Arc<AppState>,
    data: &Value,
    last_broadcast_ms: &mut std::collections::HashMap<String, i64>,
) {
    let symbol = data.get("s").and_then(|v| v.as_str()).unwrap_or_default();
    let price = parse_f64(data.get("p"));
    if symbol.is_empty() || price <= 0.0 {
        return;
    }
    let ts_ms = Utc::now().timestamp_millis();

    {
        let mut market = state.market.write().await;
        let entry = market.entry(symbol.to_string()).or_insert_with(|| MarketTick {
            symbol: symbol.to_string(),
            price,
            bid: price,
            ask: price,
            bid_qty: 0.0,
            ask_qty: 0.0,
            timestamp: ts_ms,
        });
        entry.price = price;
        entry.timestamp = ts_ms;
    }

    for account in state.active_accounts().await {
        strategy::on_price_tick(state, &account, symbol, price, ts_ms).await;
    }

    // Throttle UI price broadcasts to ~10/s per symbol; the strategy above
    // still sees every single tick for accurate signal timing.
    let last = last_broadcast_ms.get(symbol).copied().unwrap_or(0);
    if ts_ms - last >= 100 {
        last_broadcast_ms.insert(symbol.to_string(), ts_ms);
        if let Some(tick) = state.market.read().await.get(symbol).cloned() {
            state.publish_market(ServerEvent::PriceTick(tick));
        }
    }
}

fn parse_f64(v: Option<&Value>) -> f64 {
    v.and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}
