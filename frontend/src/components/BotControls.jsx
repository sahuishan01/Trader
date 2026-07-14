import { formatPct } from "../format";

export default function BotControls({ strategy, onToggle, onReset, busy }) {
  if (!strategy) return null;
  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Strategy — EMA/RSI trend, ATR risk, trailing stop</h2>
      </div>
      <div className="bot-controls-row">
        <label className="switch">
          <input
            type="checkbox"
            checked={strategy.enabled}
            onChange={(e) => onToggle(e.target.checked)}
            disabled={busy}
          />
          <span className="switch-track">
            <span className="switch-thumb" />
          </span>
          <span className="switch-label">{strategy.enabled ? "Bot running" : "Bot paused"}</span>
        </label>
        <button className="btn btn-ghost" onClick={onReset} disabled={busy}>
          Reset virtual account
        </button>
      </div>
      <div className="strategy-params">
        <span>EMA {strategy.ema_fast_period}/{strategy.ema_slow_period}</span>
        <span>RSI {strategy.rsi_period} in [{strategy.rsi_min}, {strategy.rsi_max}]</span>
        <span>Imbalance &gt; {strategy.imbalance_threshold}</span>
        <span>Max spread {strategy.max_spread_bps}bps</span>
        <span>Size {formatPct(strategy.position_size_pct * 100, { signed: false })} equity</span>
        <span>TP {strategy.tp_atr_mult}× ATR</span>
        <span>SL {strategy.sl_atr_mult}× ATR</span>
        <span>Trail {strategy.trail_atr_mult}× ATR after {strategy.trail_activate_atr_mult}× ATR</span>
        <span>Cooldown {strategy.cooldown_ms}ms (up to {strategy.max_cooldown_ms}ms after losses)</span>
      </div>
    </div>
  );
}
