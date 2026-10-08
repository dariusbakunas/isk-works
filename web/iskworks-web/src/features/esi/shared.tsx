import { ApiError, withErrorId } from "../../api/workspace";

// A fixed, EVE-specific fallback rather than the app-wide apiMessage's
// generic text -- kept local to this feature rather than merged with
// features/industry/shared/api-error.ts's apiMessage, which has different
// fallback copy for the revision-conflict and non-ApiError cases.
export function apiMessage(error: unknown): string {
  return error instanceof ApiError ? withErrorId(error, error.body.message) : "The EVE integration request could not be completed.";
}

// Unlike features/industry/shared/formatting.ts's formatDate, this accepts
// a possibly-null timestamp (e.g. a connection that has never synced) and
// reports "Never" rather than requiring the caller to guard every call site.
export function formatDate(value: string | null): string {
  return value ? new Intl.DateTimeFormat("en-US", { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)) : "Never";
}
