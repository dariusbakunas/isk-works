import { ApiError } from "./workspace";

/** What a module's requests report when the API can't be reached, and when
 * an error response carries no error body of its own. */
export interface RequestMessages {
  unavailable: string;
  failed: string;
}

/** A JSON request against the ISK Works API: sends cookies and
 * `Accept: application/json`, and turns a network failure or a non-2xx
 * response into an `ApiError`. */
export function jsonRequest(messages: RequestMessages) {
  return async function request<T>(path: string, init?: RequestInit): Promise<T> {
    const baseUrl = (import.meta.env.VITE_API_BASE_URL ?? "").replace(/\/$/, "");
    let response: Response;
    try {
      response = await fetch(`${baseUrl}${path}`, {
        headers: { Accept: "application/json", ...init?.headers },
        credentials: "include",
        ...init,
      });
    } catch {
      throw new ApiError(0, {
        code: "api_unavailable",
        message: messages.unavailable,
      });
    }
    if (!response.ok) {
      const payload = await response.json().catch(() => null);
      throw new ApiError(response.status, payload?.error ?? {
        code: "api_error",
        message: messages.failed,
      });
    }
    return response.json() as Promise<T>;
  };
}
