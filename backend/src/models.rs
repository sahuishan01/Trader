use serde::{Deserialize, Serialize};

/// Every symbol market data is streamed for. The bot may only be *enabled*
/// on a subset of these (see `AppState::enabled_symbols`), but prices are
/// always kept warm for all of them so toggling one on/off is instant and
/// a position on a since-disabled symbol still gets live unrealized P&L.
pub const SUPPORTED_SYMBOLS: [&str; 10] = [
    "BTCUSDT",
    "ETHUSDT",
    "BNBUSDT",
    "SOLUSDT",
    "XRPUSDT",
    "ADAUSDT",
    "DOGEUSDT",
    "LTCUSDT",
    "AVAXUSDT",
    "LINKUSDT",
];

pub const DEFAULT_ENABLED_SYMBOLS: [&str; 2] = ["BTCUSDT", "ETHUSDT"];

pub const STARTING_BALANCE: f64 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }
}

impl std::str::FromStr for Side {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "buy" => Ok(Side::Buy),
            "sell" => Ok(Side::Sell),
            other => Err(format!("unknown side {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeSource {
    Bot,
    Manual,
}

impl TradeSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeSource::Bot => "bot",
            TradeSource::Manual => "manual",
        }
    }
}

impl std::str::FromStr for TradeSource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bot" => Ok(TradeSource::Bot),
            "manual" => Ok(TradeSource::Manual),
            other => Err(format!("unknown trade source {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Trade {
    pub id: String,
    pub symbol: String,
    pub side: Side,
    pub qty: f64,
    pub price: f64,
    pub value: f64,
    /// Realized P&L booked by this trade. Only set on closing (Sell) trades.
    pub pnl: Option<f64>,
    pub source: TradeSource,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Position {
    pub qty: f64,
    pub avg_entry_price: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionDto {
    pub symbol: String,
    pub qty: f64,
    pub avg_entry_price: f64,
    pub current_price: f64,
    pub market_value: f64,
    pub cost_basis: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioDto {
    pub cash_balance: f64,
    pub invested_amount: f64,
    pub equity: f64,
    pub starting_balance: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: f64,
    pub total_pnl: f64,
    pub total_pnl_pct: f64,
    pub gross_profit: f64,
    pub gross_loss: f64,
    pub last_trade_pnl: Option<f64>,
    pub trade_count: u64,
    pub win_count: u64,
    pub loss_count: u64,
    pub win_rate: f64,
    pub positions: Vec<PositionDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketTick {
    pub symbol: String,
    pub price: f64,
    pub bid: f64,
    pub ask: f64,
    pub bid_qty: f64,
    pub ask_qty: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyStatus {
    pub enabled: bool,
    pub ema_fast_period: u32,
    pub ema_slow_period: u32,
    pub imbalance_threshold: f64,
    pub max_spread_bps: f64,
    pub position_size_pct: f64,
    pub atr_period: u32,
    pub rsi_period: u32,
    pub rsi_min: f64,
    pub rsi_max: f64,
    pub tp_atr_mult: f64,
    pub sl_atr_mult: f64,
    pub trail_activate_atr_mult: f64,
    pub trail_atr_mult: f64,
    pub cooldown_ms: i64,
    pub max_cooldown_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerEvent {
    #[serde(rename = "price_tick")]
    PriceTick(MarketTick),
    #[serde(rename = "trade")]
    TradeExecuted(Trade),
    #[serde(rename = "portfolio")]
    Portfolio(PortfolioDto),
    #[serde(rename = "strategy_status")]
    Strategy(StrategyStatus),
    #[serde(rename = "enabled_symbols")]
    EnabledSymbols(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManualTradeRequest {
    pub symbol: String,
    pub side: Side,
    /// Quantity in base asset. If omitted, `usd_amount` is used instead.
    pub qty: Option<f64>,
    /// Quantity expressed in quote (USDT) notional; converted to base qty at current price.
    pub usd_amount: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotToggleRequest {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnabledSymbolsRequest {
    pub symbols: Vec<String>,
}
