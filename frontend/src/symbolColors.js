// Fixed categorical order -- 8 hues, assigned by each symbol's position in
// the backend's supported-symbols list (never cycled arbitrarily). Beyond 8
// concurrently-enabled symbols the order wraps, which is acceptable here
// since color is a secondary identity cue -- every row/card also carries the
// symbol name as a direct text label.
const SERIES_VARS = [
  "var(--series-1)",
  "var(--series-2)",
  "var(--series-3)",
  "var(--series-4)",
  "var(--series-5)",
  "var(--series-6)",
  "var(--series-7)",
  "var(--series-8)",
];

export function colorForSymbol(symbol, supportedSymbols) {
  const idx = supportedSymbols.indexOf(symbol);
  return SERIES_VARS[(idx < 0 ? 0 : idx) % SERIES_VARS.length];
}
