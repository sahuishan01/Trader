use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::models::{Side, Trade, TradeSource};

pub async fn init_pool(path: &str) -> sqlx::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?.create_if_missing(true);
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[derive(sqlx::FromRow)]
struct TradeRow {
    id: String,
    symbol: String,
    side: String,
    qty: f64,
    price: f64,
    value: f64,
    pnl: Option<f64>,
    source: String,
    timestamp: i64,
}

impl TryFrom<TradeRow> for Trade {
    type Error = String;
    fn try_from(row: TradeRow) -> Result<Self, Self::Error> {
        Ok(Trade {
            id: row.id,
            symbol: row.symbol,
            side: Side::from_str(&row.side)?,
            qty: row.qty,
            price: row.price,
            value: row.value,
            pnl: row.pnl,
            source: TradeSource::from_str(&row.source)?,
            timestamp: row.timestamp,
        })
    }
}

pub async fn insert_paper_trade(pool: &SqlitePool, trade: &Trade) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO paper_trades (id, symbol, side, qty, price, value, pnl, source, timestamp)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&trade.id)
    .bind(&trade.symbol)
    .bind(trade.side.as_str())
    .bind(trade.qty)
    .bind(trade.price)
    .bind(trade.value)
    .bind(trade.pnl)
    .bind(trade.source.as_str())
    .bind(trade.timestamp)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_paper_trades(pool: &SqlitePool) -> sqlx::Result<Vec<Trade>> {
    let rows = sqlx::query_as::<_, TradeRow>(
        "SELECT id, symbol, side, qty, price, value, pnl, source, timestamp
         FROM paper_trades ORDER BY timestamp ASC, rowid ASC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| match Trade::try_from(row) {
            Ok(trade) => Some(trade),
            Err(e) => {
                tracing::error!("skipping malformed persisted trade: {e}");
                None
            }
        })
        .collect())
}

pub async fn clear_paper_trades(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM paper_trades").execute(pool).await?;
    Ok(())
}
