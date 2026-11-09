import type { ReactNode } from "react";

import { apiMessage as sharedApiMessage } from "../../../api/api-error";

export const number = new Intl.NumberFormat("en-US");

// Thin wrapper: this page's fallback for non-API failures is the generic
// text, never a thrown Error's own message.
export function apiMessage(error: unknown): string {
  return sharedApiMessage(error, "ISK Works could not complete the request.");
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
