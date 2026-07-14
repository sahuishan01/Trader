import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from "recharts";
import { formatCompactUsd, formatTime } from "../format";

function ChartTooltip({ active, payload, label }) {
  if (!active || !payload || !payload.length) return null;
  return (
    <div className="chart-tooltip">
      <div className="chart-tooltip-time">{formatTime(label)}</div>
      <div className="chart-tooltip-value tabular">{formatCompactUsd(payload[0].value)}</div>
    </div>
  );
}

export default function EquityChart({ data }) {
  const latest = data.length ? data[data.length - 1].equity : null;

  return (
    <div className="panel equity-panel">
      <div className="panel-header">
        <h2>Equity curve</h2>
        {latest !== null && <span className="equity-current tabular">{formatCompactUsd(latest)}</span>}
      </div>
      <div className="equity-chart-body">
        {data.length < 2 ? (
          <div className="empty-state">Waiting for enough data to plot…</div>
        ) : (
          <ResponsiveContainer width="100%" height={220}>
            <AreaChart data={data} margin={{ top: 8, right: 12, left: 0, bottom: 0 }}>
              <defs>
                <linearGradient id="equityFill" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--series-1)" stopOpacity={0.12} />
                  <stop offset="100%" stopColor="var(--series-1)" stopOpacity={0} />
                </linearGradient>
              </defs>
              <CartesianGrid stroke="var(--gridline)" vertical={false} strokeWidth={1} />
              <XAxis
                dataKey="t"
                tickFormatter={formatTime}
                stroke="var(--baseline)"
                tick={{ fill: "var(--text-muted)", fontSize: 11 }}
                tickLine={false}
                axisLine={{ stroke: "var(--baseline)" }}
                minTickGap={60}
              />
              <YAxis
                domain={["auto", "auto"]}
                tickFormatter={formatCompactUsd}
                stroke="var(--baseline)"
                tick={{ fill: "var(--text-muted)", fontSize: 11 }}
                tickLine={false}
                axisLine={false}
                width={64}
              />
              <Tooltip content={<ChartTooltip />} />
              <Area
                type="monotone"
                dataKey="equity"
                stroke="var(--series-1)"
                strokeWidth={2}
                fill="url(#equityFill)"
                dot={false}
                activeDot={{ r: 4, fill: "var(--series-1)", stroke: "var(--surface-1)", strokeWidth: 2 }}
                isAnimationActive={false}
              />
            </AreaChart>
          </ResponsiveContainer>
        )}
      </div>
    </div>
  );
}
