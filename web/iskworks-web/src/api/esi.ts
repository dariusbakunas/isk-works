import { jsonRequest } from "./json-request";

export type ConnectionStatus =
  | "connected"
  | "needsReconnection"
  | "missingScope"
  | "temporarilyUnavailable"
  | "disconnected";

export interface ConnectedCharacter {
  id: string;
  ownerId: string;
  eveCharacterId: number;
  characterName: string;
  status: ConnectionStatus;
  grantedScopes: string[];
  accessTokenExpiresAt: string | null;
  lastRefreshedAt: string | null;
  lastErrorCode: string | null;
  lastErrorMessage: string | null;
  connectedAt: string;
  updatedAt: string;
  disconnectedAt: string | null;
  revision: number;
}

export function getConnection(id: string): Promise<ConnectedCharacter> {
  return request(`/api/eve/connections/${id}`);
}

export function beginAuthorization(): Promise<{
  authorizationUrl: string;
  fixtureMode: boolean;
  connection: ConnectedCharacter | null;
  requestedScopes: string[];
}> {
  return request("/api/eve/connections/authorize", json("POST"));
}

export function disconnectConnection(id: string): Promise<ConnectedCharacter> {
  return request(`/api/eve/connections/${id}/disconnect`, json("POST"));
}

function json(method: string, body?: unknown): RequestInit {
  return {
    method,
    headers: { "content-type": "application/json", Accept: "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  };
}

const request = jsonRequest({
  unavailable: "ISK Works API is unavailable.",
  failed: "EVE integration request failed.",
});
