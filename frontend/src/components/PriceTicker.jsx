import { useEffect, useRef, useState } from "react";
import { formatPrice } from "../format";
import { colorForSymbol } from "../symbolColors";

function TickerCard({ symbol, tick, color }) {
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

  return (
    <div className="ticker-card">
      <div className="ticker-symbol">
        <span className="dot" style={{ background: color }} />
        {symbol}
      </div>
      {tick ? (
        <>
          <div className={`ticker-price tabular ${flash ? `flash-${flash}` : ""}`}>
            {formatPrice(tick.price)}
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

export default function PriceTicker({ symbols, supportedSymbols, market }) {
  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Live market</h2>
      </div>
      {symbols.length === 0 ? (
        <div className="empty-state">No markets enabled — pick some in Markets below.</div>
      ) : (
        <div className="ticker-row">
          {symbols.map((symbol) => (
            <TickerCard
              key={symbol}
              symbol={symbol}
              tick={market[symbol]}
              color={colorForSymbol(symbol, supportedSymbols)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
