// Pure formatting/threshold rules for the Planetary Interaction page, ported
// from the design prototype (Planetary view).
// Feature-local by convention, like the other per-feature formatters.

export type Tone = "ok" | "warn" | "danger";

const HOUR_MS = 3_600_000;

/** "4d 4h", "2.2h", or whole days from a week up ("9d"). */
export function formatHours(hours: number): string {
  if (!Number.isFinite(hours) || hours <= 0) return "0.0h";
  if (hours < 24) return `${hours.toFixed(1)}h`;
  if (hours >= 24 * 7) return `${Math.round(hours / 24)}d`;
  const rounded = Math.round(hours);
  return `${Math.floor(rounded / 24)}d ${rounded % 24}h`;
}

export interface Countdown {
  label: string;
  tone: Tone;
  expired: boolean;
  /** Exact UTC time for the `title` tooltip. */
  absolute: string;
}

export function countdown(iso: string, nowMs: number): Countdown {
  const at = new Date(iso);
  const hours = (at.getTime() - nowMs) / HOUR_MS;
  const absolute = at.toUTCString();
  if (hours <= 0) return { label: "Expired", tone: "danger", expired: true, absolute };
  return { label: formatHours(hours), tone: hours < 24 ? "warn" : "ok", expired: false, absolute };
}

export function storageTone(percent: number): Tone {
  if (percent >= 90) return "danger";
  if (percent >= 70) return "warn";
  return "ok";
}

export function importTone(lastsHours: number): Tone {
  if (lastsHours <= 0) return "danger";
  if (lastsHours < 24) return "warn";
  return "ok";
}

export function securityTone(security: number): "high" | "low" | "null" {
  if (security >= 0.45) return "high";
  if (security >= 0) return "low";
  return "null";
}

/** 1.2K / 3.4M for unit rates and quantities. */
export function formatQuantity(value: number): string {
  if (Math.abs(value) >= 1e6) return `${(value / 1e6).toFixed(1)}M`;
  if (Math.abs(value) >= 1e3) return `${(value / 1e3).toFixed(1)}K`;
  return value.toFixed(0);
}

export const toneText: Record<Tone, string> = {
  ok: "text-muted",
  warn: "text-warning",
  danger: "text-destructive",
};

/** Planet-type palette: letter chip foreground/background. */
export const PLANET_TYPES: Record<string, { short: string; label: string; color: string; background: string }> = {
  barren: { short: "B", label: "Barren", color: "#9a7a4a", background: "#1a1408" },
  gas: { short: "G", label: "Gas", color: "#4a9aa0", background: "#081e20" },
  lava: { short: "L", label: "Lava", color: "#c85a20", background: "#200e08" },
  ice: { short: "I", label: "Ice", color: "#88aacc", background: "#0e1828" },
  oceanic: { short: "O", label: "Oceanic", color: "#2a6aaa", background: "#081420" },
  plasma: { short: "P", label: "Plasma", color: "#8a5ab4", background: "#140a20" },
  storm: { short: "S", label: "Storm", color: "#5a7aaa", background: "#0e1420" },
  temperate: { short: "T", label: "Temperate", color: "#4a8f5a", background: "#081810" },
};

const STALE_AFTER_MINUTES = 60;

export function isStale(observedAt: string | null, nowMs: number): boolean {
  if (observedAt === null) return false;
  return nowMs - new Date(observedAt).getTime() > STALE_AFTER_MINUTES * 60_000;
}
