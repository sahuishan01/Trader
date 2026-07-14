use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::{broadcast, RwLock};

use crate::db;
use crate::engine::Engine;
use crate::models::{
    MarketTick, ServerEvent, StrategyStatus, Trade, DEFAULT_ENABLED_SYMBOLS, SUPPORTED_SYMBOLS,
};
use crate::strategy::{StrategyConfig, StrategyRuntime};

pub struct AppState {
    pub engine: RwLock<Engine>,
    pub market: RwLock<HashMap<String, MarketTick>>,
    pub strategy_runtime: RwLock<StrategyRuntime>,
    pub tx: broadcast::Sender<ServerEvent>,
    pub bot_enabled: AtomicBool,
    pub enabled_symbols: RwLock<HashSet<String>>,
    pub strategy_config: StrategyConfig,
    pub db: SqlitePool,
}

impl AppState {
    pub async fn new() -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(1024);
        let db = db::init_pool("trader.db")
            .await
            .expect("failed to initialize sqlite database");

        let mut engine = Engine::new();
        match db::load_paper_trades(&db).await {
            Ok(trades) => {
                let count = trades.len();
                for trade in trades {
                    let _ = engine.replay(trade);
                }
                tracing::info!("restored {count} persisted paper trades");
            }
            Err(e) => tracing::error!("failed to load persisted trades: {e}"),
        }

        Arc::new(Self {
            engine: RwLock::new(engine),
            market: RwLock::new(HashMap::new()),
            strategy_runtime: RwLock::new(StrategyRuntime::new()),
            tx,
            bot_enabled: AtomicBool::new(true),
            enabled_symbols: RwLock::new(
                DEFAULT_ENABLED_SYMBOLS.iter().map(|s| s.to_string()).collect(),
            ),
            strategy_config: StrategyConfig::default(),
            db,
        })
    }

    pub fn publish(&self, event: ServerEvent) {
        // No receivers connected yet is not an error, ignore send failures.
        let _ = self.tx.send(event);
    }

    pub async fn persist_trade(&self, trade: &Trade) {
        if let Err(e) = db::insert_paper_trade(&self.db, trade).await {
            tracing::error!("failed to persist trade {}: {e}", trade.id);
        }
    }

    pub async fn clear_persisted_trades(&self) {
        if let Err(e) = db::clear_paper_trades(&self.db).await {
            tracing::error!("failed to clear persisted trades: {e}");
        }
    }

    pub async fn current_prices(&self) -> HashMap<String, f64> {
        let market = self.market.read().await;
        SUPPORTED_SYMBOLS
            .iter()
            .filter_map(|s| market.get(*s).map(|t| (s.to_string(), t.price)))
            .collect()
    }

    pub async fn is_symbol_enabled(&self, symbol: &str) -> bool {
        self.enabled_symbols.read().await.contains(symbol)
    }

    pub async fn set_enabled_symbols(&self, symbols: HashSet<String>) {
        *self.enabled_symbols.write().await = symbols;
        self.broadcast_enabled_symbols().await;
    }

    pub async fn broadcast_enabled_symbols(&self) {
        let mut list: Vec<String> = self.enabled_symbols.read().await.iter().cloned().collect();
        list.sort();
        self.publish(ServerEvent::EnabledSymbols(list));
    }

    pub async fn broadcast_portfolio(&self) {
        let prices = self.current_prices().await;
        let snapshot = self.engine.read().await.snapshot(&prices);
        self.publish(ServerEvent::Portfolio(snapshot));
    }

    pub fn strategy_status(&self) -> StrategyStatus {
        let cfg = &self.strategy_config;
        StrategyStatus {
            enabled: self.bot_enabled.load(Ordering::Relaxed),
            ema_fast_period: cfg.ema_fast_period,
            ema_slow_period: cfg.ema_slow_period,
            imbalance_threshold: cfg.imbalance_threshold,
            max_spread_bps: cfg.max_spread_bps,
            position_size_pct: cfg.position_size_pct,
            atr_period: cfg.atr_period,
            rsi_period: cfg.rsi_period,
            rsi_min: cfg.rsi_min,
            rsi_max: cfg.rsi_max,
            tp_atr_mult: cfg.tp_atr_mult,
            sl_atr_mult: cfg.sl_atr_mult,
            trail_activate_atr_mult: cfg.trail_activate_atr_mult,
            trail_atr_mult: cfg.trail_atr_mult,
            cooldown_ms: cfg.cooldown_ms,
            max_cooldown_ms: cfg.max_cooldown_ms,
        }
    }

    pub fn broadcast_strategy_status(&self) {
        self.publish(ServerEvent::Strategy(self.strategy_status()));
    }
}
