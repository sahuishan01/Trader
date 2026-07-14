export default function StatTile({ label, value, sublabel, tone = "neutral", hero = false }) {
  const toneClass = tone === "good" ? "stat-good" : tone === "critical" ? "stat-critical" : "";
  return (
    <div className={`stat-tile ${hero ? "stat-tile--hero" : ""}`}>
      <div className="stat-label">{label}</div>
      <div className={`stat-value ${toneClass} ${hero ? "stat-value--hero" : ""}`}>
        {value}
      </div>
      {sublabel && <div className="stat-sublabel">{sublabel}</div>}
    </div>
  );
}
