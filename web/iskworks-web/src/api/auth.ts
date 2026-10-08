import { requestJson } from "./workspace";

export interface SessionResponse {
  authenticated: boolean;
  characterName: string | null;
  workspaceId: string | null;
  /**
   * Whether this character may open the Admin section
   * (`ISKWORKS_ADMIN_CHARACTER_IDS`). Nav visibility only — the API
   * enforces `/api/admin/*` itself. Optional for older responses.
   */
  isAdmin?: boolean;
  /**
   * Whether the backend enforces invite-only new-user onboarding
   * (`ISKWORKS_INVITE_REQUIRED`). Safe public config — one boolean, no
   * counts or ids. Drives whether the sign-in screen shows the invite field.
   */
  inviteRequired: boolean;
}

export interface BeginLoginResponse {
  authorizationUrl: string;
}

export async function getSession(): Promise<SessionResponse> {
  return requestJson<SessionResponse>("/api/auth/session");
}

/**
 * Start EVE SSO sign-in. `inviteCode` is the raw string the user typed, when
 * they are joining the alpha with an invite; omit it for returning users and
 * whenever invite mode is off. It travels only in this POST body over HTTPS
 * — never a query string — and the server stores only its hash.
 */
export async function beginLogin(inviteCode?: string): Promise<BeginLoginResponse> {
  const trimmed = inviteCode?.trim();
  return requestJson<BeginLoginResponse>("/api/auth/eve/login", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(trimmed ? { inviteCode: trimmed } : {}),
  });
}

export async function logout(): Promise<void> {
  await requestJson<void>("/api/auth/logout", { method: "POST" });
}
