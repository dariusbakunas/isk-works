import { useEffect, useState } from "react";

import { createAcquisitionRun, type AcquisitionRun, type Ticket } from "../../api/industry";
import { InlineAlert, TextField } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { apiMessage } from "../industry/shared/api-error";
import { sumDecimalStrings } from "../industry/shared/decimal";
import {
  acquisitionPricingIdentity,
  acquisitionPricingKey,
  describeAcquisitionPricing,
} from "./acquisition-pricing";
import { consolidateOrderTicketsByType } from "./order-ticket-consolidation";

// Name-free descriptor for the selected tickets' shared acquisition pricing
// identity, mirroring the backend `BatchKey`. Market scope takes precedence
// over a manual price list; a mix is flagged (the create call will reject
// it). Region/location names aren't resolved here -- the confirm dialog
// only needs to say *which kind* of identity, not spell out the station.
function pricingLabel(tickets: Ticket[], priceSourceNameById: Map<string, string>): string {
  const keys = new Set(tickets.map((ticket) => acquisitionPricingKey(acquisitionPricingIdentity(ticket))));
  if (keys.size === 0) return "—";
  if (keys.size > 1) return "Multiple (incompatible)";
  const identity = acquisitionPricingIdentity(tickets[0]);
  const priceSourceName =
    identity.kind === "list" ? (priceSourceNameById.get(identity.priceSourceId) ?? null) : null;
  return describeAcquisitionPricing(identity, { priceSourceName }).label;
}

// Standalone-ticket counterpart to CreateAcquisitionRunDialog.
export function CreateOrderAcquisitionRunDialog({
  open,
  tickets,
  priceSourceNameById,
  onCancel,
  onCreated,
}: {
  open: boolean;
  tickets: Ticket[];
  priceSourceNameById: Map<string, string>;
  onCancel: () => void;
  onCreated: (run: AcquisitionRun) => void;
}) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) {
      setName("");
      setError("");
    }
  }, [open]);

  if (!open) return null;

  const items = consolidateOrderTicketsByType(tickets);
  const totalCost = sumDecimalStrings(tickets.map((ticket) => ticket.estimatedLineTotal)).value;

  async function create() {
    setError("");
    setBusy(true);
    try {
      const run = await createAcquisitionRun({
        name: name.trim() || undefined,
        ticketIds: tickets.map((ticket) => ticket.id),
      });
      onCreated(run);
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="create-order-run-title"
        aria-modal="true"
        className="iw-dialog w-full max-w-lg p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="create-order-run-title">
          Create Acquisition Run
        </h2>
        <p className="iw-muted mt-1">
          Groups {tickets.length} selected ticket{tickets.length === 1 ? "" : "s"} for one shopping trip.
          Source Orders stay untouched until the Run starts.
        </p>

        {error ? (
          <div className="mt-3">
            <InlineAlert title="Could not create the Run">{error}</InlineAlert>
          </div>
        ) : null}

        <div className="mt-3">
          <TextField id="order-acquisition-run-name" label="Name (optional)" onChange={setName} value={name} />
          <p className="iw-muted mt-1 text-xs">
            Stable ID will be auto-generated: <span className="font-mono">ACQ-####</span>
          </p>
        </div>

        <div className="mt-3 grid grid-cols-2 gap-3 text-xs">
          <div>
            <p className="iw-muted">Pricing</p>
            <p className="mt-0.5 font-medium text-foreground">{pricingLabel(tickets, priceSourceNameById)}</p>
          </div>
          <div>
            <p className="iw-muted">Source tickets</p>
            <p className="mt-0.5 font-medium text-foreground">
              {tickets.length} ticket{tickets.length === 1 ? "" : "s"}
            </p>
          </div>
        </div>

        <div className="mt-3">
          <p className="iw-muted text-xs font-medium tracking-wide">Consolidated items</p>
          <div className="mt-1.5 max-h-40 overflow-y-auto rounded-md border border-border">
            {items.map((item) => (
              <div
                className="flex items-center justify-between gap-2 border-b border-border px-2 py-1.5 text-xs last:border-b-0"
                key={item.typeId}
              >
                <span className="min-w-0 flex-1 truncate text-foreground">{item.capturedName}</span>
                <span className="shrink-0 font-mono tabular-nums text-muted">
                  {item.neededQuantity.toLocaleString()}
                </span>
                <span className="w-20 shrink-0 text-right font-mono tabular-nums text-warning">
                  {item.lineCost ? <MoneyAmount mode="summary" value={item.lineCost} /> : "—"}
                </span>
              </div>
            ))}
          </div>
        </div>

        <ul className="mt-3 max-h-56 overflow-y-auto rounded-md border border-border">
          {tickets.map((ticket) => (
            <li
              className="flex items-center justify-between gap-2 border-b border-border px-2 py-1.5 text-xs last:border-b-0"
              key={ticket.id}
            >
              <span className="font-mono text-muted">{ticket.displayId}</span>
              <span className="min-w-0 flex-1 truncate px-2 text-foreground">{ticket.capturedName}</span>
            </li>
          ))}
        </ul>

        <div className="mt-4 flex items-center justify-between gap-2">
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
            <button
              className="iw-button-primary"
              disabled={busy || tickets.length === 0}
              onClick={() => void create()}
              type="button"
            >
              {busy ? "Creating…" : "Create Run"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
