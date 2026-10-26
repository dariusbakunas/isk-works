import { useEffect, useState } from "react";

import { combineTickets, type Ticket } from "../../api/industry";
import { InlineAlert } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { CharacterName } from "../../observability/private";
import { apiMessage } from "../industry/shared/api-error";
import { sumDecimalStrings } from "../industry/shared/decimal";

// The Epic a step ticket serves (a selectable step has exactly one).
function epicOf(ticket: Ticket): string | null {
  return ticket.operations?.[0]?.orderId ?? ticket.orderId;
}

// Confirms combining the selected production steps into one ticket.
// Mirrors CreateOrderAcquisitionRunDialog's layout.
export function CombineTicketsDialog({
  open,
  tickets,
  epicNameById,
  characterNameById,
  onCancel,
  onCombined,
}: {
  open: boolean;
  tickets: Ticket[];
  epicNameById: Map<string, string>;
  characterNameById: Map<string, string>;
  onCancel: () => void;
  onCombined: (combined: Ticket) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) setError("");
  }, [open]);

  if (!open || tickets.length === 0) return null;

  const first = tickets[0];
  const quantity = tickets.reduce((total, ticket) => total + (ticket.quantity ?? 0), 0);
  const runs = tickets.reduce((total, ticket) => total + (ticket.executionSnapshot?.runs ?? 0), 0);
  const epicCount = new Set(tickets.map(epicOf)).size;
  const totalCost = sumDecimalStrings(tickets.map((ticket) => ticket.estimatedLineTotal)).value;
  const assigneeName = first.assigneeCharacterId
    ? (characterNameById.get(first.assigneeCharacterId) ?? "Unknown character")
    : null;
  const mixedAssignees = new Set(tickets.map((ticket) => ticket.assigneeCharacterId)).size > 1;

  async function combine() {
    setError("");
    setBusy(true);
    try {
      onCombined(await combineTickets(tickets.map((ticket) => ticket.id)));
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="combine-tickets-title"
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-lg p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="combine-tickets-title">
          Combine {tickets.length} tickets
        </h2>
        <p className="iw-muted mt-1">
          Makes one ticket that does these steps together. The selected tickets are canceled; Split undoes
          this until anything is recorded or linked on the combined ticket.
        </p>

        <div className="iw-dialog-body">
          {error ? (
            <div className="mt-3">
              <InlineAlert title="Could not combine">{error}</InlineAlert>
            </div>
          ) : null}

          <div className="mt-3 grid grid-cols-3 gap-3 text-xs">
            <div>
              <p className="iw-muted">Item</p>
              <p className="mt-0.5 font-medium text-foreground">{first.capturedName}</p>
            </div>
            <div>
              <p className="iw-muted">Combined</p>
              <p className="mt-0.5 font-mono font-medium tabular-nums text-foreground">
                ×{quantity.toLocaleString()}
              </p>
              <p className="iw-muted mt-0.5">{runs.toLocaleString()} runs</p>
            </div>
            <div>
              <p className="iw-muted">Serves</p>
              <p className="mt-0.5 font-medium text-foreground">
                {epicCount} Epic{epicCount === 1 ? "" : "s"}
              </p>
            </div>
          </div>

          <div className="mt-3 text-xs">
            <p className="iw-muted">Assignee</p>
            <p className="mt-0.5 font-medium text-foreground">
              {assigneeName ? <CharacterName name={assigneeName} /> : "Unassigned"}
            </p>
            {mixedAssignees ? (
              <p className="iw-muted mt-0.5">Assignees differ: taken from {first.displayId}, the first selected.</p>
            ) : null}
          </div>

          <ul className="mt-3 max-h-56 overflow-y-auto rounded-md border border-border">
            {tickets.map((ticket) => {
              const epic = epicOf(ticket);
              return (
                <li
                  className="flex items-center justify-between gap-2 border-b border-border px-2 py-1.5 text-xs last:border-b-0"
                  key={ticket.id}
                >
                  <span className="font-mono text-muted">{ticket.displayId}</span>
                  <span className="min-w-0 flex-1 truncate px-2 text-foreground">
                    {epic ? (epicNameById.get(epic) ?? "Unknown Epic") : "—"}
                  </span>
                  <span className="shrink-0 font-mono tabular-nums text-muted">
                    ×{(ticket.quantity ?? 0).toLocaleString()}
                  </span>
                  <span className="w-14 shrink-0 text-right font-mono tabular-nums text-muted">
                    {ticket.executionSnapshot?.runs ?? "—"} runs
                  </span>
                </li>
              );
            })}
          </ul>
        </div>

        <div className="iw-dialog-footer items-center justify-between">
          <div className="text-xs">
            {totalCost ? (
              <>
                <p className="iw-muted">Estimated total</p>
                <p className="font-mono font-medium text-warning">
                  <MoneyAmount value={totalCost} />
                </p>
              </>
            ) : null}
          </div>
          <div className="flex gap-2">
            <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">
              Cancel
            </button>
            <button className="iw-button-primary" disabled={busy} onClick={() => void combine()} type="button">
              {busy ? "Combining…" : "Combine"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
