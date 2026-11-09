import { ApiError, withErrorId } from "./workspace";

const DEFAULT_FALLBACK = "ISK Works could not complete the request.";

/** The user-facing text for a failed request.
 *
 * An `ApiError` shows the backend's curated message, plus its Error ID when
 * the backend redacted an internal failure (see `withErrorId`). A revision
 * conflict gets fixed "reload" copy.
 *
 * Anything else (a network failure, a thrown bug) shows `fallback` when the
 * caller gives one. Without a fallback, an `Error` shows its own message and
 * any other value the generic text. */
export function apiMessage(error: unknown, fallback?: string): string {
  if (error instanceof ApiError) {
    const base =
      error.body.code === "revision_conflict"
        ? "This record changed after you loaded it. Reload before trying again."
        : error.body.message;
    return withErrorId(error, base);
  }
  if (fallback !== undefined) return fallback;
  if (error instanceof Error) return error.message;
  return DEFAULT_FALLBACK;
}
