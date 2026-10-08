import { useCallback, useEffect, useState } from "react";
import {
  createInvite,
  deleteInvite,
  disableInvite,
  listInvites,
  revealInviteCode,
  type AdminInvite,
  type CreatedInvite,
  type InviteStatus,
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
import { Private } from "../../observability/private";
import { apiMessage } from "../industry/shared/api-error";
import { Field } from "../industry/shared/field";
import { formatDate } from "../industry/shared/formatting";

type AsyncState<T> =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; data: T };

const COLUMNS: OperationalColumn[] = [
  { key: "note", label: "Note", width: "minmax(180px,1fr)", sticky: true },
  { key: "status", label: "Status", width: "110px" },
  { key: "uses", label: "Uses", width: "80px", align: "right", numeric: true },
  { key: "created", label: "Created", width: "160px", hideBelow: "tablet" },
  { key: "expires", label: "Expires", width: "160px", hideBelow: "desktop" },
  { key: "actions", label: "", width: "230px", align: "right" },
];

const STATUS_LABEL: Record<InviteStatus, string> = {
  active: "Active",
  disabled: "Disabled",
  expired: "Expired",
  exhausted: "Used up",
};

export function InvitesPage() {
  const [state, setState] = useState<AsyncState<AdminInvite[]>>({ status: "loading" });
  const [created, setCreated] = useState<CreatedInvite | null>(null);
  const [revealed, setRevealed] = useState<{ note: string | null; code: string } | null>(null);
  const [pending, setPending] = useState<{ kind: "disable" | "delete"; invite: AdminInvite } | null>(
    null,
  );
  const [actionError, setActionError] = useState("");

  const reload = useCallback(async () => {
    try {
      setState({ status: "ready", data: await listInvites() });
    } catch (error) {
      setState({ status: "error", message: apiMessage(error) });
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function reveal(invite: AdminInvite) {
    setActionError("");
    try {
      const { code } = await revealInviteCode(invite.id);
      setRevealed({ note: invite.note, code });
    } catch (error) {
      setActionError(apiMessage(error));
    }
  }

  async function confirmPending() {
    const action = pending;
    setPending(null);
    if (!action) return;
    setActionError("");
    try {
      if (action.kind === "disable") await disableInvite(action.invite.id);
      else await deleteInvite(action.invite.id);
      await reload();
    } catch (error) {
      setActionError(apiMessage(error));
    }
  }

  return (
    <div className="grid gap-4">
      <CreateInviteForm
        onCreated={(result) => {
          setCreated(result);
          void reload();
        }}
      />
      {created ? (
        <CodePanel
          code={created.code}
          heading="Invite created"
          hint={
            created.invite.revealable
              ? "You can reveal this code again from the list while the invite is active."
              : "Copy this code now. It is not stored and cannot be shown again."
          }
          onDismiss={() => setCreated(null)}
        />
      ) : null}
      {revealed ? (
        <CodePanel
          code={revealed.code}
          heading={revealed.note ? `Invite code for ${revealed.note}` : "Invite code"}
          onDismiss={() => setRevealed(null)}
        />
      ) : null}
      {actionError ? <InlineAlert title="Invite action failed">{actionError}</InlineAlert> : null}
      {state.status === "loading" ? <Panel>Loading invites...</Panel> : null}
      {state.status === "error" ? (
        <InlineAlert title="Invites unavailable">{state.message}</InlineAlert>
      ) : null}
      {state.status === "ready" && state.data.length === 0 ? (
        <Panel>
          <EmptyState title="No invites yet">Create one above to let someone register.</EmptyState>
        </Panel>
      ) : null}
      {state.status === "ready" && state.data.length > 0 ? (
        <OperationalTable
          ariaLabel="Invites"
          columns={COLUMNS}
          onSelectRow={() => undefined}
          selectedRowKey={null}
        >
          <tbody>
            {state.data.map((invite) => (
              <OperationalTableRow
                cells={{
                  note: <Private as="span">{invite.note ?? "—"}</Private>,
                  status: <StatusBadge>{STATUS_LABEL[invite.status]}</StatusBadge>,
                  uses: `${invite.useCount}/${invite.maxUses}`,
                  created: <span className="text-muted">{formatDate(invite.createdAt)}</span>,
                  expires: (
                    <span className="text-muted">
                      {invite.expiresAt ? formatDate(invite.expiresAt) : "Never"}
                    </span>
                  ),
                  actions: (
                    <span className="flex justify-end gap-1">
                      {invite.status === "active" && invite.revealable ? (
                        <button
                          aria-label={`Reveal invite ${invite.note ?? invite.id}`}
                          className="iw-button-secondary px-2 py-1 text-xs"
                          onClick={() => void reveal(invite)}
                          type="button"
                        >
                          Reveal
                        </button>
                      ) : null}
                      {invite.status === "active" ? (
                        <button
                          aria-label={`Disable invite ${invite.note ?? invite.id}`}
                          className="iw-button-secondary px-2 py-1 text-xs"
                          onClick={() => setPending({ kind: "disable", invite })}
                          type="button"
                        >
                          Disable
                        </button>
                      ) : null}
                      <button
                        aria-label={`Delete invite ${invite.note ?? invite.id}`}
                        className="iw-button-secondary px-2 py-1 text-xs"
                        onClick={() => setPending({ kind: "delete", invite })}
                        type="button"
                      >
                        Delete
                      </button>
                    </span>
                  ),
                }}
                interactive={false}
                key={invite.id}
                rowKey={invite.id}
              />
            ))}
          </tbody>
        </OperationalTable>
      ) : null}
      <ConfirmDialog
        confirmLabel={pending?.kind === "delete" ? "Delete invite" : "Disable invite"}
        onCancel={() => setPending(null)}
        onConfirm={() => void confirmPending()}
        open={pending !== null}
        title={pending?.kind === "delete" ? "Delete this invite?" : "Disable this invite?"}
      >
        {pending?.kind === "delete"
          ? "The invite is removed permanently, along with its usage history, and its code stops working immediately. Anyone who already registered with it keeps their account."
          : "The code stops working immediately, but the invite stays in the list and its history is kept. Anyone who already registered with it keeps their account."}
      </ConfirmDialog>
    </div>
  );
}

function CreateInviteForm({ onCreated }: { onCreated: (created: CreatedInvite) => void }) {
  const [maxUses, setMaxUses] = useState("1");
  const [expiresAt, setExpiresAt] = useState("");
  const [note, setNote] = useState("");
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError("");
    setSaving(true);
    try {
      const result = await createInvite({
        maxUses: Number(maxUses),
        expiresAt: expiresAt ? new Date(expiresAt).toISOString() : undefined,
        note: note.trim() || undefined,
      });
      setNote("");
      setExpiresAt("");
      setMaxUses("1");
      onCreated(result);
    } catch (caught) {
      setError(apiMessage(caught));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Panel>
      <form className="grid gap-3 md:grid-cols-[minmax(0,1fr)_120px_220px_auto] md:items-end" onSubmit={(event) => void submit(event)}>
        <Field label="Note" value={note} onChange={setNote} />
        <Field label="Max uses" type="number" min={1} max={1000} inputMode="numeric" value={maxUses} onChange={setMaxUses} />
        <label className="block">
          <span className="mb-1 block text-sm font-semibold">Expires (optional)</span>
          <input
            className="iw-input"
            onChange={(event) => setExpiresAt(event.target.value)}
            type="datetime-local"
            value={expiresAt}
          />
        </label>
        <button className="iw-button-primary" disabled={saving} type="submit">
          {saving ? "Creating..." : "Create invite"}
        </button>
      </form>
      {error ? (
        <div className="mt-3">
          <InlineAlert title="Invite not created">{error}</InlineAlert>
        </div>
      ) : null}
    </Panel>
  );
}

function CodePanel({
  code,
  heading,
  hint,
  onDismiss,
}: {
  code: string;
  heading: string;
  hint?: string;
  onDismiss: () => void;
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <Panel>
      <div role="status">
        <p className="text-sm font-semibold">{heading}</p>
        {hint ? <p className="iw-muted mt-1">{hint}</p> : null}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Private as="code" hard className="rounded border border-border bg-panel-strong px-3 py-2 font-mono text-base tracking-wider">
            {code}
          </Private>
          <button className="iw-button-secondary" onClick={() => void copy()} type="button">
            {copied ? "Copied" : "Copy"}
          </button>
          <button className="iw-button-secondary" onClick={onDismiss} type="button">
            Done
          </button>
        </div>
      </div>
    </Panel>
  );
}
