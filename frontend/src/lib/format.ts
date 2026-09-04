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
