import { formatPrice, formatQty, formatUsd, formatPct } from "../format";
import { colorForSymbol } from "../symbolColors";

export default function PositionsTable({ positions, supportedSymbols }) {
  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Open positions</h2>
      </div>
      {positions.length === 0 ? (
        <div className="empty-state">No open positions — the bot is flat and watching for a signal.</div>
      ) : (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Symbol</th>
                <th className="num">Qty</th>
                <th className="num">Avg entry</th>
                <th className="num">Current</th>
                <th className="num">Market value</th>
                <th className="num">Unrealized P&L</th>
              </tr>
            </thead>
            <tbody>
              {positions.map((p) => (
                <tr key={p.symbol}>
                  <td>
                    <span className="dot" style={{ background: colorForSymbol(p.symbol, supportedSymbols) }} />
                    {p.symbol}
                  </td>
                  <td className="num tabular">{formatQty(p.qty, p.symbol)}</td>
                  <td className="num tabular">{formatPrice(p.avg_entry_price)}</td>
                  <td className="num tabular">{formatPrice(p.current_price)}</td>
                  <td className="num tabular">{formatUsd(p.market_value)}</td>
                  <td className={`num tabular ${p.unrealized_pnl >= 0 ? "stat-good" : "stat-critical"}`}>
                    {formatUsd(p.unrealized_pnl, { signed: true })}
                    <span className="pnl-pct"> ({formatPct(p.unrealized_pnl_pct)})</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
