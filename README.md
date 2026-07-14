# Virtual HFT Trading Desk

A paper-trading dashboard that streams **live Binance market data** (public,
no API key) into a Rust engine that simulates a virtual spot account. A
rule-based scalping bot trades it automatically; you can also place manual
virtual trades from the dashboard.

Nothing here touches a real exchange account or real money — it's a
learning/demo environment for seeing how an HFT-style strategy actually
behaves against live prices.

## Architecture

- **`backend/`** — Rust (Axum + Tokio)
  - `market.rs` — connects to Binance's public combined WebSocket stream
    (`bookTicker` + `trade` for BTCUSDT/ETHUSDT), no API key required.
  - `strategy.rs` — EMA(5/20) crossover confirmed by order-book imbalance,
    gated by a max-spread liquidity filter, with take-profit/stop-loss and
    an entry cooldown. O(1) per tick.
  - `engine.rs` — the virtual ledger: cash, positions, realized/unrealized
    P&L, gross profit/loss, win rate. Long-only, spot-style (no shorting,
    no margin — same constraints as a real Binance spot account).
  - `api.rs` / `ws.rs` — REST endpoints + a WebSocket that broadcasts live
    price ticks, trade fills, and portfolio snapshots to the frontend.
- **`frontend/`** — React + Vite dashboard: stat tiles, equity curve,
  live price ticker, positions/trades tables, bot controls, manual trade
  form.

## Running it

**Backend** (listens on `http://localhost:8787`):

```bash
cd backend
cargo run
```

**Frontend** (listens on `http://localhost:5173`):

```bash
cd frontend
npm install
npm run dev
```

Open `http://localhost:5173`. The bot starts trading automatically as soon
as it gets a qualifying signal — no setup needed.

## Notes

- **Starting balance**: $10,000 virtual USDT, reset anytime from the
  dashboard ("Reset virtual account").
- **Manual trades act on the same shared position as the bot** — e.g. if
  you manually buy BTCUSDT, the bot's take-profit/stop-loss/trend-exit
  logic will manage and may close that position too, since it treats any
  open position on a symbol it watches as its own.
- **No Binance API key is used or required** for this setup — all market
  data comes from Binance's public endpoints, and all trades are simulated
  locally. A key would only be needed if this were later pointed at
  Binance's Spot Testnet (`testnet.binance.vision`) for exchange-side order
  matching instead of the local paper engine.
- Strategy parameters (EMA periods, thresholds, position sizing, TP/SL,
  cooldown) are in `backend/src/strategy.rs::StrategyConfig::default()`.
