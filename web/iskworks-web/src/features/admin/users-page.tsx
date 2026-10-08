import { useCallback, useEffect, useState } from "react";
import {
  countOrphanedWorkspaces,
  deleteUser,
  disableUser,
  enableUser,
  eraseOrphanedWorkspaces,
  listUsers,
  type AdminUser,
} from "../../api/admin";
import {
  OperationalTable,
  OperationalTableRow,
  type OperationalColumn,
} from "../../components/operational-table";
import {
  ConfirmDialog,
  EmptyState,
  InlineAlert,
  Panel,
  StatusBadge,
} from "../../components/primitives";
import { CharacterName } from "../../observability/private";
import { apiMessage } from "../industry/shared/api-error";
import { formatDate } from "../industry/shared/formatting";
import { TypeToConfirmDialog } from "./type-to-confirm-dialog";

type AsyncState<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

const COLUMNS: OperationalColumn[] = [
  { key: "pilot", label: "Signed in as", width: "minmax(200px,1fr)", sticky: true },
  { key: "characters", label: "Characters", width: "170px" },
  { key: "sessions", label: "Sessions", width: "90px", align: "right", numeric: true, hideBelow: "tablet" },
  { key: "created", label: "Registered", width: "160px", hideBelow: "desktop" },
  { key: "lastLogin", label: "Last login", width: "160px" },
  { key: "actions", label: "", width: "190px", align: "right" },
];

const ACTIVE_WINDOW_MS = 7 * 24 * 60 * 60 * 1000;

export function UsersPage() {
  const [state, setState] = useState<AsyncState<AdminUser[]>>({ status: "loading" });
  const [pending, setPending] = useState<
    { kind: "disable" | "delete"; user: AdminUser } | { kind: "erase-orphans" } | null
  >(null);
  const [orphans, setOrphans] = useState(0);
  const [actionError, setActionError] = useState("");
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      setState({ status: "ready", data: await listUsers() });
    } catch (error) {
      setState({ status: "error", message: apiMessage(error) });
      return;
    }
    // Secondary info: a failure here must not hide the user list.
    try {
      setOrphans(await countOrphanedWorkspaces());
    } catch {
      setOrphans(0);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function run(action: () => Promise<void>) {
    setActionError("");
    setBusy(true);
    try {
      await action();
      await reload();
    } catch (error) {
      setActionError(apiMessage(error));
    } finally {
      setBusy(false);
      setPending(null);
    }
  }

  if (state.status === "loading") return <Panel>Loading users...</Panel>;
  if (state.status === "error") {
    return <InlineAlert title="Users unavailable">{state.message}</InlineAlert>;
  }
  if (state.data.length === 0) {
    return (
      <Panel>
        <EmptyState title="No users yet">Nobody has signed in.</EmptyState>
      </Panel>
    );
  }

  const users = state.data;
  const now = Date.now();
  const activeThisWeek = users.filter(
    (user) => now - new Date(user.lastLoginAt).getTime() <= ACTIVE_WINDOW_MS,
  ).length;
  const totalCharacters = users.reduce((sum, user) => sum + user.characterCount, 0);

  return (
    <div className="grid gap-4">
      {actionError ? <InlineAlert title="User action failed">{actionError}</InlineAlert> : null}
      {orphans > 0 ? (
        <Panel>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-sm">
              <strong>{orphans}</strong> leftover {orphans === 1 ? "workspace" : "workspaces"} from
              users deleted before full erase existed. Their data is unreachable but still stored.
            </p>
            <button
              className="iw-button-secondary px-2 py-1 text-xs"
              disabled={busy}
              onClick={() => setPending({ kind: "erase-orphans" })}
              type="button"
            >
              Erase leftover data
            </button>
          </div>
        </Panel>
      ) : null}
      <dl className="grid grid-cols-3 gap-2">
        <Stat label="Users" value={users.length} />
        <Stat label="Active in 7 days" value={activeThisWeek} />
        <Stat label="Linked characters" value={totalCharacters} />
      </dl>
      <OperationalTable
        ariaLabel="Users"
        columns={COLUMNS}
        onSelectRow={() => undefined}
        selectedRowKey={null}
      >
        <tbody>
          {users.map((user) => (
            <OperationalTableRow
              cells={{
                pilot: (
                  <span className="flex min-w-0 items-center gap-2">
                    <CharacterName className="truncate font-medium" name={user.characterName} />
                    {user.isCurrentUser ? <StatusBadge>You</StatusBadge> : null}
                    {user.isAdmin && !user.isCurrentUser ? <StatusBadge>Admin</StatusBadge> : null}
                    {user.disabledAt ? <StatusBadge>Disabled</StatusBadge> : null}
                  </span>
                ),
                characters: (
                  <span className="flex items-center gap-2">
                    <span className="font-mono tabular-nums">{user.characterCount}</span>
                    {user.charactersNeedingAttention > 0 ? (
                      <StatusBadge>{user.charactersNeedingAttention} need attention</StatusBadge>
                    ) : null}
                  </span>
                ),
                sessions: user.activeSessions,
                created: <span className="text-muted">{formatDate(user.createdAt)}</span>,
                lastLogin: <span className="text-muted">{formatDate(user.lastLoginAt)}</span>,
                actions:
                  user.isAdmin || user.isCurrentUser ? null : (
                    <span className="flex justify-end gap-1">
                      {user.disabledAt ? (
                        <button
                          aria-label={`Enable ${user.characterName}`}
                          className="iw-button-secondary px-2 py-1 text-xs"
                          disabled={busy}
                          onClick={() => void run(() => enableUser(user.id))}
                          type="button"
                        >
                          Enable
                        </button>
                      ) : (
                        <button
                          aria-label={`Disable ${user.characterName}`}
                          className="iw-button-secondary px-2 py-1 text-xs"
                          onClick={() => setPending({ kind: "disable", user })}
                          type="button"
                        >
                          Disable
                        </button>
                      )}
                      <button
                        aria-label={`Delete ${user.characterName}`}
                        className="iw-button-secondary px-2 py-1 text-xs"
                        onClick={() => setPending({ kind: "delete", user })}
                        type="button"
                      >
                        Delete
                      </button>
                    </span>
                  ),
              }}
              interactive={false}
              key={user.id}
              rowKey={user.id}
            />
          ))}
        </tbody>
      </OperationalTable>
      <ConfirmDialog
        confirmLabel="Disable user"
        onCancel={() => setPending(null)}
        onConfirm={() => {
          const target = pending;
          if (target?.kind === "disable") void run(() => disableUser(target.user.id));
        }}
        open={pending?.kind === "disable"}
        title="Disable this user?"
      >
        They are signed out immediately and cannot sign in again until you enable them. Nothing is
        deleted.
      </ConfirmDialog>
      <TypeToConfirmDialog
        busy={busy}
        confirmLabel="Delete user"
        expected={pending?.kind === "delete" ? pending.user.characterName : ""}
        onCancel={() => setPending(null)}
        onConfirm={(typed) => {
          const target = pending;
          if (target?.kind === "delete") void run(() => deleteUser(target.user.id, typed));
        }}
        open={pending?.kind === "delete"}
        title="Delete this user?"
      >
        <p>
          This permanently erases their account <strong>and everything in their workspace</strong>:
          inventory and its history, builds, orders and tickets, market data, finance data,
          connected characters and ESI tokens. It cannot be undone.
        </p>
        <p className="mt-2">
          They can register again, with a new invite, as a brand-new user.
        </p>
      </TypeToConfirmDialog>
      <TypeToConfirmDialog
        busy={busy}
        confirmLabel="Erase leftover data"
        expected="ERASE"
        onCancel={() => setPending(null)}
        onConfirm={() => void run(async () => void (await eraseOrphanedWorkspaces()))}
        open={pending?.kind === "erase-orphans"}
        title="Erase leftover workspaces?"
      >
        <p>
          Permanently erases {orphans} {orphans === 1 ? "workspace" : "workspaces"} that no user can
          reach any more, with all their data. It cannot be undone.
        </p>
      </TypeToConfirmDialog>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="iw-panel p-3">
      <dt className="text-xs text-muted">{label}</dt>
      <dd className="mt-1 font-mono text-xl tabular-nums">{value.toLocaleString()}</dd>
    </div>
  );
}
