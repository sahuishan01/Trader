import { useState } from "react";
import { formatPct } from "../format";
import { updateStrategyConfig } from "../api";

const FIELDS = [
  { key: "ema_fast_period", label: "EMA fast", step: 1, min: 1 },
  { key: "ema_slow_period", label: "EMA slow", step: 1, min: 2 },
  { key: "rsi_period", label: "RSI period", step: 1, min: 1 },
  { key: "rsi_min", label: "RSI min", step: 1, min: 0, max: 100 },
  { key: "rsi_max", label: "RSI max", step: 1, min: 0, max: 100 },
  { key: "imbalance_threshold", label: "Imbalance >", step: 0.01, min: 0, max: 1 },
  { key: "max_spread_bps", label: "Max spread (bps)", step: 0.5, min: 0.1 },
  { key: "position_size_pct", label: "Position size (0-1)", step: 0.01, min: 0.01, max: 1 },
  { key: "atr_period", label: "ATR period", step: 1, min: 1 },
  { key: "tp_atr_mult", label: "TP (x ATR)", step: 0.1, min: 0.1 },
  { key: "sl_atr_mult", label: "SL (x ATR)", step: 0.1, min: 0.1 },
  { key: "trail_activate_atr_mult", label: "Trail activate (x ATR)", step: 0.1, min: 0.1 },
  { key: "trail_atr_mult", label: "Trail distance (x ATR)", step: 0.1, min: 0.1 },
  { key: "cooldown_ms", label: "Cooldown (ms)", step: 100, min: 0 },
  { key: "max_cooldown_ms", label: "Max cooldown (ms)", step: 500, min: 0 },
  { key: "loss_streak_cooldown_mult", label: "Loss streak mult", step: 0.1, min: 1 },
  { key: "min_hold_ms", label: "Min hold (ms)", step: 50, min: 0 },
  { key: "max_concurrent_positions", label: "Max open positions", step: 1, min: 1 },
  { key: "max_exposure_pct", label: "Max exposure/symbol (0-1)", step: 0.01, min: 0.01, max: 1 },
  { key: "daily_loss_limit_pct", label: "Daily loss limit (%)", step: 0.5, min: 0.5, max: 100 },
];

export default function BotControls({ strategy, onToggle, onReset, busy }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);

  if (!strategy) return null;

  function startEdit() {
    setDraft(Object.fromEntries(FIELDS.map((f) => [f.key, strategy[f.key]])));
    setError(null);
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setDraft(null);
    setError(null);
  }

  async function saveEdit() {
    setSaving(true);
    setError(null);
    try {
      const patch = Object.fromEntries(FIELDS.map((f) => [f.key, Number(draft[f.key])]));
      await updateStrategyConfig(patch);
      setEditing(false);
      setDraft(null);
    } catch (err) {
      setError(err.message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <h2>Strategy — EMA/RSI trend, ATR risk, trailing stop</h2>
        {!editing && (
          <button className="btn btn-ghost" onClick={startEdit}>
            Tune parameters
          </button>
        )}
      </div>
      {strategy.risk_halted && (
        <div className="form-status form-status--error" style={{ marginBottom: 12 }}>
          Kill switch tripped — {strategy.risk_halt_reason}. Re-enable the bot once you've reviewed it.
        </div>
      )}

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

      {editing ? (
        <>
          <div className="strategy-edit-grid">
            {FIELDS.map((f) => (
              <label key={f.key} className="strategy-edit-field">
                <span>{f.label}</span>
                <input
                  type="number"
                  step={f.step}
                  min={f.min}
                  max={f.max}
                  value={draft[f.key]}
                  onChange={(e) => setDraft((prev) => ({ ...prev, [f.key]: e.target.value }))}
                />
              </label>
            ))}
          </div>
          {error && <div className="form-status form-status--error">{error}</div>}
          <div className="bot-controls-row" style={{ marginTop: 12, marginBottom: 0 }}>
            <button className="btn btn-primary" onClick={saveEdit} disabled={saving}>
              {saving ? "Saving…" : "Save changes"}
            </button>
            <button className="btn btn-ghost" onClick={cancelEdit} disabled={saving}>
              Cancel
            </button>
          </div>
        </>
      ) : (
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
          <span>Max {strategy.max_concurrent_positions} open positions</span>
          <span>Max {formatPct(strategy.max_exposure_pct * 100, { signed: false })} exposure/symbol</span>
          <span>Daily loss limit {strategy.daily_loss_limit_pct}%</span>
        </div>
      )}
    </div>
  );
}
