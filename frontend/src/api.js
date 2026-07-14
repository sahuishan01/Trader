export const API_BASE = "http://localhost:8787";
export const WS_URL = "ws://localhost:8787/ws";

async function request(path, options) {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText);
    throw new Error(text || `request failed: ${res.status}`);
  }
  const contentType = res.headers.get("content-type") || "";
  return contentType.includes("application/json") ? res.json() : null;
}

export function getState() {
  return request("/api/state");
}

export function placeManualTrade({ symbol, side, usd_amount }) {
  return request("/api/trade", {
    method: "POST",
    body: JSON.stringify({ symbol, side, usd_amount }),
  });
}

export function toggleBot(enabled) {
  return request("/api/bot/toggle", {
    method: "POST",
    body: JSON.stringify({ enabled }),
  });
}

export function resetPortfolio() {
  return request("/api/reset", { method: "POST" });
}

export function setEnabledSymbols(symbols) {
  return request("/api/symbols", {
    method: "POST",
    body: JSON.stringify({ symbols }),
  });
}
