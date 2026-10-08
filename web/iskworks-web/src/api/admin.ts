import { requestJson } from "./workspace";

export type InviteStatus = "active" | "disabled" | "expired" | "exhausted";

export interface AdminInvite {
  id: string;
  status: InviteStatus;
  createdAt: string;
  expiresAt: string | null;
  disabledAt: string | null;
  maxUses: number;
  useCount: number;
  note: string | null;
  /** An encrypted copy is stored, so the code can be revealed again. */
  revealable: boolean;
}

export interface CreateInviteRequest {
  maxUses?: number;
  expiresAt?: string;
  note?: string;
}

export interface CreatedInvite {
  /** Plaintext code — returned exactly once, never listed again. */
  code: string;
  invite: AdminInvite;
}

export interface AdminUser {
  id: string;
  /** The EVE character they signed in with. */
  characterId: number;
  characterName: string;
  createdAt: string;
  lastLoginAt: string;
  /** Characters currently linked to their workspace. */
  characterCount: number;
  charactersNeedingAttention: number;
  activeSessions: number;
  /** Set when an admin has blocked this account from signing in. */
  disabledAt: string | null;
  /** A configured admin; cannot be disabled or deleted from the UI. */
  isAdmin: boolean;
  /** The signed-in admin's own account. */
  isCurrentUser: boolean;
}

export function listUsers(): Promise<AdminUser[]> {
  return requestJson<AdminUser[]>("/api/admin/users");
}

export async function disableUser(id: string): Promise<void> {
  await requestJson<null>(`/api/admin/users/${encodeURIComponent(id)}/disable`, { method: "POST" });
}

export async function enableUser(id: string): Promise<void> {
  await requestJson<null>(`/api/admin/users/${encodeURIComponent(id)}/enable`, { method: "POST" });
}

/** Removes the account; `confirmCharacterName` must match, the server checks too. */
export async function deleteUser(id: string, confirmCharacterName: string): Promise<void> {
  await requestJson<null>(`/api/admin/users/${encodeURIComponent(id)}`, {
    method: "DELETE",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ confirmCharacterName }),
  });
}

/** Workspaces left behind by users deleted before full erase existed. */
export async function countOrphanedWorkspaces(): Promise<number> {
  const { count } = await requestJson<{ count: number }>("/api/admin/orphaned-workspaces");
  return count;
}

export async function eraseOrphanedWorkspaces(): Promise<number> {
  const { erased } = await requestJson<{ erased: number }>(
    "/api/admin/orphaned-workspaces/erase",
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ confirmation: "erase" }),
    },
  );
  return erased;
}

export function listInvites(): Promise<AdminInvite[]> {
  return requestJson<AdminInvite[]>("/api/admin/invites");
}

export function createInvite(request: CreateInviteRequest): Promise<CreatedInvite> {
  return requestJson<CreatedInvite>("/api/admin/invites", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(request),
  });
}

export async function revealInviteCode(id: string): Promise<{ code: string }> {
  return requestJson<{ code: string }>(`/api/admin/invites/${encodeURIComponent(id)}/code`);
}

export async function deleteInvite(id: string): Promise<void> {
  await requestJson<null>(`/api/admin/invites/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export async function disableInvite(id: string): Promise<void> {
  await requestJson<null>(`/api/admin/invites/${encodeURIComponent(id)}/disable`, {
    method: "POST",
  });
}
