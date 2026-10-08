import { ApiError } from "../workspace";

export function json(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { "content-type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
  };
}

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
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
      message: "ISK Works API is unavailable. Check that the backend is running.",
    });
  }
  if (response.status === 204) return undefined as T;
  const body = await response.json().catch(() => null);
  if (!response.ok) {
    throw new ApiError(response.status, body?.error ?? {
      code: "api_error",
      message: "ISK Works could not complete the request.",
    });
  }
  return body as T;
}
