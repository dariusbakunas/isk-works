import { useEffect, useState } from "react";

import {
  completeAcquisitionRun,
  getAcquisitionRun,
  recordAcquisitionProgress,
  startAcquisitionRun,
  type AcquisitionRun,
  type AcquisitionRunItem,
  type Ticket,
} from "../../api/industry";
import { MapPin, Tag } from "lucide-react";

import { Badge, InlineAlert, ProgressBar } from "../../components/primitives";
import { MoneyAmount } from "../../components/money";
import { PlannerInspectorShell } from "../../components/planner-inspector-shell";
import { apiMessage } from "../industry/shared/api-error";
import { sumDecimalStrings } from "../industry/shared/decimal";
import { acquisitionPricingIdentity } from "./acquisition-pricing";
import { consolidateOrderTicketsByType, withRealOrderAcquiredTotals } from "./order-ticket-consolidation";
import { acquisitionRunStatusMeta } from "./ticket-meta";
import { useAcquisitionPricingDescriptor } from "./use-acquisition-pricing-descriptor";

// Drawer for an Acquisition Run of standalone tickets. Deliberately no
// start-preview/confirm step -- an Order has no stored status or lock
// transition to warn about (its status is fully derived), so Start just
// starts.
export function OrderAcquisitionRunDrawer({
  run,
  tickets,
  priceSourceName,
  onClose,
  onChanged,
}: {
  run: AcquisitionRun;
  tickets: Ticket[];
  /** Resolved name of the run's manual price list, when it has one. */
  priceSourceName?: string | null;
  onClose: () => void;
  onChanged: () => void;
}) {
  const pricing = useAcquisitionPricingDescriptor(acquisitionPricingIdentity(run), priceSourceName);
  const [runItems, setRunItems] = useState<AcquisitionRunItem[]>([]);
  const items = withRealOrderAcquiredTotals(consolidateOrderTicketsByType(tickets), runItems);
  const [inputs, setInputs] = useState<Record<number, string>>({});
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;
    getAcquisitionRun(run.id)
      .then((detail) => {
        if (!cancelled) {
          setRunItems(detail.items);
        }
      })
      .catch(() => {
        // Best-effort: ticket-derived (capped) sums remain a reasonable
        // fallback display if this fetch fails.
      });
    return () => {
      cancelled = true;
    };
  }, [run.id, run.updatedAt]);

  useEffect(() => {
    const next: Record<number, string> = {};
    for (const item of items) {
      next[item.typeId] = String(item.acquiredQuantity);
    }
    setInputs(next);
    setError("");
    // Re-seed the inputs only when the run or its source data changes; `items` is derived from them each render.
  }, [run.id, tickets, runItems]);

  const status = acquisitionRunStatusMeta[run.status];
  const totalCost = sumDecimalStrings(tickets.map((ticket) => ticket.estimatedLineTotal)).value;
  const fullyAcquired = items.every((item) => item.acquiredQuantity >= item.neededQuantity);

  function toggleExpanded(typeId: number) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(typeId)) {
        next.delete(typeId);
      } else {
        next.add(typeId);
      }
      return next;
    });
  }

  async function saveProgress() {
    setError("");
    setBusy(true);
    try {
      await recordAcquisitionProgress(
        run.id,
        items.map((item) => ({
          typeId: item.typeId,
          acquiredQuantity: Number(inputs[item.typeId] ?? item.acquiredQuantity),
        })),
      );
      onChanged();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function start() {
    setError("");
    setBusy(true);
    try {
      await startAcquisitionRun(run.id);
      onChanged();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function complete() {
    setError("");
    setBusy(true);
    try {
      await completeAcquisitionRun(run.id);
      onChanged();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <PlannerInspectorShell
      closeLabel="Close acquisition run details"
      eyebrow={run.displayId}
      onClose={onClose}
      open
      title={run.name}
      width="wide"
    >
      <div className="flex-1 space-y-2 p-4">
        <div className="flex items-center gap-2">
          <Badge tone={status.tone}>{status.label}</Badge>
          <span className="iw-muted flex min-w-0 items-center gap-1 truncate text-xs">
            {pricing.icon === "location" ? (
              <MapPin aria-hidden="true" size={11} />
            ) : pricing.icon === "list" ? (
              <Tag aria-hidden="true" size={11} />
            ) : null}
            <span className="truncate">{pricing.label}</span>
          </span>
        </div>

        {error ? (
          <div className="mt-3">
            <InlineAlert title="Action did not finish">{error}</InlineAlert>
          </div>
        ) : null}

        {run.status === "inProgress" ? (
          <p className="iw-muted mt-2 text-xs">Pending delivery — not yet in Inventory.</p>
        ) : null}

        <div className="mt-4 flex-1 space-y-2">
          {items.map((item) => {
            const percent = item.neededQuantity === 0 ? 100 : (item.acquiredQuantity / item.neededQuantity) * 100;
            const remaining = Math.max(0, item.neededQuantity - item.acquiredQuantity);
            const surplus = Math.max(0, item.acquiredQuantity - item.neededQuantity);
            const reserve = Math.min(item.acquiredQuantity, item.neededQuantity);
            const isExpanded = expanded.has(item.typeId);
            return (
              <div className="iw-panel p-2 text-sm" key={item.typeId}>
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium text-foreground">{item.capturedName}</span>
                  <button
                    className="iw-muted text-xs hover:text-foreground"
                    onClick={() => toggleExpanded(item.typeId)}
                    type="button"
                  >
                    {item.tickets.length} ticket{item.tickets.length === 1 ? "" : "s"} {isExpanded ? "▲" : "▼"}
                  </button>
                </div>
                <div className="mt-1 flex items-center gap-2">
                  <ProgressBar percent={percent} tone={percent >= 100 ? "positive" : "warning"} />
                  <span className="font-mono text-xs text-muted">
                    Required {item.neededQuantity.toLocaleString()} · Acquired{" "}
                    {item.acquiredQuantity.toLocaleString()}
                  </span>
                </div>
                {remaining > 0 ? (
                  <p className="iw-muted mt-1 text-xs">Remaining to buy {remaining.toLocaleString()}</p>
                ) : null}
                {surplus > 0 ? (
                  <p className="iw-muted mt-1 text-xs">
                    On delivery: reserve {reserve.toLocaleString()} · to stock {surplus.toLocaleString()}
                  </p>
                ) : null}
                {run.status === "inProgress" ? (
                  <div className="mt-2 flex items-center gap-2">
                    <input
                      aria-label={`Acquired ${item.capturedName}`}
                      className="iw-input w-28"
                      min={0}
                      onChange={(event) =>
                        setInputs((current) => ({ ...current, [item.typeId]: event.target.value }))
                      }
                      type="number"
                      value={inputs[item.typeId] ?? ""}
                    />
                    <span className="iw-muted text-xs">of {item.neededQuantity.toLocaleString()} needed</span>
                  </div>
                ) : null}
                {isExpanded ? (
                  <ul className="mt-2 space-y-1 border-t border-border pt-2">
                    {item.tickets.map((ticket) => {
                      const ticketQuantity = ticket.quantity ?? 0;
                      const share = item.neededQuantity === 0 ? 0 : (ticketQuantity / item.neededQuantity) * 100;
                      return (
                        <li className="flex items-center justify-between gap-2 text-xs" key={ticket.id}>
                          <span className="font-mono text-muted">{ticket.displayId}</span>
                          <span className="font-mono tabular-nums text-muted">
                            ×{ticketQuantity.toLocaleString()} · {Math.round(share)}%
                          </span>
                        </li>
                      );
                    })}
                  </ul>
                ) : null}
              </div>
            );
          })}
        </div>

        {totalCost ? (
          <p className="iw-muted mt-2 border-t border-border pt-2 font-mono text-xs">
            Estimated total <MoneyAmount value={totalCost} />
          </p>
        ) : null}

        <div className="mt-4 flex justify-end gap-2 border-t border-border pt-3">
          {run.status === "ready" ? (
            <button className="iw-button-primary" disabled={busy} onClick={() => void start()} type="button">
              {busy ? "Starting…" : "Start Run"}
            </button>
          ) : null}
          {run.status === "inProgress" ? (
            <>
              <button
                className="iw-button-secondary"
                disabled={busy}
                onClick={() => void saveProgress()}
                type="button"
              >
                {busy ? "Saving…" : "Save Progress"}
              </button>
              <button
                className="iw-button-primary"
                disabled={busy}
                onClick={() => void complete()}
                title={
                  fullyAcquired
                    ? undefined
                    : "Some items are still short of what's needed — completing now delivers only what's been recorded, and remaining demand stays outstanding."
                }
                type="button"
              >
                Complete Run
              </button>
            </>
          ) : null}
        </div>
      </div>
    </PlannerInspectorShell>
  );
}
