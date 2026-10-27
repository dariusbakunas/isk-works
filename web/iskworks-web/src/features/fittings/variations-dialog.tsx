import { X } from "lucide-react";
import { useEffect, useId, useState } from "react";

import { getFitVariations, type CatalogueItem, type FitItemView, type PilotChoice } from "../../api/fittings";
import { formatStat } from "./fittings-format";

/** A module's variations (Tech I, Tech II, meta, faction, …) to swap it for. */
export function VariationsDialog({
  item,
  pilot,
  onPick,
  onClose,
}: {
  item: FitItemView;
  pilot: PilotChoice;
  onPick: (entry: CatalogueItem) => void;
  onClose: () => void;
}) {
  const titleId = useId();
  const name = item.name ?? `Type ${item.typeId}`;
  const [entries, setEntries] = useState<CatalogueItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    getFitVariations(item.typeId, pilot, controller.signal)
      .then((view) => setEntries(view.entries))
      .catch((reason: unknown) => {
        if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : "Couldn't load variations.");
      });
    return () => controller.abort();
  }, [item.typeId, pilot]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog iw-dialog-frame w-full max-w-md p-4" role="dialog">
        <div className="flex items-center gap-2">
          <h2 className="text-base font-semibold" id={titleId}>Variations of {name}</h2>
          <button aria-label="Close" className="ml-auto text-muted hover:text-foreground" onClick={onClose} type="button">
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>
        <div className="iw-dialog-body mt-3">
          {error ? <p className="text-xs text-danger">{error}</p> : null}
          {entries === null && !error ? <p className="text-xs text-muted">Loading…</p> : null}
          {entries?.length === 0 ? <p className="text-xs text-muted">This module has no variations.</p> : null}
          <ul className="divide-y divide-border">
            {entries?.map((entry) => {
              const current = entry.typeId === item.typeId;
              return (
                <li className={`flex items-center gap-2 py-1.5 text-xs ${entry.canUse === false ? "opacity-60" : ""}`} key={entry.typeId}>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{entry.name}</span>
                    <span className="block font-mono text-[0.625rem] text-muted">
                      {formatStat(entry.cpu, 1)} tf · {formatStat(entry.powergrid, 1)} MW
                      {entry.canUse === false ? " · pilot lacks the skills" : ""}
                    </span>
                  </span>
                  {current ? (
                    <span className="text-[0.625rem] text-muted">Fitted</span>
                  ) : (
                    <button aria-label={`Use ${entry.name}`} className="iw-button-secondary" onClick={() => onPick(entry)} type="button">
                      Use
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      </section>
    </div>
  );
}
