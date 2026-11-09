import { apiMessage as sharedApiMessage } from "../../../api/api-error";

export type LoadState<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

export function apiMessage(error: unknown): string {
  return sharedApiMessage(error, "Opportunities request failed.");
}
