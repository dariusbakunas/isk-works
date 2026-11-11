import { Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { getEsiHoldings, getInventoryItem, setEsiHoldingIncluded, type EsiHoldings, type InventoryItem } from "../../../api/inventory";
import { EveCharacterPortrait } from "../../../components/eve-character-portrait";
import { HoldingsTable } from "../../../components/holdings-table";
import { InlineAlert } from "../../../components/primitives";
import { InventoryReviewDiscrepancyPanel } from "./inventory-review-discrepancy-panel";
import { apiMessage } from "./shared";

type HoldingsState = { status: "loading" } | { status: "error"; message: string } | { status: "ready"; holdings: EsiHoldings };

export function InventoryEsiHoldingsModal({ item: initialItem, onClose, onChanged }: {
  item: InventoryItem;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [state, setState] = useState<HoldingsState>({ status: "loading" });
  const [item, setItem] = useState(initialItem);
  const [view, setView] = useState<"holdings" | "review">("holdings");
  const [updatingKey, setUpdatingKey] = useState("");
  const [mutationError, setMutationError] = useState("");
  const dialogRef = useRef<HTMLDialogElement>(null);
  const typeId = initialItem.balance.key.typeId;

  useEffect(() => {
    const element = dialogRef.current;
    if (!element || element.open) return;
    if (typeof element.showModal === "function") element.showModal();
    else element.setAttribute("open", "");
  }, []);

  useEffect(() => {
    setState({ status: "loading" });
    getEsiHoldings(typeId).then((holdings) => setState({ status: "ready", holdings }))
      .catch((error) => setState({ status: "error", message: apiMessage(error) }));
  }, [typeId]);

  async function refreshItem() {
    const fresh = await getInventoryItem(typeId);
    setItem(fresh);
    onChanged();
  }

  async function toggleIncluded(contributor: EsiHoldings["contributors"][number]) {
    const key = `${contributor.eveCharacterId}:${contributor.locationId}`;
    setUpdatingKey(key);
    setMutationError("");
    try {
      const holdings = await setEsiHoldingIncluded(typeId, contributor.eveCharacterId, contributor.locationId, contributor.ignoredForReconciliation);
      setState({ status: "ready", holdings });
      await refreshItem();
    } catch (error) {
      setMutationError(apiMessage(error));
    } finally {
      setUpdatingKey("");
    }
  }

  async function refreshAfterAdjustment() {
    setMutationError("");
    try {
      const [holdings, fresh] = await Promise.all([getEsiHoldings(typeId), getInventoryItem(typeId)]);
      setState({ status: "ready", holdings });
      setItem(fresh);
      setView("holdings");
      onChanged();
    } catch (error) {
      setMutationError(`Adjustment posted, but refreshed reconciliation data could not be loaded: ${apiMessage(error)}`);
      setView("holdings");
    }
  }

  const difference = item.reconciliationDifference;
  const inJobs = item.inJobsQuantity ?? 0;
  return (
    <dialog aria-labelledby="esi-holdings-title" className="iw-dialog iw-dialog-frame m-auto w-[min(720px,calc(100vw-2rem))] p-5 text-foreground backdrop:bg-black/70" onCancel={(event) => { event.preventDefault(); onClose(); }} ref={dialogRef}>
      <div className="mb-4 flex shrink-0 items-start justify-between gap-3">
        <div><p className="iw-eyebrow">ESI holdings</p><h2 className="text-lg font-semibold" id="esi-holdings-title">{item.balance.typeName}</h2></div>
        <button className="iw-button-secondary" onClick={onClose} type="button">Close</button>
      </div>
      {view === "holdings" ? <>
        <div className="iw-dialog-body">
          <dl className={`mb-4 grid grid-cols-2 gap-3 rounded-md border border-border bg-background p-3 text-sm ${inJobs > 0 ? "sm:grid-cols-6" : "sm:grid-cols-5"}`}>
            <Metric label="Accounting inventory" value={item.balance.quantity.toLocaleString()} />
            <Metric label="ESI observed" value={state.status === "ready" ? state.holdings.observedQuantity.toLocaleString() : "—"} />
            <Metric label="Ignored" value={state.status === "ready" ? (state.holdings.ignoredQuantity ?? 0).toLocaleString() : "—"} />
            <Metric label="Included" value={state.status === "ready" ? (state.holdings.includedQuantity ?? state.holdings.observedQuantity).toLocaleString() : "—"} />
            {inJobs > 0 ? <Metric label="In jobs" value={`+${inJobs.toLocaleString()}`} /> : null}
            <Metric label="Difference" value={difference == null ? "—" : difference === 0 ? "Match" : `${difference > 0 ? "+" : ""}${difference.toLocaleString()}`} />
          </dl>
          {state.status === "loading" ? <p className="flex items-center justify-center gap-2 py-8 text-sm text-muted" role="status"><Loader2 aria-hidden="true" className="h-4 w-4 animate-spin" />Loading ESI holdings...</p> : null}
          {state.status === "error" ? <InlineAlert title="ESI holdings unavailable">{state.message}</InlineAlert> : null}
          {mutationError ? <div className="mb-3"><InlineAlert title="Reconciliation data not refreshed">{mutationError}</InlineAlert></div> : null}
          {state.status === "ready" && state.holdings.contributors.length === 0 ? <p className="py-8 text-center text-sm text-muted">No contributing holdings -- ESI has not observed this item for any connected character.</p> : null}
          {state.status === "ready" && state.holdings.contributors.length > 0 ? <HoldingsTable
            action={{ label: "Reconcile", width: "10rem" }}
            ariaLabel={`ESI holdings of ${item.balance.typeName}`}
            rows={state.holdings.contributors.map((contributor) => {
              const key = `${contributor.eveCharacterId}:${contributor.locationId}`;
              return {
                key: `${contributor.connectionId}:${contributor.locationId}`,
                characterName: contributor.characterName,
                portrait: <EveCharacterPortrait characterId={contributor.eveCharacterId} characterName={contributor.characterName} size={32} />,
                location: contributor.locationName ?? `Unknown location ${contributor.locationId}`,
                locationTitle: `Location ${contributor.locationId} · ${contributor.locationFlag}`,
                quantity: contributor.quantity,
                dimmed: contributor.ignoredForReconciliation,
                action: <span className="flex flex-col items-end py-0.5"><button aria-label={`${contributor.ignoredForReconciliation ? "Include" : "Ignore"} ${item.balance.typeName} at this location ${contributor.ignoredForReconciliation ? "in" : "for"} reconciliation`} className="iw-button-secondary" disabled={updatingKey === key} onClick={() => void toggleIncluded(contributor)} type="button">{updatingKey === key ? "Saving..." : contributor.ignoredForReconciliation ? "Include" : "Ignore"}</button>{contributor.ignoredForReconciliation ? <span className="mt-1 text-xs text-muted">Ignored for reconciliation</span> : null}</span>,
              };
            })}
            total={state.holdings.observedQuantity}
          /> : null}
        </div>
        {difference != null && difference !== 0 ? <div className="iw-dialog-footer"><button className="iw-button-primary w-full" onClick={() => setView("review")} type="button">Review adjustment</button></div> : null}
      </> : <div className="iw-dialog-body"><InventoryReviewDiscrepancyPanel cancelLabel="Back" item={item} onCancel={() => setView("holdings")} onSaved={() => { void refreshAfterAdjustment(); }} /></div>}
    </dialog>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div><dt className="text-xs text-muted">{label}</dt><dd className="font-mono font-semibold">{value}</dd></div>;
}
