import { requestJson } from "./workspace";

export interface HealthResponse {
  status: string;
  version: string;
  /**
   * Non-production deployment marker (`ISKWORKS_ENVIRONMENT_LABEL` on the
   * API, e.g. "STAGE"). Absent on production.
   */
  environmentLabel?: string;
}

export async function getHealth(): Promise<HealthResponse> {
  return requestJson<HealthResponse>("/api/health");
}

/** `GET /api/esi/status`: whether ESI is paused because EVE's servers are down. */
export interface EsiStatusResponse {
  downtime: boolean;
  /** While in downtime, roughly when to ask again. */
  retryAfterSeconds: number | null;
}

export async function getEsiStatus(): Promise<EsiStatusResponse> {
  return requestJson<EsiStatusResponse>("/api/esi/status");
}

export interface MaintenanceNotice {
  message: string;
  /** ISO timestamp. The API stops returning the notice at this time. */
  startsAt: string;
}

/** Public, so the notice also reaches the sign-in screen. */
export async function getMaintenanceNotice(): Promise<MaintenanceNotice | null> {
  const response = await requestJson<{ notice?: MaintenanceNotice | null }>(
    "/api/maintenance-notice",
  );
  return response.notice ?? null;
}
