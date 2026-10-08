import { formatIskSummary, formatSignedIsk } from "../../../components/money";

export function formatMoney(value: string | null): string {
  return value === null ? "Incomplete" : formatIskSummary(value);
}

export function signedMoney(value: string | null): string {
  return value === null ? "Incomplete" : formatSignedIsk(value);
}

export function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "unknown";
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

// Coarse "how long ago" label for the Builds library card's updated line
// -- deliberately low-resolution (today / Nd / Nw / Nmo / Ny). Per this
// codebase's per-feature formatter convention (see characters-formatters.ts
// / market-browser/shared.tsx), kept local rather than shared.
export function formatRelativeAge(value: string): string {
  const ageMs = Date.now() - new Date(value).getTime();
  const days = Math.floor(ageMs / 86_400_000);
  if (Number.isNaN(days) || days <= 0) return "today";
  if (days === 1) return "1d ago";
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.floor(days / 7)}w ago`;
  if (days < 365) return `${Math.floor(days / 30)}mo ago`;
  return `${Math.floor(days / 365)}y ago`;
}

export function formatDuration(seconds: number | null): string {
  if (!seconds) return "Duration unavailable";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return hours ? `${hours}h ${minutes}m` : `${minutes}m`;
}

/** A percentage for display, with the `%` sign. Trims the API's fixed
 * 6-decimal strings (`"1.000000"` -> `"1%"`, `"2.530000"` -> `"2.5%"`) to at
 * most `maxFractionDigits` decimals. `null` -> "Unavailable". */
export function formatPercent(value: string | number | null, maxFractionDigits = 1): string {
  if (value === null) return "Unavailable";
  const n = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(n)) return typeof value === "string" ? `${value}%` : "Unavailable";
  return `${n.toLocaleString("en-US", { maximumFractionDigits: maxFractionDigits })}%`;
}

export function splitCamel(value: string): string {
  return value
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/^./, (letter) => letter.toUpperCase());
}
