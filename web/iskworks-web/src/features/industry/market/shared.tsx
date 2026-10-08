import type { ReactNode } from "react";

import { ApiError, withErrorId } from "../../../api/workspace";

export const number = new Intl.NumberFormat("en-US");

// Kept local rather than merged with features/industry/shared/api-error.ts's
// apiMessage: that one special-cases revision_conflict with different
// copy and falls back to `error.message` for a generic Error, neither of
// which this page has ever done.
export function apiMessage(error: unknown): string {
  return error instanceof ApiError ? withErrorId(error, error.body.message) : "ISK Works could not complete the request.";
}

// Kept local: unlike the other formatDate variants elsewhere in the app,
// this one uses plain toLocaleString() output and an "Unknown" (not
// "Never") fallback.
export function formatDate(value: string | null): string {
  return value ? new Date(value).toLocaleString() : "Unknown";
}

export function Metric({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="min-w-0">
      <span className="iw-eyebrow">{label}</span>
      <strong className="mt-1 block break-words font-mono text-sm">{typeof value === "number" ? number.format(value) : value}</strong>
    </div>
  );
}
