import { reportBackendError } from "../observability/logrocket";

export interface WorkspaceDto {
  id: string;
  name: string;
  ownerId: string;
  createdAt: string;
  updatedAt: string;
}

export interface OwnerDto {
  id: string;
  workspaceId: string;
  kind: "manual";
  displayName: string;
  hidden: boolean;
}

export interface WorkspaceStateResponse {
  configured: boolean;
  workspace: WorkspaceDto | null;
  owner: OwnerDto | null;
  version: string;
}

export interface ApiErrorBody {
  code: string;
  message: string;
  fields?: Record<string, string>;
  /** Present on infrastructure/internal failures whose real detail was redacted. */
  retryable?: boolean;
  /** Ties a redacted internal failure to its server-side log line. */
  correlationId?: string;
}

export class ApiError extends Error {
  readonly status: number;
  readonly body: ApiErrorBody;

  constructor(status: number, body: ApiErrorBody) {
    super(body.message);
    this.status = status;
    this.body = body;

    // Bridge a redacted internal failure's correlation id into the
    // LogRocket timeline. No-ops unless replay is running; never allowed
    // to disturb error handling.
    if (body.correlationId) {
      try {
        reportBackendError(body.correlationId, status);
      } catch {
        /* observability must not break the app */
      }
    }
  }
}

/** `message`, plus the Error ID the backend attaches when it has redacted an
 * internal failure to a curated message, so a user can quote it in a bug
 * report. Ordinary validation/not-found/conflict errors carry no ID. */
export function withErrorId(error: ApiError, message: string): string {
  return error.body.correlationId ? `${message} (Error ID: ${error.body.correlationId})` : message;
}

export async function getWorkspaceState(): Promise<WorkspaceStateResponse> {
  return requestJson<WorkspaceStateResponse>("/api/workspace");
}

export async function createWorkspace(name: string): Promise<WorkspaceStateResponse> {
  return requestJson<WorkspaceStateResponse>("/api/workspace", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name }),
  });
}

export async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const baseUrl = import.meta.env.VITE_API_BASE_URL ?? "";
  let response: Response;

  try {
    response = await fetch(`${baseUrl}${path}`, { credentials: "include", ...init });
  } catch {
    throw new ApiError(0, {
      code: "api_unavailable",
      message: "ISK Works API is unavailable. Check that the backend is running.",
    });
  }

  const body = await response.json().catch(() => null);

  if (!response.ok) {
    throw new ApiError(response.status, body?.error ?? {
      code: "api_error",
      message: "ISK Works could not complete the request.",
    });
  }

  return body as T;
}
