use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::models::{
    BotToggleRequest, EnabledSymbolsRequest, ManualTradeRequest, MarketTick, PortfolioDto,
    ServerEvent, Side, StrategyStatus, Trade, TradeSource, SUPPORTED_SYMBOLS,
};
use crate::state::AppState;

#[derive(Serialize)]
pub struct StateSnapshot {
    pub portfolio: PortfolioDto,
    pub market: Vec<MarketTick>,
    pub trades: Vec<Trade>,
    pub strategy: StrategyStatus,
    pub supported_symbols: Vec<String>,
    pub enabled_symbols: Vec<String>,
}

pub async fn get_state(State(state): State<Arc<AppState>>) -> Json<StateSnapshot> {
    let prices = state.current_prices().await;
    let portfolio = state.engine.read().await.snapshot(&prices);
    let market = state.market.read().await;
    let market_list: Vec<MarketTick> = SUPPORTED_SYMBOLS
        .iter()
        .filter_map(|s| market.get(*s).cloned())
        .collect();
    drop(market);
    let trades = {
        let engine = state.engine.read().await;
        let mut t: Vec<Trade> = engine.trades.iter().rev().take(100).cloned().collect();
        t.reverse();
        t
    };
    let mut enabled_symbols: Vec<String> = state.enabled_symbols.read().await.iter().cloned().collect();
    enabled_symbols.sort();

    Json(StateSnapshot {
        portfolio,
        market: market_list,
        trades,
        strategy: state.strategy_status(),
        supported_symbols: SUPPORTED_SYMBOLS.iter().map(|s| s.to_string()).collect(),
        enabled_symbols,
    })
}

#[derive(Deserialize)]
pub struct TradesQuery {
    limit: Option<usize>,
}

pub async fn get_trades(
    State(state): State<Arc<AppState>>,
    Query(q): Query<TradesQuery>,
) -> Json<Vec<Trade>> {
    let limit = q.limit.unwrap_or(100).min(1000);
    let engine = state.engine.read().await;
    let mut t: Vec<Trade> = engine.trades.iter().rev().take(limit).cloned().collect();
    t.reverse();
    Json(t)
}

pub async fn post_manual_trade(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ManualTradeRequest>,
) -> Result<Json<Trade>, (StatusCode, String)> {
    if !SUPPORTED_SYMBOLS.contains(&req.symbol.as_str()) {
        return Err((StatusCode::BAD_REQUEST, format!("unknown symbol {}", req.symbol)));
    }
    let price = state
        .market
        .read()
        .await
        .get(&req.symbol)
        .map(|t| t.price)
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "no market price yet".into()))?;

    let qty = match (req.qty, req.usd_amount) {
        (Some(q), _) if q > 0.0 => q,
        (_, Some(usd)) if usd > 0.0 => usd / price,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "provide a positive qty or usd_amount".into(),
            ))
        }
    };

    let side = req.side;
    if side == Side::Sell {
        let held = state.engine.read().await.position_qty(&req.symbol);
        if qty > held + 1e-12 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("cannot sell {qty}, only {held} held"),
            ));
        }
    }

    let result = state
        .engine
        .write()
        .await
        .execute(&req.symbol, side, qty, price, TradeSource::Manual);

    match result {
        Ok(trade) => {
            state.persist_trade(&trade).await;
            state.publish(ServerEvent::TradeExecuted(trade.clone()));
            state.broadcast_portfolio().await;
            Ok(Json(trade))
        }
        Err(_) => Err((StatusCode::BAD_REQUEST, "trade rejected: insufficient cash or position".into())),
    }
}

pub async fn post_bot_toggle(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BotToggleRequest>,
) -> Json<StrategyStatus> {
    state.bot_enabled.store(req.enabled, Ordering::Relaxed);
    state.broadcast_strategy_status();
    Json(state.strategy_status())
}

pub async fn get_symbols(State(state): State<Arc<AppState>>) -> Json<SymbolsResponse> {
    let mut enabled: Vec<String> = state.enabled_symbols.read().await.iter().cloned().collect();
    enabled.sort();
    Json(SymbolsResponse {
        supported: SUPPORTED_SYMBOLS.iter().map(|s| s.to_string()).collect(),
        enabled,
    })
}

#[derive(Serialize)]
pub struct SymbolsResponse {
    pub supported: Vec<String>,
    pub enabled: Vec<String>,
}

pub async fn post_symbols(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EnabledSymbolsRequest>,
) -> Result<Json<SymbolsResponse>, (StatusCode, String)> {
    let mut enabled = HashSet::new();
    for symbol in req.symbols {
        if !SUPPORTED_SYMBOLS.contains(&symbol.as_str()) {
            return Err((StatusCode::BAD_REQUEST, format!("unknown symbol {symbol}")));
        }
        enabled.insert(symbol);
    }

    state.set_enabled_symbols(enabled).await;

    let mut enabled: Vec<String> = state.enabled_symbols.read().await.iter().cloned().collect();
    enabled.sort();
    Ok(Json(SymbolsResponse {
        supported: SUPPORTED_SYMBOLS.iter().map(|s| s.to_string()).collect(),
        enabled,
    }))
}

pub async fn post_reset(State(state): State<Arc<AppState>>) -> StatusCode {
    state.engine.write().await.reset();
    state.strategy_runtime.write().await.clear();
    state.clear_persisted_trades().await;
    state.broadcast_portfolio().await;
    StatusCode::OK
}
