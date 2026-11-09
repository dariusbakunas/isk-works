import { apiMessage as sharedApiMessage } from "../../api/api-error";

// Thin wrapper: a fixed, EVE-specific fallback for non-API failures.
export function apiMessage(error: unknown): string {
  return sharedApiMessage(error, "The EVE integration request could not be completed.");
}

// Unlike lib/formatting.ts's formatDate, this accepts
// a possibly-null timestamp (e.g. a connection that has never synced) and
// reports "Never" rather than requiring the caller to guard every call site.
export function formatDate(value: string | null): string {
  return value ? new Intl.DateTimeFormat("en-US", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)) : "Never";
}
