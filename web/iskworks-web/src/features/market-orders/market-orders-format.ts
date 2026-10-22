import type { OrderRange, StandingStatus } from "../../api/market-orders";
import type { Tone } from "../../components/primitives";

export function statusLabel(status: StandingStatus): string {
  return { best: "Best", tied: "Tied", outbid: "Outbid", unknown: "Unknown" }[status];
}

export function statusTone(status: StandingStatus): Tone {
  return ({ best: "positive", tied: "warning", outbid: "danger", unknown: "muted" } as const)[status];
}

/** Outbid first, then undecided, tied and best. */
export function statusRank(status: StandingStatus): number {
  return { outbid: 0, unknown: 1, tied: 2, best: 3 }[status];
}

export function rangeLabel(range: OrderRange): string {
  switch (range.kind) {
    case "station":
      return "Station";
    case "solarSystem":
      return "System";
    case "jumps":
      return `${range.jumps} ${range.jumps === 1 ? "jump" : "jumps"}`;
    case "region":
      return "Region";
  }
}

/**
 * An order price with every digit a competitor can differ by: the market
 * keeps four significant digits, so 9,210 and 9,215 stay distinct while
 * 2.149b reads compactly.
 */
export function formatPrice(value: string): string {
  const amount = Number(value);
  if (!Number.isFinite(amount)) return value;
  if (Math.abs(amount) < 1_000) {
    return new Intl.NumberFormat("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(amount);
  }
  return new Intl.NumberFormat("en-US", { notation: "compact", maximumSignificantDigits: 4 }).format(amount);
}

export function isNegative(value: string | null): boolean {
  return value !== null && value.trim().startsWith("-");
}

export function percentLabel(value: string | null, signed = false): string {
  if (value === null) return "—";
  const amount = Number(value);
  const sign = signed && amount > 0 ? "+" : "";
  return `${sign}${amount.toFixed(2)}%`;
}

/** "3m 12s" until `iso`, or `null` once it has passed. */
export function countdown(iso: string, nowMs: number): string | null {
  const seconds = Math.ceil((Date.parse(iso) - nowMs) / 1000);
  if (seconds <= 0) return null;
  const minutes = Math.floor(seconds / 60);
  return minutes > 0 ? `${minutes}m ${seconds % 60}s` : `${seconds}s`;
}

export function ago(iso: string | null, nowMs: number): string {
  if (!iso) return "never";
  const minutes = Math.max(0, Math.floor((nowMs - Date.parse(iso)) / 60_000));
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours}h ${minutes % 60}m ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/** "62d 4h" until expiry, or "Expired". */
export function expiresIn(iso: string, nowMs: number): string {
  const hours = Math.floor((Date.parse(iso) - nowMs) / 3_600_000);
  if (hours < 0) return "Expired";
  const days = Math.floor(hours / 24);
  return days > 0 ? `${days}d ${hours % 24}h` : `${hours}h`;
}
