import { useEffect, useRef, useState } from "react";
import { formatPrice, formatPct } from "../format";
import { colorForSymbol } from "../symbolColors";
import Sparkline from "./Sparkline";

function TickerCard({ symbol, tick, color, enabled, history }) {
  const prevPrice = useRef(null);
  const [flash, setFlash] = useState(null);

  useEffect(() => {
    if (!tick) return;
    if (prevPrice.current !== null && tick.price !== prevPrice.current) {
      setFlash(tick.price > prevPrice.current ? "up" : "down");
      const timer = setTimeout(() => setFlash(null), 400);
      prevPrice.current = tick.price;
      return () => clearTimeout(timer);
    }
    prevPrice.current = tick.price;
  }, [tick?.price]);

  const first = history && history.length ? history[0].price : null;
  const changePct = first && tick ? ((tick.price - first) / first) * 100 : null;
  const trendColor = changePct == null ? color : changePct >= 0 ? "var(--good)" : "var(--critical)";

  return (
    <div className={`ticker-card ${enabled ? "" : "ticker-card--dim"}`}>
      <div className="ticker-symbol">
        <span className="dot" style={{ background: color }} />
        {symbol}
        {!enabled && <span className="ticker-off-badge">off</span>}
      </div>
      {tick ? (
        <>
          <div className={`ticker-price tabular ${flash ? `flash-${flash}` : ""}`}>
            {formatPrice(tick.price)}
          </div>
          <div className="ticker-meta-row">
            <span className={`ticker-change tabular ${changePct == null ? "text-muted" : changePct >= 0 ? "stat-good" : "stat-critical"}`}>
              {changePct == null ? "—" : formatPct(changePct)}
            </span>
            <Sparkline data={history} color={trendColor} />
          </div>
          <div className="ticker-spread tabular">
            <span className="stat-critical">{formatPrice(tick.bid)}</span>
            <span className="ticker-spread-sep">/</span>
            <span className="stat-good">{formatPrice(tick.ask)}</span>
          </div>
        </>
      ) : (
        <div className="ticker-price tabular text-muted">connecting…</div>
      )}
    </div>
  );
}

export default function PriceTicker({ supportedSymbols, enabledSymbols, market, history }) {
  const enabledSet = new Set(enabledSymbols);
  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Live markets</h2>
        <span className="text-muted" style={{ fontSize: 12 }}>
          {enabledSymbols.length} of {supportedSymbols.length} tradable
        </span>
      </div>
      {supportedSymbols.length === 0 ? (
        <div className="empty-state">Loading markets…</div>
      ) : (
        <div className="ticker-row">
          {supportedSymbols.map((symbol) => (
            <TickerCard
              key={symbol}
              symbol={symbol}
              tick={market[symbol]}
              color={colorForSymbol(symbol, supportedSymbols)}
              enabled={enabledSet.has(symbol)}
              history={history[symbol]}
            />
          ))}
        </div>
      )}
    </div>
  );
}
