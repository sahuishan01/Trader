import { formatPrice, formatQty, formatUsd, formatTime } from "../format";

export default function TradesTable({ trades }) {
  return (
    <div className="panel trades-panel">
      <div className="panel-header">
        <h2>Live trades</h2>
        <span className="text-muted">{trades.length} shown</span>
      </div>
      {trades.length === 0 ? (
        <div className="empty-state">No trades yet — the bot fires as soon as it sees a qualifying signal.</div>
      ) : (
        <div className="table-scroll table-scroll--trades">
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>Symbol</th>
                <th>Side</th>
                <th className="num">Qty</th>
                <th className="num">Price</th>
                <th className="num">Value</th>
                <th className="num">P&L</th>
                <th>Source</th>
              </tr>
            </thead>
            <tbody>
              {trades.map((t) => (
                <tr key={t.id}>
                  <td className="tabular text-muted">{formatTime(t.timestamp)}</td>
                  <td>{t.symbol}</td>
                  <td>
                    <span className={`side-badge side-${t.side}`}>
                      {t.side === "buy" ? "↑ Buy" : "↓ Sell"}
                    </span>
                  </td>
                  <td className="num tabular">{formatQty(t.qty, t.symbol)}</td>
                  <td className="num tabular">{formatPrice(t.price)}</td>
                  <td className="num tabular">{formatUsd(t.value)}</td>
                  <td className={`num tabular ${t.pnl == null ? "text-muted" : t.pnl >= 0 ? "stat-good" : "stat-critical"}`}>
                    {t.pnl == null ? "—" : formatUsd(t.pnl, { signed: true })}
                  </td>
                  <td>
                    <span className={`source-pill source-${t.source}`}>{t.source}</span>
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
