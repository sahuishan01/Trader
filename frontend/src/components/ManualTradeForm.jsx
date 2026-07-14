import { useEffect, useState } from "react";
import { formatUsd } from "../format";

export default function ManualTradeForm({ cashBalance, positions, enabledSymbols, onSubmit }) {
  // Anything enabled, plus any symbol you're still holding even if it was
  // since disabled -- you should always be able to sell out of a position.
  const tradableSymbols = Array.from(
    new Set([...enabledSymbols, ...positions.map((p) => p.symbol)])
  ).sort();

  const [symbol, setSymbol] = useState(tradableSymbols[0] ?? "");
  const [side, setSide] = useState("buy");
  const [amount, setAmount] = useState("100");
  const [status, setStatus] = useState(null); // { kind: 'error'|'success', text }
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (tradableSymbols.length && !tradableSymbols.includes(symbol)) {
      setSymbol(tradableSymbols[0]);
    }
  }, [tradableSymbols.join(",")]);

  const position = positions.find((p) => p.symbol === symbol);
  const maxSellUsd = position ? position.market_value : 0;

  async function handleSubmit(e) {
    e.preventDefault();
    const usd = parseFloat(amount);
    if (!usd || usd <= 0) {
      setStatus({ kind: "error", text: "Enter a positive USD amount." });
      return;
    }
    setSubmitting(true);
    setStatus(null);
    try {
      await onSubmit({ symbol, side, usd_amount: usd });
      setStatus({ kind: "success", text: `${side === "buy" ? "Bought" : "Sold"} ~${formatUsd(usd)} of ${symbol}.` });
    } catch (err) {
      setStatus({ kind: "error", text: err.message || "Trade rejected." });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Manual trade</h2>
        <span className="text-muted tabular">Cash {formatUsd(cashBalance)}</span>
      </div>
      <form className="trade-form" onSubmit={handleSubmit}>
        <div className="trade-form-row">
          <select value={symbol} onChange={(e) => setSymbol(e.target.value)} disabled={!tradableSymbols.length}>
            {tradableSymbols.length === 0 && <option value="">No symbols enabled</option>}
            {tradableSymbols.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
          <div className="side-toggle">
            <button
              type="button"
              className={`btn-side ${side === "buy" ? "btn-side--active-buy" : ""}`}
              onClick={() => setSide("buy")}
            >
              Buy
            </button>
            <button
              type="button"
              className={`btn-side ${side === "sell" ? "btn-side--active-sell" : ""}`}
              onClick={() => setSide("sell")}
            >
              Sell
            </button>
          </div>
        </div>
        <div className="trade-form-row">
          <div className="usd-input">
            <span>$</span>
            <input
              type="number"
              min="1"
              step="1"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
            />
          </div>
          <button className="btn btn-primary" type="submit" disabled={submitting || !symbol}>
            {submitting ? "Placing…" : "Place trade"}
          </button>
        </div>
        {side === "sell" && (
          <div className="text-muted trade-hint">Position value available: {formatUsd(maxSellUsd)}</div>
        )}
        {status && (
          <div className={status.kind === "error" ? "form-status form-status--error" : "form-status form-status--success"}>
            {status.text}
          </div>
        )}
      </form>
    </div>
  );
}
