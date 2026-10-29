import { useEffect, useState } from "react";

import { combineTickets } from "../../api/industry";
import { MoneyAmount } from "../../components/money";
import { apiMessage } from "../industry/shared/api-error";
import type { CombineSuggestion } from "./combine-suggestions";

type RowResult = { status: "combined" } | { status: "failed"; message: string };

// Combines several suggestion groups at once: one Combine request per
// checked group, one after another, so a group the server refuses (e.g. a
// job got linked meanwhile) doesn't stop the rest. Mirrors
// CombineTicketsDialog's layout. `onClose(changed)` -- `changed` when any
// group was combined, so the Board reloads.
export function CombineSuggestionsDialog({
  open,
  suggestions,
  onClose,
}: {
  open: boolean;
  suggestions: CombineSuggestion[];
  onClose: (changed: boolean) => void;
}) {
  const [unchecked, setUnchecked] = useState<Set<string>>(new Set());
  const [results, setResults] = useState<Map<string, RowResult>>(new Map());
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open) {
      setUnchecked(new Set());
      setResults(new Map());
    }
  }, [open]);

  if (!open || suggestions.length === 0) return null;

  const finished = results.size > 0;
  const checked = suggestions.filter((group) => !unchecked.has(group.key));
  const changed = [...results.values()].some((result) => result.status === "combined");

  function toggle(key: string) {
    setUnchecked((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  async function combineChecked() {
    setBusy(true);
    for (const group of checked) {
      let result: RowResult;
      try {
        await combineTickets(group.tickets.map((ticket) => ticket.id));
        result = { status: "combined" };
      } catch (requestError) {
        result = { status: "failed", message: apiMessage(requestError) };
      }
      setResults((current) => new Map(current).set(group.key, result));
    }
    setBusy(false);
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="combine-suggestions-title"
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-2xl p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="combine-suggestions-title">
          Combine suggestions
        </h2>
        <p className="iw-muted mt-1">
          Unstarted steps of the same item, priced at the same facility. Each checked group becomes one ticket; Split
          undoes it until anything is recorded or linked.
        </p>

        <div className="iw-dialog-body">
          <ul className="mt-3 max-h-[60vh] overflow-y-auto rounded-md border border-border">
            {suggestions.map((group) => {
              const result = results.get(group.key);
              const label = `${group.capturedName} (${group.tickets.length} tickets)`;
              return (
                <li className="border-b border-border px-2 py-2 text-xs last:border-b-0" key={group.key}>
                  <label className="flex items-center gap-2">
                    <input
                      aria-label={label}
                      checked={!unchecked.has(group.key)}
                      disabled={busy || finished}
                      onChange={() => toggle(group.key)}
                      type="checkbox"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="font-medium text-foreground">{group.capturedName}</span>
                      <span className="iw-muted">
                        {" "}
                        · {group.tickets.length} tickets · {group.epicCount} Epic{group.epicCount === 1 ? "" : "s"}
                        {group.facility ? ` · ${group.facility.name}` : ""}
                      </span>
                    </span>
                    <span className="shrink-0 font-mono tabular-nums text-muted">
                      ×{group.quantity.toLocaleString()}
                    </span>
                    <span className="w-20 shrink-0 whitespace-nowrap text-right font-mono tabular-nums text-muted">
                      {group.runs.toLocaleString()} runs
                    </span>
                    <span className="w-24 shrink-0 text-right font-mono text-warning">
                      {group.estimatedLineTotal ? <MoneyAmount value={group.estimatedLineTotal} /> : "—"}
                    </span>
                  </label>
                  <p className="iw-muted mt-0.5 pl-6 font-mono">
                    {group.tickets.map((ticket) => ticket.displayId).join(", ")}
                  </p>
                  {group.mixedAssignees ? (
                    <p className="iw-muted mt-0.5 pl-6">Assignees differ: taken from {group.tickets[0].displayId}.</p>
                  ) : null}
                  {result?.status === "combined" ? <p className="mt-0.5 pl-6 text-positive">Combined</p> : null}
                  {result?.status === "failed" ? <p className="mt-0.5 pl-6 text-danger">{result.message}</p> : null}
                </li>
              );
            })}
          </ul>
        </div>

        <div className="iw-dialog-footer items-center justify-end gap-2">
          {finished ? (
            <button className="iw-button-primary" disabled={busy} onClick={() => onClose(changed)} type="button">
              Done
            </button>
          ) : (
            <>
              <button className="iw-button-secondary" disabled={busy} onClick={() => onClose(false)} type="button">
                Cancel
              </button>
              <button
                className="iw-button-primary"
                disabled={busy || checked.length === 0}
                onClick={() => void combineChecked()}
                type="button"
              >
                {busy ? "Combining…" : `Combine selected (${checked.length})`}
              </button>
            </>
          )}
        </div>
      </section>
    </div>
  );
}
