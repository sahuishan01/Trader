import { useCallback, useEffect, useRef, useState } from "react";
import StatTile from "./components/StatTile";
import EquityChart from "./components/EquityChart";
import PriceTicker from "./components/PriceTicker";
import PositionsTable from "./components/PositionsTable";
import TradesTable from "./components/TradesTable";
import BotControls from "./components/BotControls";
import ManualTradeForm from "./components/ManualTradeForm";
import MarketSelector from "./components/MarketSelector";
import {
  getState,
  placeManualTrade,
  toggleBot,
  resetPortfolio,
  setEnabledSymbols as apiSetEnabledSymbols,
  WS_URL,
} from "./api";
import { formatCompactUsd, formatUsd, formatPct } from "./format";
import "./App.css";

const MAX_TRADES = 200;
const MAX_EQUITY_POINTS = 500;

export default function App() {
  const [connected, setConnected] = useState(false);
  const [portfolio, setPortfolio] = useState(null);
  const [market, setMarket] = useState({});
  const [trades, setTrades] = useState([]);
  const [strategy, setStrategy] = useState(null);
  const [equityHistory, setEquityHistory] = useState([]);
  const [supportedSymbols, setSupportedSymbols] = useState([]);
  const [enabledSymbols, setEnabledSymbols] = useState([]);
  const [busy, setBusy] = useState(false);
  const [symbolsBusy, setSymbolsBusy] = useState(false);
  const [loadError, setLoadError] = useState(null);
  const wsRef = useRef(null);
  const reconnectTimer = useRef(null);

  const handleEvent = useCallback((event) => {
    switch (event.type) {
      case "price_tick":
        setMarket((prev) => ({ ...prev, [event.data.symbol]: event.data }));
        break;
      case "trade":
        setTrades((prev) =>
          prev.some((t) => t.id === event.data.id) ? prev : [event.data, ...prev].slice(0, MAX_TRADES)
        );
        break;
      case "portfolio":
        setPortfolio(event.data);
        setEquityHistory((prev) =>
          [...prev, { t: Date.now(), equity: event.data.equity }].slice(-MAX_EQUITY_POINTS)
        );
        break;
      case "strategy_status":
        setStrategy(event.data);
        break;
      case "enabled_symbols":
        setEnabledSymbols(event.data);
        break;
      default:
        break;
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    getState()
      .then((snapshot) => {
        if (cancelled) return;
        setPortfolio(snapshot.portfolio);
        setStrategy(snapshot.strategy);
        setTrades([...snapshot.trades].reverse());
        const marketMap = {};
        snapshot.market.forEach((tick) => {
          marketMap[tick.symbol] = tick;
        });
        setMarket(marketMap);
        setEquityHistory([{ t: Date.now(), equity: snapshot.portfolio.equity }]);
        setSupportedSymbols(snapshot.supported_symbols);
        setEnabledSymbols(snapshot.enabled_symbols);
      })
      .catch((err) => setLoadError(err.message));
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    function connect() {
      const socket = new WebSocket(WS_URL);
      wsRef.current = socket;
      socket.onopen = () => setConnected(true);
      socket.onclose = () => {
        setConnected(false);
        reconnectTimer.current = setTimeout(connect, 2000);
      };
      socket.onerror = () => socket.close();
      socket.onmessage = (msg) => {
        try {
          handleEvent(JSON.parse(msg.data));
        } catch {
          /* ignore malformed frame */
        }
      };
    }
    connect();
    return () => {
      clearTimeout(reconnectTimer.current);
      wsRef.current?.close();
    };
  }, [handleEvent]);

  async function handleToggle(enabled) {
    setBusy(true);
    try {
      await toggleBot(enabled);
    } finally {
      setBusy(false);
    }
  }

  async function handleSymbolsChange(symbols) {
    setSymbolsBusy(true);
    setEnabledSymbols(symbols); // optimistic; the WS broadcast will confirm
    try {
      await apiSetEnabledSymbols(symbols);
    } finally {
      setSymbolsBusy(false);
    }
  }

  async function handleReset() {
    setBusy(true);
    try {
      await resetPortfolio();
      setTrades([]);
      setEquityHistory([]);
    } finally {
      setBusy(false);
    }
  }

  const p = portfolio;

  return (
    <div className="app">
      <header className="app-header">
        <div>
          <h1>Virtual HFT Trading Desk</h1>
          <p className="subtitle">Paper trading on live Binance prices — no real funds, no API key required.</p>
        </div>
        <div className={`conn-pill ${connected ? "conn-live" : "conn-down"}`}>
          <span className="conn-dot" />
          {connected ? "Live" : "Reconnecting…"}
        </div>
      </header>

      {loadError && <div className="banner banner-error">Failed to load initial state: {loadError}</div>}

      {p && (
        <section className="stat-grid">
          <StatTile
            label="Equity"
            value={formatCompactUsd(p.equity)}
            sublabel={`${formatUsd(p.total_pnl, { signed: true })} (${formatPct(p.total_pnl_pct)}) since start`}
            tone={p.total_pnl >= 0 ? "good" : "critical"}
            hero
          />
          <StatTile label="Cash balance" value={formatCompactUsd(p.cash_balance)} />
          <StatTile label="Invested amount" value={formatCompactUsd(p.invested_amount)} />
          <StatTile
            label="Unrealized P&L"
            value={formatUsd(p.unrealized_pnl, { signed: true })}
            tone={p.unrealized_pnl >= 0 ? "good" : "critical"}
          />
          <StatTile
            label="Realized P&L"
            value={formatUsd(p.realized_pnl, { signed: true })}
            tone={p.realized_pnl >= 0 ? "good" : "critical"}
          />
          <StatTile
            label="Last trade P&L"
            value={p.last_trade_pnl == null ? "—" : formatUsd(p.last_trade_pnl, { signed: true })}
            tone={p.last_trade_pnl == null ? "neutral" : p.last_trade_pnl >= 0 ? "good" : "critical"}
          />
          <StatTile label="Gross profit" value={formatUsd(p.gross_profit)} tone="good" />
          <StatTile label="Gross loss" value={formatUsd(-p.gross_loss)} tone="critical" />
          <StatTile
            label="Win rate"
            value={`${p.win_rate.toFixed(1)}%`}
            sublabel={`${p.win_count}W / ${p.loss_count}L · ${p.trade_count} trades`}
          />
        </section>
      )}

      <section className="grid-2">
        <EquityChart data={equityHistory} />
        <PriceTicker symbols={enabledSymbols} supportedSymbols={supportedSymbols} market={market} />
      </section>

      <section className="grid-2">
        <PositionsTable positions={p?.positions ?? []} supportedSymbols={supportedSymbols} />
        <div className="stack">
          <BotControls strategy={strategy} onToggle={handleToggle} onReset={handleReset} busy={busy} />
          <MarketSelector
            supportedSymbols={supportedSymbols}
            enabledSymbols={enabledSymbols}
            onChange={handleSymbolsChange}
            busy={symbolsBusy}
          />
        </div>
      </section>

      <section className="grid-2">
        <TradesTable trades={trades} />
        <ManualTradeForm
          cashBalance={p?.cash_balance ?? 0}
          positions={p?.positions ?? []}
          enabledSymbols={enabledSymbols}
          onSubmit={placeManualTrade}
        />
      </section>
    </div>
  );
}
