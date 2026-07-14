use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};

use crate::models::{Side, Trade, TradeSource};

pub async fn init_pool(path: &str) -> sqlx::Result<SqlitePool> {
    // Default rollback-journal mode takes an exclusive lock and fsyncs on
    // every single commit -- fine for occasional writes, but this app
    // inserts a row on nearly every strategy tick across every active
    // account, and that was measured queuing up to multi-second commits on
    // Windows. WAL lets readers run concurrently with the writer and batches
    // fsyncs, which is the standard fix for this write pattern.
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[derive(sqlx::FromRow)]
struct TransactionRow {
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

impl TryFrom<TransactionRow> for Trade {
    type Error = String;
    fn try_from(row: TransactionRow) -> Result<Self, Self::Error> {
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

pub async fn insert_transaction(pool: &SqlitePool, user_id: &str, trade: &Trade) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO transactions (id, user_id, symbol, side, qty, price, value, pnl, source, timestamp)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&trade.id)
    .bind(user_id)
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

pub async fn load_transactions(pool: &SqlitePool, user_id: &str) -> sqlx::Result<Vec<Trade>> {
    let rows = sqlx::query_as::<_, TransactionRow>(
        "SELECT id, symbol, side, qty, price, value, pnl, source, timestamp
         FROM transactions WHERE user_id = ? ORDER BY timestamp ASC, rowid ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| match Trade::try_from(row) {
            Ok(trade) => Some(trade),
            Err(e) => {
                tracing::error!("skipping malformed persisted transaction: {e}");
                None
            }
        })
        .collect())
}

pub async fn clear_transactions(pool: &SqlitePool, user_id: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM transactions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
