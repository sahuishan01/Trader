use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::NaiveDate;
use sqlx::SqlitePool;
use tokio::sync::{broadcast, RwLock};

use crate::db;
use crate::engine::Engine;
use crate::models::{
    MarketTick, ServerEvent, StrategyStatus, Trade, DEFAULT_ENABLED_SYMBOLS, SUPPORTED_SYMBOLS,
};
use crate::strategy::{StrategyConfig, StrategyConfigPatch, StrategyRuntime};

/// Tracks one user's daily loss budget for the kill switch. A "day" is a
/// UTC calendar date; `daily_start_equity` is a baseline captured the first
/// time each new date is observed.
pub struct RiskState {
    pub daily_start_equity: f64,
    pub daily_date: NaiveDate,
    pub halted: bool,
    pub halt_reason: Option<String>,
}

impl RiskState {
    fn new() -> Self {
        Self {
            daily_start_equity: 0.0,
            // Guarantees the first `check_risk_limits` call always sees a
            // "new day" and captures a real baseline.
            daily_date: NaiveDate::MIN,
            halted: false,
            halt_reason: None,
        }
    }
}

/// One user's isolated paper-trading account: its own ledger, strategy
/// runtime/config, bot on/off state, risk budget, and a private broadcast
/// channel for events only that user should see (portfolio/trades/strategy
/// status/enabled symbols). Raw market price ticks are NOT sent on this
/// channel -- those are global and broadcast separately by `AppState`.
pub struct UserAccount {
    pub user_id: String,
    pub engine: RwLock<Engine>,
    pub strategy_runtime: RwLock<StrategyRuntime>,
    pub bot_enabled: AtomicBool,
    pub enabled_symbols: RwLock<HashSet<String>>,
    pub strategy_config: RwLock<StrategyConfig>,
    pub risk_state: RwLock<RiskState>,
    pub tx: broadcast::Sender<ServerEvent>,
}

impl UserAccount {
    fn new(user_id: String) -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self {
            user_id,
            engine: RwLock::new(Engine::new()),
            strategy_runtime: RwLock::new(StrategyRuntime::new()),
            bot_enabled: AtomicBool::new(true),
            enabled_symbols: RwLock::new(
                DEFAULT_ENABLED_SYMBOLS.iter().map(|s| s.to_string()).collect(),
            ),
            strategy_config: RwLock::new(StrategyConfig::default()),
            risk_state: RwLock::new(RiskState::new()),
            tx,
        }
    }

    pub fn publish(&self, event: ServerEvent) {
        // No receivers connected yet is not an error, ignore send failures.
        let _ = self.tx.send(event);
    }

    pub async fn persist_trade(&self, db: &SqlitePool, trade: &Trade) {
        if let Err(e) = db::insert_transaction(db, &self.user_id, trade).await {
            tracing::error!("failed to persist transaction {}: {e}", trade.id);
        }
    }

    pub async fn clear_persisted_trades(&self, db: &SqlitePool) {
        if let Err(e) = db::clear_transactions(db, &self.user_id).await {
            tracing::error!("failed to clear transactions: {e}");
        }
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

    pub async fn broadcast_portfolio(&self, prices: &HashMap<String, f64>) {
        let snapshot = {
            let mut engine = self.engine.write().await;
            let equity = engine.equity(prices);
            engine.track_drawdown(equity);
            engine.snapshot(prices)
        };
        self.check_risk_limits(snapshot.equity).await;
        self.publish(ServerEvent::Portfolio(snapshot));
    }

    /// Kill switch: trips (disabling the bot) if today's loss from the
    /// day's starting equity breaches `daily_loss_limit_pct`. Runs off the
    /// same cadence as the portfolio broadcast -- a risk stat like this
    /// doesn't need sub-second resolution.
    async fn check_risk_limits(&self, equity: f64) {
        let today = chrono::Utc::now().date_naive();
        let mut risk = self.risk_state.write().await;

        if risk.daily_date != today {
            // New day: reset the loss budget baseline. The halt itself is
            // NOT auto-cleared into a resumed bot -- that requires the user
            // to flip the toggle back on, so a bad day never silently
            // resumes trading unattended.
            risk.daily_date = today;
            risk.daily_start_equity = equity;
            risk.halted = false;
            risk.halt_reason = None;
            return;
        }

        if risk.halted || !self.bot_enabled.load(Ordering::Relaxed) || risk.daily_start_equity <= 0.0 {
            return;
        }

        let cfg = *self.strategy_config.read().await;
        let daily_pnl_pct = (equity - risk.daily_start_equity) / risk.daily_start_equity * 100.0;
        if daily_pnl_pct <= -cfg.daily_loss_limit_pct {
            risk.halted = true;
            risk.halt_reason = Some(format!(
                "Daily loss limit hit: {daily_pnl_pct:.2}% (limit -{:.2}%)",
                cfg.daily_loss_limit_pct
            ));
            drop(risk);
            self.bot_enabled.store(false, Ordering::Relaxed);
            tracing::warn!("risk kill-switch tripped for user {}: daily loss limit breached", self.user_id);
            self.broadcast_strategy_status().await;
        }
    }

    /// Clears the kill-switch halt without touching the daily loss budget
    /// baseline, so the same day's limit still applies.
    pub async fn clear_risk_halt(&self) {
        let mut risk = self.risk_state.write().await;
        risk.halted = false;
        risk.halt_reason = None;
    }

    /// Clears the kill-switch halt and re-baselines the daily loss budget.
    /// Called on a manual account reset so a fresh start isn't immediately
    /// shadowed by yesterday's halt reason.
    pub async fn reset_risk_state(&self, equity: f64) {
        let mut risk = self.risk_state.write().await;
        risk.daily_date = chrono::Utc::now().date_naive();
        risk.daily_start_equity = equity;
        risk.halted = false;
        risk.halt_reason = None;
    }

    pub async fn strategy_status(&self) -> StrategyStatus {
        let cfg = *self.strategy_config.read().await;
        let risk = self.risk_state.read().await;
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
            loss_streak_cooldown_mult: cfg.loss_streak_cooldown_mult,
            min_hold_ms: cfg.min_hold_ms,
            max_concurrent_positions: cfg.max_concurrent_positions,
            max_exposure_pct: cfg.max_exposure_pct,
            daily_loss_limit_pct: cfg.daily_loss_limit_pct,
            risk_halted: risk.halted,
            risk_halt_reason: risk.halt_reason.clone(),
        }
    }

    pub async fn broadcast_strategy_status(&self) {
        self.publish(ServerEvent::Strategy(self.strategy_status().await));
    }

    pub async fn update_strategy_config(&self, patch: StrategyConfigPatch) -> Result<StrategyStatus, String> {
        let current = *self.strategy_config.read().await;
        let next = current.apply_patch(&patch)?;
        *self.strategy_config.write().await = next;
        self.broadcast_strategy_status().await;
        Ok(self.strategy_status().await)
    }
}

pub struct AppState {
    /// Lazily populated as users authenticate/are touched -- a user's bot
    /// only runs once their account has been loaded at least once since
    /// server startup (e.g. by logging in).
    pub accounts: RwLock<HashMap<String, Arc<UserAccount>>>,
    pub market: RwLock<HashMap<String, MarketTick>>,
    /// Global stream for raw price ticks, shared by every connected client
    /// regardless of user -- market data isn't account-specific.
    pub market_tx: broadcast::Sender<ServerEvent>,
    pub db: SqlitePool,
}

impl AppState {
    pub async fn new() -> Arc<Self> {
        let (market_tx, _rx) = broadcast::channel(1024);
        let db = db::init_pool("trader.db")
            .await
            .expect("failed to initialize sqlite database");

        Arc::new(Self {
            accounts: RwLock::new(HashMap::new()),
            market: RwLock::new(HashMap::new()),
            market_tx,
            db,
        })
    }

    pub fn publish_market(&self, event: ServerEvent) {
        let _ = self.market_tx.send(event);
    }

    pub async fn current_prices(&self) -> HashMap<String, f64> {
        let market = self.market.read().await;
        SUPPORTED_SYMBOLS
            .iter()
            .filter_map(|s| market.get(*s).map(|t| (s.to_string(), t.price)))
            .collect()
    }

    /// Returns the account for `user_id`, creating and loading it from
    /// persisted transaction history on first access.
    pub async fn account_for(&self, user_id: &str) -> Arc<UserAccount> {
        if let Some(acc) = self.accounts.read().await.get(user_id) {
            return acc.clone();
        }
        let mut accounts = self.accounts.write().await;
        if let Some(acc) = accounts.get(user_id) {
            return acc.clone(); // lost the race to create it, someone else already did
        }

        let account = Arc::new(UserAccount::new(user_id.to_string()));
        match db::load_transactions(&self.db, user_id).await {
            Ok(trades) => {
                let count = trades.len();
                let mut engine = account.engine.write().await;
                for trade in trades {
                    let _ = engine.replay(trade);
                }
                tracing::info!("restored {count} persisted transactions for user {user_id}");
            }
            Err(e) => tracing::error!("failed to load transactions for {user_id}: {e}"),
        }

        accounts.insert(user_id.to_string(), account.clone());
        account
    }

    /// Snapshot of every account currently loaded in memory -- used by the
    /// market feed and the periodic portfolio broadcaster to drive each
    /// user's strategy/portfolio independently.
    pub async fn active_accounts(&self) -> Vec<Arc<UserAccount>> {
        self.accounts.read().await.values().cloned().collect()
    }
}
