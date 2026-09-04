/** Format cents-per-share as a compact price, e.g. 63 -> "63c". */
export function formatPrice(cents: number | null): string {
  return cents === null ? "--" : `${cents}c`;
}

/** Format play-money cents as dollars, e.g. 995130 -> "$9,951.30". */
export function formatCash(cents: number): string {
  const sign = cents < 0 ? "-" : "";
  const abs = Math.abs(cents);
  const dollars = Math.floor(abs / 100).toLocaleString("en-US");
  const rem = (abs % 100).toString().padStart(2, "0");
  return `${sign}$${dollars}.${rem}`;
}

/** Format a share quantity with thousands separators. */
export function formatQty(qty: number): string {
  return qty.toLocaleString("en-US");
}

/** Format a unix-ms timestamp as local HH:MM:SS. */
export function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString("en-US", { hour12: false });
}

/** Format cents as dollars with an explicit + or - sign, e.g. 6000 -> "+$60.00". */
export function formatSignedCash(cents: number): string {
  const body = formatCash(Math.abs(cents));
  return cents < 0 ? `-${body}` : `+${body}`;
}

/** Format a percentage with one decimal and a sign, e.g. 4.2 -> "+4.2%". */
export function formatPct(pct: number): string {
  const sign = pct < 0 ? "-" : "+";
  return `${sign}${Math.abs(pct).toFixed(1)}%`;
}

/** Format a Sharpe ratio to two decimals. */
export function formatSharpe(sharpe: number): string {
  return sharpe.toFixed(2);
}

/** Format seconds as M:SS for the round clock. */
export function formatClock(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  const m = Math.floor(s / 60);
  return `${m}:${(s % 60).toString().padStart(2, "0")}`;
}

/**
 * A one-line grade for a finished round, from its return and Sharpe. The
 * thresholds are deliberately blunt: clearing the spread is the whole job.
 */
export function grade(returnPct: number, sharpe: number): string {
  if (returnPct >= 5 && sharpe >= 1.5) return "Sharp read. You worked the flow.";
  if (returnPct >= 1) return "In the green. You beat the bots.";
  if (returnPct > -1) return "Flat. The spread ate your edge.";
  return "Underwater. The bots picked you off.";
}
