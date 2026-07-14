import { formatUsd } from "../format";
import { colorForSymbol } from "../symbolColors";

export default function SymbolStatsTable({ stats, supportedSymbols }) {
  const rows = (stats ?? []).filter((s) => s.trade_count > 0);
  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Performance by symbol</h2>
      </div>
      {rows.length === 0 ? (
        <div className="empty-state">No closed trades yet.</div>
      ) : (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>Symbol</th>
                <th className="num">Trades</th>
                <th className="num">Win rate</th>
                <th className="num">Realized P&L</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((s) => (
                <tr key={s.symbol}>
                  <td>
                    <span className="dot" style={{ background: colorForSymbol(s.symbol, supportedSymbols) }} />
                    {s.symbol}
                  </td>
                  <td className="num tabular">
                    {s.trade_count}
                    <span className="pnl-pct"> ({s.win_count}W/{s.loss_count}L)</span>
                  </td>
                  <td className="num tabular">{s.win_rate.toFixed(1)}%</td>
                  <td className={`num tabular ${s.realized_pnl >= 0 ? "stat-good" : "stat-critical"}`}>
                    {formatUsd(s.realized_pnl, { signed: true })}
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
