import { ApiError, withErrorId } from "../../../api/workspace";
import { formatIskCompact, formatIskSummary } from "../../../components/money";
import type { Money } from "../../../api/industry";

export function apiMessage(error: unknown): string {
  return error instanceof ApiError ? withErrorId(error, error.body.message) : "ISK Works could not complete the request.";
}

export function formatAge(value: string | null): string {
  if (value === null) return "No observations yet";
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

// Minute-level remaining duration until an order expires, e.g. "2d 21h 14m"
// or "42m" -- no second-level ticking (the spec explicitly doesn't need it,
// just a page-render-time calculation), and every applicable unit is always
// shown together rather than being dropped for long durations, matching the
// EVE Market Browser reference this was checked against.
export function formatExpiresIn(expiresAt: string): string {
  const remainingMs = new Date(expiresAt).getTime() - Date.now();
  if (remainingMs <= 0) return "Expired";
  const totalMinutes = Math.floor(remainingMs / 60_000);
  const days = Math.floor(totalMinutes / (24 * 60));
  const hours = Math.floor((totalMinutes % (24 * 60)) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) return `${days}d ${hours}h ${minutes}m`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

export function MoneyCell({ value }: { value: Money | null }) {
  if (value === null) {
    return <span className="text-muted">—</span>;
  }
  return (
    <span className="whitespace-nowrap font-mono tabular-nums" title={formatIskSummary(value)}>
      {formatIskCompact(value)}
    </span>
  );
}
