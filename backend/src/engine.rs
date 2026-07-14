use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

use crate::models::{
    Position, PositionDto, PortfolioDto, Side, SymbolStats, Trade, TradeSource, STARTING_BALANCE,
};

/// Long-only virtual spot trading ledger. Mirrors how a real Binance spot
/// account behaves: you can't sell more than you hold, there is no margin.
pub struct Engine {
    pub cash: f64,
    pub positions: HashMap<String, Position>,
    pub trades: Vec<Trade>,
    pub realized_pnl: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub last_trade_pnl: Option<f64>,
    pub win_count: u64,
    pub loss_count: u64,
    pub peak_equity: f64,
    pub max_drawdown_usd: f64,
    pub max_drawdown_pct: f64,
    pub symbol_stats: HashMap<String, SymbolStats>,
}

pub enum ExecuteError {
    InsufficientCash,
    InsufficientPosition,
    InvalidQty,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            cash: STARTING_BALANCE,
            positions: HashMap::new(),
            trades: Vec::new(),
            realized_pnl: 0.0,
            gross_profit: 0.0,
            gross_loss: 0.0,
            last_trade_pnl: None,
            win_count: 0,
            loss_count: 0,
            peak_equity: STARTING_BALANCE,
            max_drawdown_usd: 0.0,
            max_drawdown_pct: 0.0,
            symbol_stats: HashMap::new(),
        }
    }

    pub fn position_qty(&self, symbol: &str) -> f64 {
        self.positions.get(symbol).map(|p| p.qty).unwrap_or(0.0)
    }

    /// Cheap equity calc (cash + mark-to-market positions) without building
    /// the full `PortfolioDto` — used for position sizing and drawdown
    /// tracking, which don't need the per-position breakdown.
    pub fn equity(&self, current_prices: &HashMap<String, f64>) -> f64 {
        let market_value_total: f64 = self
            .positions
            .iter()
            .filter(|(_, p)| p.qty > 0.0)
            .map(|(symbol, p)| {
                let price = *current_prices.get(symbol).unwrap_or(&p.avg_entry_price);
                p.qty * price
            })
            .sum();
        self.cash + market_value_total
    }

    /// Updates the running equity peak and worst drawdown from that peak.
    /// Call at a steady cadence (the periodic portfolio broadcast) rather
    /// than on every tick -- drawdown is a slow-moving risk stat, not
    /// something that needs sub-second resolution.
    pub fn track_drawdown(&mut self, equity: f64) {
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
        let dd_usd = self.peak_equity - equity;
        if dd_usd > self.max_drawdown_usd {
            self.max_drawdown_usd = dd_usd;
            self.max_drawdown_pct = if self.peak_equity > 0.0 {
                dd_usd / self.peak_equity * 100.0
            } else {
                0.0
            };
        }
    }

    pub fn execute(
        &mut self,
        symbol: &str,
        side: Side,
        qty: f64,
        price: f64,
        source: TradeSource,
    ) -> Result<Trade, ExecuteError> {
        let (value, pnl) = self.apply(symbol, side, qty, price)?;
        let trade = Trade {
            id: Uuid::new_v4().to_string(),
            symbol: symbol.to_string(),
            side,
            qty,
            price,
            value,
            pnl,
            source,
            timestamp: Utc::now().timestamp_millis(),
        };
        self.trades.push(trade.clone());
        Ok(trade)
    }

    /// Re-applies a trade record loaded from persistence, preserving its
    /// original id/timestamp/pnl instead of generating new ones. Used to
    /// reconstruct cash/positions/P&L on startup by replaying trade history
    /// in the same order it originally happened.
    pub fn replay(&mut self, trade: Trade) -> Result<(), ExecuteError> {
        self.apply(&trade.symbol, trade.side, trade.qty, trade.price)?;
        self.trades.push(trade);
        Ok(())
    }

    /// Pure ledger accounting shared by `execute` (live trades) and
    /// `replay` (restoring persisted history): moves cash, updates the
    /// position, and books realized P&L. Returns `(value, pnl)`.
    fn apply(
        &mut self,
        symbol: &str,
        side: Side,
        qty: f64,
        price: f64,
    ) -> Result<(f64, Option<f64>), ExecuteError> {
        if qty <= 0.0 || price <= 0.0 {
            return Err(ExecuteError::InvalidQty);
        }

        let value = qty * price;
        let mut pnl: Option<f64> = None;

        match side {
            Side::Buy => {
                if value > self.cash {
                    return Err(ExecuteError::InsufficientCash);
                }
                self.cash -= value;
                let pos = self.positions.entry(symbol.to_string()).or_default();
                let new_qty = pos.qty + qty;
                pos.avg_entry_price = if pos.qty > 0.0 {
                    (pos.avg_entry_price * pos.qty + price * qty) / new_qty
                } else {
                    price
                };
                pos.qty = new_qty;
            }
            Side::Sell => {
                let pos = self
                    .positions
                    .get_mut(symbol)
                    .filter(|p| p.qty > 0.0)
                    .ok_or(ExecuteError::InsufficientPosition)?;
                if qty > pos.qty + 1e-12 {
                    return Err(ExecuteError::InsufficientPosition);
                }
                let realized = (price - pos.avg_entry_price) * qty;
                self.cash += value;
                pos.qty -= qty;
                if pos.qty <= 1e-12 {
                    self.positions.remove(symbol);
                }
                self.realized_pnl += realized;
                if realized >= 0.0 {
                    self.gross_profit += realized;
                    self.win_count += 1;
                } else {
                    self.gross_loss += realized.abs();
                    self.loss_count += 1;
                }
                self.last_trade_pnl = Some(realized);
                pnl = Some(realized);

                let stats = self.symbol_stats.entry(symbol.to_string()).or_insert_with(|| SymbolStats {
                    symbol: symbol.to_string(),
                    ..Default::default()
                });
                stats.trade_count += 1;
                if realized >= 0.0 {
                    stats.win_count += 1;
                } else {
                    stats.loss_count += 1;
                }
                stats.realized_pnl += realized;
                stats.win_rate = stats.win_count as f64 / stats.trade_count as f64 * 100.0;
            }
        }

        Ok((value, pnl))
    }

    pub fn snapshot(&self, current_prices: &HashMap<String, f64>) -> PortfolioDto {
        let mut invested_amount = 0.0;
        let mut market_value_total = 0.0;
        let mut unrealized_pnl = 0.0;
        let mut positions: Vec<PositionDto> = Vec::new();

        for (symbol, pos) in self.positions.iter() {
            if pos.qty <= 0.0 {
                continue;
            }
            let current_price = *current_prices.get(symbol).unwrap_or(&pos.avg_entry_price);
            let cost_basis = pos.qty * pos.avg_entry_price;
            let market_value = pos.qty * current_price;
            let upnl = market_value - cost_basis;
            invested_amount += cost_basis;
            market_value_total += market_value;
            unrealized_pnl += upnl;
            positions.push(PositionDto {
                symbol: symbol.clone(),
                qty: pos.qty,
                avg_entry_price: pos.avg_entry_price,
                current_price,
                market_value,
                cost_basis,
                unrealized_pnl: upnl,
                unrealized_pnl_pct: if cost_basis > 0.0 {
                    upnl / cost_basis * 100.0
                } else {
                    0.0
                },
            });
        }

        let equity = self.cash + market_value_total;
        let total_pnl = equity - STARTING_BALANCE;
        let trade_count = self.win_count + self.loss_count;

        PortfolioDto {
            cash_balance: self.cash,
            invested_amount,
            equity,
            starting_balance: STARTING_BALANCE,
            realized_pnl: self.realized_pnl,
            unrealized_pnl,
            total_pnl,
            total_pnl_pct: total_pnl / STARTING_BALANCE * 100.0,
            gross_profit: self.gross_profit,
            gross_loss: self.gross_loss,
            last_trade_pnl: self.last_trade_pnl,
            trade_count,
            win_count: self.win_count,
            loss_count: self.loss_count,
            win_rate: if trade_count > 0 {
                self.win_count as f64 / trade_count as f64 * 100.0
            } else {
                0.0
            },
            peak_equity: self.peak_equity,
            max_drawdown_usd: self.max_drawdown_usd,
            max_drawdown_pct: self.max_drawdown_pct,
            positions,
            symbol_stats: {
                let mut stats: Vec<SymbolStats> = self.symbol_stats.values().cloned().collect();
                stats.sort_by(|a, b| a.symbol.cmp(&b.symbol));
                stats
            },
        }
    }

    pub fn reset(&mut self) {
        self.cash = STARTING_BALANCE;
        self.positions.clear();
        self.trades.clear();
        self.realized_pnl = 0.0;
        self.gross_profit = 0.0;
        self.gross_loss = 0.0;
        self.last_trade_pnl = None;
        self.win_count = 0;
        self.loss_count = 0;
        self.peak_equity = STARTING_BALANCE;
        self.max_drawdown_usd = 0.0;
        self.max_drawdown_pct = 0.0;
        self.symbol_stats.clear();
    }
}
