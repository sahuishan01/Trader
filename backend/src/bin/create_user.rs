//! Admin-only account creation. There is no public signup flow by design --
//! run this to create a login: `cargo run --bin create_user -- <email> <password>`.
//!
//! Bootstraps its own SQLite connection rather than depending on the main
//! `trader-backend` binary crate's modules (this crate has no lib target),
//! which is a deliberate small duplication to avoid restructuring the whole
//! crate into lib+bin just for one admin CLI.

use std::str::FromStr;
use std::time::Duration;

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::Argon2;
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use uuid::Uuid;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: create_user <email> <password>");
        std::process::exit(1);
    }
    let email = &args[1];
    let password = &args[2];

    let opts = SqliteConnectOptions::from_str("sqlite://trader.db")
        .expect("invalid db path")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await
        .expect("failed to connect to trader.db");
    sqlx::migrate!("./migrations").run(&pool).await.expect("failed to run migrations");

    let existing_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .expect("failed to count users");

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("failed to hash password")
        .to_string();

    let user_id = Uuid::new_v4().to_string();
    let now = Utc::now().timestamp();

    sqlx::query("INSERT INTO users (id, email, password_hash, created_at) VALUES (?, ?, ?, ?)")
        .bind(&user_id)
        .bind(email)
        .bind(&hash)
        .bind(now)
        .execute(&pool)
        .await
        .expect("failed to insert user (email may already be taken)");

    println!("Created user {email} ({user_id})");

    // The first account inherits any pre-multi-user demo trade history so
    // the existing paper-trading run isn't silently lost.
    if existing_users == 0 {
        let legacy_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM paper_trades")
            .fetch_one(&pool)
            .await
            .unwrap_or(0);
        if legacy_count > 0 {
            sqlx::query(
                "INSERT INTO transactions (id, user_id, symbol, side, qty, price, value, pnl, source, timestamp)
                 SELECT id, ?, symbol, side, qty, price, value, pnl, source, timestamp FROM paper_trades",
            )
            .bind(&user_id)
            .execute(&pool)
            .await
            .expect("failed to import legacy trades");
            println!("Imported {legacy_count} legacy paper-trading transactions into this account");
        }
    }
}
