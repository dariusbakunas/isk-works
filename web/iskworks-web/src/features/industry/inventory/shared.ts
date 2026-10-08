import { ApiError, withErrorId } from "../../../api/workspace";

export type LoadState<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

export function apiMessage(error: unknown): string {
  return error instanceof ApiError ? withErrorId(error, error.body?.message ?? error.message) : "Inventory request failed.";
}
