# Handoff notes

Written for whichever agent (human or AI) picks this project up next. Read
this before making architectural changes — it captures intent and tradeoffs
that aren't visible from the code alone.

## The actual goal — read this first

**This project is being built toward a real, live trading bot — not a demo
that stays in paper mode forever.** The user's own words: "finalizing this
as a live trading bot." Everything currently in the repo (virtual ledger,
public-only market data, no order placement) is a deliberately scoped
*first stage*, not the end state. Keep that framing in mind when triaging
what's "good enough" vs what still needs work — a rough edge that's fine in
paper mode (e.g. no slippage modeling, no partial fills, no real order
book) becomes a real-money risk once this trades for real. Do not assume
low-effort/low-rigor changes are acceptable just because today's version is
"just paper trading."

Do not silently wire up real trading (real API keys, real order placement)
without explicit, explicit user confirmation at the time — this is the one
class of change in this repo where "just do it" is the wrong default. Real
money execution is inherently high blast-radius and hard to reverse.

## Current state (as of this handoff)

A working paper-trading dashboard:

- **Backend**: Rust/Axum, streams live Binance public market data
  (`bookTicker` + `trade`, no API key needed), runs an automated
  EMA/RSI/ATR trend-following strategy with trailing stops, executes both
  bot and manual trades against a virtual ledger, persists to SQLite.
- **Frontend**: React/Vite, live dashboard over WebSocket + REST, strategy
  tuning UI, risk control UI, per-symbol analytics, login screen.
- **Multi-user**: each login gets a fully isolated portfolio/bot/strategy
  config. Accounts are admin-created only (`cargo run --bin create_user --
  <email> <password>`) — there is no public signup, by design.

See `README.md` for run instructions and a fuller architecture description.

## File map

```
backend/src/
  main.rs       — route table, CORS, startup tasks (market feed, periodic broadcast)
  auth.rs       — session cookie auth, argon2 password verify, login/logout/me
  state.rs      — AppState (global: market data, db, account registry)
                  UserAccount (per-user: engine, strategy config/runtime, bot
                  on/off, risk state, own broadcast channel)
  engine.rs     — the virtual ledger: cash/positions/P&L accounting, drawdown
                  tracking, per-symbol stats. Long-only, no margin.
  strategy.rs   — the trading strategy (EMA crossover + RSI filter + ATR-based
                  TP/SL/trailing stop + adaptive cooldown) and its config/patch
                  types. Runs per-user-account, once per market tick.
  market.rs     — Binance WS client; on each tick, updates global market state
                  then runs the strategy for every currently-loaded account.
  api.rs        — REST handlers, all auth-gated except /api/auth/*.
  ws.rs         — WebSocket handler; merges the global price-tick stream with
                  the authenticated user's private account stream.
  db.rs         — SQLite access (WAL mode — see "Known issues fixed" below).
  models.rs     — shared DTOs/enums, SUPPORTED_SYMBOLS, STARTING_BALANCE.
  bin/create_user.rs — admin CLI for account creation (no public signup).

backend/migrations/
  0001_init.sql    — legacy paper_trades table (pre-multi-user, now just
                     historical; new trades go to `transactions`), users,
                     transactions tables.
  0002_sessions.sql — session cookie storage.

frontend/src/
  App.jsx                    — top-level: auth check -> Login or Dashboard
  components/Login.jsx       — email/password sign-in
  components/BotControls.jsx — bot toggle, reset, strategy tuning form
  components/MarketSelector.jsx — which symbols the bot may trade
  components/PriceTicker.jsx + Sparkline.jsx — live market ratings panel
  components/PositionsTable.jsx / TradesTable.jsx / SymbolStatsTable.jsx
  components/EquityChart.jsx, StatTile.jsx
  api.js       — fetch wrapper (credentials: include), all endpoint calls
```

## Design decisions worth knowing before you change things

- **Manual trades and bot trades share one position per symbol.** If you
  manually buy BTCUSDT while the bot also trades it, the bot's risk
  management (TP/SL/trailing) adopts your manual position using its real
  average cost. This was a deliberate fix for a real bug (see git history:
  "instant-sell" bug) — don't reintroduce a code path where
  `SymbolRuntime`'s default-zeroed TP/SL can be evaluated against a
  position it didn't structure itself.
- **Disabling the bot or a symbol only blocks new entries, never abandons
  an open position.** The exit-management code path in
  `strategy::on_price_tick` is deliberately NOT gated by `bot_enabled` or
  `is_symbol_enabled`. This was explicit user feedback early in the
  project — preserve it.
- **The kill switch (`daily_loss_limit_pct`) does not auto-resume.** A new
  UTC day clears the halt *flag* but does not silently turn the bot back
  on — that requires the user to flip the toggle. This is intentional: a
  circuit breaker that quietly resumes trading unattended defeats its own
  purpose.
- **Position sizing is a fixed % of equity, not volatility-adjusted.** This
  was a conscious choice during the "upgrade the algo" work — a
  volatility-scaled size would need an uncalibrated magic-number reference
  threshold that couldn't be validated without real backtesting data. If
  you add real backtesting (see below), this is worth revisiting.
- **A user's bot only runs once their account has been loaded into memory**
  (`AppState::account_for`, called on login/any authenticated request).
  There's no "run for all registered users on boot" — fine for the current
  single-admin-account scale, but would need addressing before onboarding
  users who aren't actively logged in.
- **Market data streaming is always-on for all `SUPPORTED_SYMBOLS`**,
  regardless of which symbols any user has enabled — this is what makes
  toggling a symbol instant (no WS reconnect) and keeps unrealized P&L live
  even on disabled symbols with open positions.

## Known issues fixed this session (context for why the code looks like it does)

- **`cargo run` ambiguity**: adding `src/bin/create_user.rs` broke bare
  `cargo run` (two binaries, no default). Fixed via `default-run =
  "trader-backend"` in `Cargo.toml`. If you add more `src/bin/*` binaries,
  this is already handled — don't remove that key.
- **SQLite write latency**: default rollback-journal mode was fsyncing on
  every trade insert, observed queuing into multi-second commits under the
  bot's tick-driven write frequency. Fixed with WAL mode +
  `synchronous=NORMAL` + a `busy_timeout` in both `db.rs` and
  `create_user.rs`. If persistence latency becomes a problem again even
  under WAL, the next lever is making `persist_trade` fire-and-forget
  (spawn a task) instead of awaited inline in the tick-processing hot path
  — not yet needed, but flagged here as the next step.
- **CORS + cookies**: credentialed requests can't use a wildcard origin.
  `main.rs` CORS is pinned to `http://localhost:5173` explicitly. If you
  deploy this anywhere other than local dev, this needs to become
  configurable (env var), not hardcoded.

## What "finalizing as a live trading bot" actually requires (not yet started)

None of this exists yet. In roughly the order it'd need to happen:

1. **Backtesting.** The strategy has never been validated against
   historical data — only observed live in paper mode. Before real money is
   at stake, it needs a backtest harness against historical OHLCV/tick data
   with realistic fills.
2. **Realistic execution modeling.** Paper trading currently assumes
   instant fill at the observed tick price with zero fees and zero
   slippage. Real Binance spot trading has both. The paper engine should
   model them before being trusted as a signal of real performance.
3. **Authenticated Binance API integration.** Currently only the public
   market-data WebSocket is used — no API key exists anywhere in this
   codebase. Going live means: secure API key storage (encrypted at rest,
   never logged, never sent to the frontend), real order placement
   (`POST /order` on Binance's authenticated REST API), order status
   polling/websocket user-data-stream handling, and reconciling the local
   ledger against Binance's actual account state rather than trusting local
   bookkeeping alone.
4. **A real emergency stop.** The daily-loss kill switch pauses *new*
   entries; there's no "flatten everything right now" panic button. Real
   trading needs one.
5. **Security hardening commensurate with real money**: rate limiting on
   `/api/auth/login` (currently none — brute-forceable), audit logging of
   every trade/config change, and a real secrets-management story for API
   keys (not env vars in plaintext, ideally).
6. **Tests.** There is currently no automated test suite anywhere in this
   repo. Before real money is at stake, the engine's ledger math (`apply()`
   in `engine.rs`) and the strategy's entry/exit logic at minimum need unit
   coverage — that code has already had one real bug slip through to
   design-review-only detection once (the instant-sell bug above).

## Operational notes for agents working in this repo

- The user runs their own `cargo watch -x run` in their own terminal
  window. **Do not start a competing `cargo run`/backend instance** — it
  will collide on port 8787 (`AddrInUse`). If you need to verify a change,
  start a temporary instance, confirm, then kill it immediately
  (`Get-Process trader-backend | Stop-Process -Force`) and say so
  explicitly.
- Windows file locking: `cargo build`/`cargo run` will fail with "Access is
  denied" while the user's own process holds the `.exe`. Use `cargo check`
  (or `cargo check --bins` to include `create_user`) to verify compilation
  without touching the locked binary.
- An admin account already exists (created via the `create_user` CLI,
  email `ishan20015.s@gmail.com`). Its password was shared with the user
  directly in conversation, not stored anywhere in this repo — don't add it
  here. If you need a fresh account, just run the CLI again.
- `trader.db` (+ `-wal`/`-shm` sidecar files) is gitignored and contains
  live user data — never commit it, never overwrite it without checking
  `git status`-equivalent care first (it's not git-tracked, but it's real
  state the user cares about).
