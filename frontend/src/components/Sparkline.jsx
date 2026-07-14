// Lightweight inline SVG sparkline — deliberately not recharts here: this
// renders inside up to 10 ticker cards updating multiple times a second,
// and mounting that many ResponsiveContainer/ResizeObserver instances would
// be far heavier than a plain polyline for no visual benefit at this size.
export default function Sparkline({ data, width = 96, height = 32, color }) {
  if (!data || data.length < 2) {
    return <svg width={width} height={height} className="sparkline" />;
  }

  const values = data.map((d) => d.price);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;

  const points = values.map((v, i) => {
    const x = (i / (values.length - 1)) * width;
    const y = height - ((v - min) / range) * height;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });

  const areaPoints = `0,${height} ${points.join(" ")} ${width},${height}`;

  return (
    <svg width={width} height={height} className="sparkline" viewBox={`0 0 ${width} ${height}`}>
      <polyline points={areaPoints} fill={color} opacity={0.12} stroke="none" />
      <polyline points={points.join(" ")} fill="none" stroke={color} strokeWidth={1.5} />
    </svg>
  );
}
