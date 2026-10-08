import { ApiError, withErrorId } from "../../../api/workspace";

export function apiMessage(error: unknown): string {
  if (error instanceof ApiError) {
    const base =
      error.body.code === "revision_conflict"
        ? "This record changed after you loaded it. Reload before trying again."
        : error.body.message;
    return withErrorId(error, base);
  }
  if (error instanceof Error) return error.message;
  return "ISK Works could not complete the request.";
}
