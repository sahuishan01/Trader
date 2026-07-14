use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::models::{Side, TradeSource};
use crate::state::{AppState, UserAccount};

/// Rule-based micro-scalping strategy:
///
/// - **Trend**: EMA crossover, confirmed by order-book imbalance and gated
///   by a spread/liquidity filter (unchanged from v1).
/// - **Momentum filter**: a Wilder RSI band avoids buying into an already
///   overbought spike or a dead/exhausted move.
/// - **Volatility-adaptive risk**: take-profit and stop-loss are set as
///   multiples of a per-symbol ATR proxy (an EMA of tick-to-tick price
///   range) instead of a fixed percentage, so a volatile symbol gets more
///   room and a quiet one gets tighter risk automatically.
/// - **Trailing stop**: once price has moved far enough in its favor, the
///   stop trails the peak instead of waiting for a static take-profit,
///   letting winners run while still locking in gains.
/// - **Adaptive cooldown**: consecutive losses on a symbol widen its
///   re-entry cooldown (capped), a simple circuit breaker against
///   repeatedly re-entering a choppy losing pattern; a win resets it.
///
/// Every update is still O(1) (all EMAs), so it comfortably keeps up with
/// raw trade-tick frequency.
#[derive(Debug, Clone, Copy)]
pub struct StrategyConfig {
    pub ema_fast_period: u32,
    pub ema_slow_period: u32,
    pub imbalance_threshold: f64,
    pub max_spread_bps: f64,
    pub position_size_pct: f64,
    pub cooldown_ms: i64,
    pub max_cooldown_ms: i64,
    pub loss_streak_cooldown_mult: f64,
    pub min_hold_ms: i64,

    pub atr_period: u32,
    pub rsi_period: u32,
    pub rsi_min: f64,
    pub rsi_max: f64,

    pub tp_atr_mult: f64,
    pub sl_atr_mult: f64,
    pub trail_activate_atr_mult: f64,
    pub trail_atr_mult: f64,

    // Account-level risk controls.
    pub max_concurrent_positions: u32,
    pub max_exposure_pct: f64,
    pub daily_loss_limit_pct: f64,
}

/// Partial update for `StrategyConfig` — every field optional so the API
/// can patch just the parameters the user changed. Ranges are sanity
/// bounds, not tuned recommendations.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct StrategyConfigPatch {
    pub ema_fast_period: Option<u32>,
    pub ema_slow_period: Option<u32>,
    pub imbalance_threshold: Option<f64>,
    pub max_spread_bps: Option<f64>,
    pub position_size_pct: Option<f64>,
    pub cooldown_ms: Option<i64>,
    pub max_cooldown_ms: Option<i64>,
    pub loss_streak_cooldown_mult: Option<f64>,
    pub min_hold_ms: Option<i64>,
    pub atr_period: Option<u32>,
    pub rsi_period: Option<u32>,
    pub rsi_min: Option<f64>,
    pub rsi_max: Option<f64>,
    pub tp_atr_mult: Option<f64>,
    pub sl_atr_mult: Option<f64>,
    pub trail_activate_atr_mult: Option<f64>,
    pub trail_atr_mult: Option<f64>,
    pub max_concurrent_positions: Option<u32>,
    pub max_exposure_pct: Option<f64>,
    pub daily_loss_limit_pct: Option<f64>,
}

impl StrategyConfig {
    pub fn apply_patch(&self, patch: &StrategyConfigPatch) -> Result<StrategyConfig, String> {
        let mut next = *self;
        if let Some(v) = patch.ema_fast_period {
            next.ema_fast_period = v;
        }
        if let Some(v) = patch.ema_slow_period {
            next.ema_slow_period = v;
        }
        if let Some(v) = patch.imbalance_threshold {
            next.imbalance_threshold = v;
        }
        if let Some(v) = patch.max_spread_bps {
            next.max_spread_bps = v;
        }
        if let Some(v) = patch.position_size_pct {
            next.position_size_pct = v;
        }
        if let Some(v) = patch.cooldown_ms {
            next.cooldown_ms = v;
        }
        if let Some(v) = patch.max_cooldown_ms {
            next.max_cooldown_ms = v;
        }
        if let Some(v) = patch.loss_streak_cooldown_mult {
            next.loss_streak_cooldown_mult = v;
        }
        if let Some(v) = patch.min_hold_ms {
            next.min_hold_ms = v;
        }
        if let Some(v) = patch.atr_period {
            next.atr_period = v;
        }
        if let Some(v) = patch.rsi_period {
            next.rsi_period = v;
        }
        if let Some(v) = patch.rsi_min {
            next.rsi_min = v;
        }
        if let Some(v) = patch.rsi_max {
            next.rsi_max = v;
        }
        if let Some(v) = patch.tp_atr_mult {
            next.tp_atr_mult = v;
        }
        if let Some(v) = patch.sl_atr_mult {
            next.sl_atr_mult = v;
        }
        if let Some(v) = patch.trail_activate_atr_mult {
            next.trail_activate_atr_mult = v;
        }
        if let Some(v) = patch.trail_atr_mult {
            next.trail_atr_mult = v;
        }
        if let Some(v) = patch.max_concurrent_positions {
            next.max_concurrent_positions = v;
        }
        if let Some(v) = patch.max_exposure_pct {
            next.max_exposure_pct = v;
        }
        if let Some(v) = patch.daily_loss_limit_pct {
            next.daily_loss_limit_pct = v;
        }
        next.validate()?;
        Ok(next)
    }

    fn validate(&self) -> Result<(), String> {
        if self.ema_fast_period == 0 || self.ema_slow_period == 0 {
            return Err("EMA periods must be positive".into());
        }
        if self.ema_fast_period >= self.ema_slow_period {
            return Err("EMA fast period must be less than slow period".into());
        }
        if !(0.0..=1.0).contains(&self.imbalance_threshold) {
            return Err("imbalance_threshold must be between 0 and 1".into());
        }
        if self.max_spread_bps <= 0.0 {
            return Err("max_spread_bps must be positive".into());
        }
        if !(0.0..=1.0).contains(&self.position_size_pct) || self.position_size_pct <= 0.0 {
            return Err("position_size_pct must be between 0 (exclusive) and 1".into());
        }
        if self.cooldown_ms < 0 || self.max_cooldown_ms < self.cooldown_ms {
            return Err("cooldowns must be non-negative and max >= base".into());
        }
        if self.loss_streak_cooldown_mult < 1.0 {
            return Err("loss_streak_cooldown_mult must be >= 1".into());
        }
        if self.min_hold_ms < 0 {
            return Err("min_hold_ms must be non-negative".into());
        }
        if self.atr_period == 0 || self.rsi_period == 0 {
            return Err("atr_period and rsi_period must be positive".into());
        }
        if !(0.0..=100.0).contains(&self.rsi_min) || !(0.0..=100.0).contains(&self.rsi_max) {
            return Err("RSI bounds must be between 0 and 100".into());
        }
        if self.rsi_min >= self.rsi_max {
            return Err("rsi_min must be less than rsi_max".into());
        }
        if self.tp_atr_mult <= 0.0 || self.sl_atr_mult <= 0.0 {
            return Err("tp_atr_mult and sl_atr_mult must be positive".into());
        }
        if self.trail_activate_atr_mult <= 0.0 || self.trail_atr_mult <= 0.0 {
            return Err("trail multipliers must be positive".into());
        }
        if self.max_concurrent_positions == 0 {
            return Err("max_concurrent_positions must be at least 1".into());
        }
        if !(0.0..=1.0).contains(&self.max_exposure_pct) || self.max_exposure_pct <= 0.0 {
            return Err("max_exposure_pct must be between 0 (exclusive) and 1".into());
        }
        if self.daily_loss_limit_pct <= 0.0 || self.daily_loss_limit_pct > 100.0 {
            return Err("daily_loss_limit_pct must be between 0 (exclusive) and 100".into());
        }
        Ok(())
    }
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            ema_fast_period: 5,
            ema_slow_period: 20,
            imbalance_threshold: 0.12,
            max_spread_bps: 8.0,
            position_size_pct: 0.08,
            cooldown_ms: 2_000,
            max_cooldown_ms: 20_000,
            loss_streak_cooldown_mult: 1.7,
            min_hold_ms: 300,

            atr_period: 50,
            rsi_period: 14,
            rsi_min: 42.0,
            rsi_max: 72.0,

            tp_atr_mult: 2.2,
            sl_atr_mult: 1.1,
            trail_activate_atr_mult: 1.2,
            trail_atr_mult: 0.9,

            max_concurrent_positions: 4,
            max_exposure_pct: 0.25,
            daily_loss_limit_pct: 8.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SymbolRuntime {
    pub ema_fast: Option<f64>,
    pub ema_slow: Option<f64>,
    pub trend_bullish: Option<bool>,
    pub bid: f64,
    pub ask: f64,
    pub bid_qty: f64,
    pub ask_qty: f64,
    pub last_entry_ms: i64,

    // Volatility (tick-range EMA -- a proxy for ATR, since only trade
    // ticks are available here, not OHLC candles).
    pub last_price: Option<f64>,
    pub atr: Option<f64>,

    // Wilder RSI components.
    pub rsi_avg_gain: Option<f64>,
    pub rsi_avg_loss: Option<f64>,

    // Open-position risk management, set at entry (or adopted on first
    // sight of a position this runtime didn't open itself -- a manual
    // trade, or one restored from persistence after a restart).
    pub entry_atr: f64,
    pub tp_price: f64,
    pub sl_price: f64,
    pub peak_price: f64,

    // Adaptive cooldown.
    pub consecutive_losses: u32,
}

impl SymbolRuntime {
    /// Updates trend (EMA crossover), volatility (ATR proxy), and RSI in
    /// one pass. Returns (crossed_bullish, crossed_bearish).
    fn update(&mut self, price: f64, cfg: &StrategyConfig) -> (bool, bool) {
        let alpha_fast = 2.0 / (cfg.ema_fast_period as f64 + 1.0);
        let alpha_slow = 2.0 / (cfg.ema_slow_period as f64 + 1.0);
        self.ema_fast = Some(match self.ema_fast {
            Some(v) => alpha_fast * price + (1.0 - alpha_fast) * v,
            None => price,
        });
        self.ema_slow = Some(match self.ema_slow {
            Some(v) => alpha_slow * price + (1.0 - alpha_slow) * v,
            None => price,
        });
        let prev_trend = self.trend_bullish;
        let new_trend = match (self.ema_fast, self.ema_slow) {
            (Some(f), Some(s)) => Some(f > s),
            _ => None,
        };
        let crossed_bullish = matches!((prev_trend, new_trend), (Some(false), Some(true)))
            || (prev_trend.is_none() && new_trend == Some(true));
        let crossed_bearish = matches!((prev_trend, new_trend), (Some(true), Some(false)));
        self.trend_bullish = new_trend;

        if let Some(last) = self.last_price {
            let delta = price - last;

            let vol_alpha = 2.0 / (cfg.atr_period as f64 + 1.0);
            let range = delta.abs();
            self.atr = Some(match self.atr {
                Some(v) => vol_alpha * range + (1.0 - vol_alpha) * v,
                None => range,
            });

            let rsi_alpha = 1.0 / cfg.rsi_period as f64;
            let gain = delta.max(0.0);
            let loss = (-delta).max(0.0);
            self.rsi_avg_gain = Some(match self.rsi_avg_gain {
                Some(v) => rsi_alpha * gain + (1.0 - rsi_alpha) * v,
                None => gain,
            });
            self.rsi_avg_loss = Some(match self.rsi_avg_loss {
                Some(v) => rsi_alpha * loss + (1.0 - rsi_alpha) * v,
                None => loss,
            });
        }
        self.last_price = Some(price);

        (crossed_bullish, crossed_bearish)
    }

    fn rsi(&self) -> Option<f64> {
        match (self.rsi_avg_gain, self.rsi_avg_loss) {
            (Some(g), Some(l)) if l > 1e-12 => Some(100.0 - 100.0 / (1.0 + g / l)),
            (Some(g), Some(_)) if g > 0.0 => Some(100.0),
            (Some(_), Some(_)) => Some(50.0),
            _ => None,
        }
    }
}

pub type StrategyRuntime = HashMap<String, SymbolRuntime>;

pub async fn on_book_update(
    account: &Arc<UserAccount>,
    symbol: &str,
    bid: f64,
    ask: f64,
    bid_qty: f64,
    ask_qty: f64,
) {
    let mut runtime = account.strategy_runtime.write().await;
    let r = runtime.entry(symbol.to_string()).or_default();
    r.bid = bid;
    r.ask = ask;
    r.bid_qty = bid_qty;
    r.ask_qty = ask_qty;
}

struct Indicators {
    crossed_bullish: bool,
    crossed_bearish: bool,
    spread_bps: f64,
    imbalance: f64,
    atr: Option<f64>,
    rsi: Option<f64>,
    since_entry_ms: i64,
    consecutive_losses: u32,
}

pub async fn on_price_tick(
    state: &Arc<AppState>,
    account: &Arc<UserAccount>,
    symbol: &str,
    price: f64,
    ts_ms: i64,
) {
    let cfg = *account.strategy_config.read().await;

    let indicators = {
        let mut runtime = account.strategy_runtime.write().await;
        let r = runtime.entry(symbol.to_string()).or_default();
        let (crossed_bullish, crossed_bearish) = r.update(price, &cfg);

        let mid = (r.ask + r.bid) / 2.0;
        let spread_bps = if mid > 0.0 {
            (r.ask - r.bid) / mid * 10_000.0
        } else {
            f64::MAX
        };
        let imbalance = if r.bid_qty + r.ask_qty > 0.0 {
            (r.bid_qty - r.ask_qty) / (r.bid_qty + r.ask_qty)
        } else {
            0.0
        };

        Indicators {
            crossed_bullish,
            crossed_bearish,
            spread_bps,
            imbalance,
            atr: r.atr,
            rsi: r.rsi(),
            since_entry_ms: ts_ms - r.last_entry_ms,
            consecutive_losses: r.consecutive_losses,
        }
    };

    let position = account.engine.read().await.positions.get(symbol).cloned();

    match position {
        None => {
            // Disabling the bot, or disabling this specific symbol, only
            // stops new positions from opening; an already-open one (below)
            // is still managed to a clean exit rather than being abandoned
            // mid-trade.
            if !account.bot_enabled.load(Ordering::Relaxed) || !account.is_symbol_enabled(symbol).await {
                return;
            }
            let (Some(atr), Some(rsi)) = (indicators.atr.filter(|a| *a > 0.0), indicators.rsi) else {
                return; // ATR/RSI not warmed up yet.
            };

            let effective_cooldown = ((cfg.cooldown_ms as f64)
                * cfg.loss_streak_cooldown_mult.powi(indicators.consecutive_losses as i32))
            .min(cfg.max_cooldown_ms as f64) as i64;

            if indicators.crossed_bullish
                && indicators.imbalance > cfg.imbalance_threshold
                && indicators.spread_bps < cfg.max_spread_bps
                && rsi >= cfg.rsi_min
                && rsi <= cfg.rsi_max
                && indicators.since_entry_ms > effective_cooldown
            {
                let prices = state.current_prices().await;
                let (equity, open_positions) = {
                    let engine = account.engine.read().await;
                    (engine.equity(&prices), engine.positions.len())
                };
                let notional = equity * cfg.position_size_pct;
                let qty = notional / price;
                let within_exposure_cap = equity > 0.0 && notional / equity <= cfg.max_exposure_pct;
                if qty > 0.0
                    && notional > 10.0
                    && open_positions < cfg.max_concurrent_positions as usize
                    && within_exposure_cap
                {
                    let trade_result = account
                        .engine
                        .write()
                        .await
                        .execute(symbol, Side::Buy, qty, price, TradeSource::Bot);
                    if let Ok(trade) = trade_result {
                        account.persist_trade(&state.db, &trade).await;
                        account.publish(crate::models::ServerEvent::TradeExecuted(trade));
                        account.broadcast_portfolio(&prices).await;
                        let mut runtime = account.strategy_runtime.write().await;
                        if let Some(r) = runtime.get_mut(symbol) {
                            r.last_entry_ms = ts_ms;
                            r.entry_atr = atr;
                            r.tp_price = price + cfg.tp_atr_mult * atr;
                            r.sl_price = price - cfg.sl_atr_mult * atr;
                            r.peak_price = price;
                        }
                    }
                }
            }
        }
        Some(pos) if pos.qty > 0.0 => {
            let managed = {
                let mut runtime = account.strategy_runtime.write().await;
                let r = runtime.entry(symbol.to_string()).or_default();
                r.peak_price = r.peak_price.max(price);

                if r.entry_atr <= 0.0 {
                    // This runtime didn't structure the entry for the
                    // position it's looking at (a manual trade, or one
                    // restored from persistence after a restart) -- adopt
                    // it now using its real average cost so TP/SL/trailing
                    // still apply, instead of exiting on garbage defaults.
                    let Some(atr) = indicators.atr.filter(|a| *a > 0.0) else {
                        return; // ATR not warmed up yet -- skip this tick.
                    };
                    r.entry_atr = atr;
                    r.tp_price = pos.avg_entry_price + cfg.tp_atr_mult * atr;
                    r.sl_price = pos.avg_entry_price - cfg.sl_atr_mult * atr;
                    r.peak_price = price.max(pos.avg_entry_price);
                    r.last_entry_ms = ts_ms;
                }

                (ts_ms - r.last_entry_ms, r.tp_price, r.sl_price, r.entry_atr, r.peak_price)
            };
            let (held_ms, tp_price, sl_price, entry_atr, peak_price) = managed;

            if held_ms < cfg.min_hold_ms {
                return;
            }

            let trail_trigger = pos.avg_entry_price + cfg.trail_activate_atr_mult * entry_atr;
            let effective_sl = if peak_price >= trail_trigger {
                sl_price.max(peak_price - cfg.trail_atr_mult * entry_atr)
            } else {
                sl_price
            };

            let take_profit_hit = price >= tp_price;
            let stop_loss_hit = price <= effective_sl;

            if take_profit_hit || stop_loss_hit || indicators.crossed_bearish {
                let trade_result = account.engine.write().await.execute(
                    symbol,
                    Side::Sell,
                    pos.qty,
                    price,
                    TradeSource::Bot,
                );
                if let Ok(trade) = trade_result {
                    account.persist_trade(&state.db, &trade).await;
                    let pnl = trade.pnl;
                    account.publish(crate::models::ServerEvent::TradeExecuted(trade));
                    let prices = state.current_prices().await;
                    account.broadcast_portfolio(&prices).await;
                    let mut runtime = account.strategy_runtime.write().await;
                    if let Some(r) = runtime.get_mut(symbol) {
                        match pnl {
                            Some(p) if p >= 0.0 => r.consecutive_losses = 0,
                            Some(_) => r.consecutive_losses += 1,
                            None => {}
                        }
                        r.entry_atr = 0.0; // force re-adoption / fresh init on the next entry
                    }
                }
            }
        }
        _ => {}
    }
}
