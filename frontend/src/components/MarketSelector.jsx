import { colorForSymbol } from "../symbolColors";

export default function MarketSelector({ supportedSymbols, enabledSymbols, onChange, busy }) {
  const enabledSet = new Set(enabledSymbols);

  function toggle(symbol) {
    const next = new Set(enabledSet);
    if (next.has(symbol)) {
      next.delete(symbol);
    } else {
      next.add(symbol);
    }
    onChange(Array.from(next));
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Markets</h2>
        <span className="text-muted">{enabledSymbols.length} of {supportedSymbols.length} enabled</span>
      </div>
      <p className="markets-hint text-muted">
        Choose which pairs the bot is allowed to open new positions on. Prices keep streaming for
        every pair regardless, and disabling one won't abandon a trade it already has open.
      </p>
      <div className="market-chips">
        {supportedSymbols.map((symbol) => {
          const active = enabledSet.has(symbol);
          return (
            <button
              key={symbol}
              type="button"
              className={`market-chip ${active ? "market-chip--active" : ""}`}
              onClick={() => toggle(symbol)}
              disabled={busy}
              aria-pressed={active}
            >
              <span
                className="dot"
                style={{ background: active ? colorForSymbol(symbol, supportedSymbols) : "var(--baseline)" }}
              />
              {symbol}
            </button>
          );
        })}
      </div>
    </div>
  );
}
