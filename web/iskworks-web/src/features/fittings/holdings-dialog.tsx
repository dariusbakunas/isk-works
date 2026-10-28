import { Search, X } from "lucide-react";
import { useEffect, useId } from "react";

import type { FitHolding } from "../../api/fittings";
import { CharacterName } from "../../observability/private";

/** 1,729 as 1.7K and 12,341 as 12K: a count has to fit beside the magnifier. */
const compactCount = new Intl.NumberFormat("en-US", { notation: "compact" });

/** How many of a type the workspace's characters hold, with the magnifier
 * that lists where (as on Inventory rows); a dash when they hold none, and
 * nothing while that isn't known. */
export function HeldCell({ name, held, onOpen }: { name: string; held: number | undefined; onOpen: () => void }) {
  if (held === undefined) return null;
  if (held === 0) {
    return (
      <span className="text-muted" title={`None of your characters hold ${name}`}>
        —
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1">
      <span className="font-mono text-xs" title={held.toLocaleString("en-US")}>
        {compactCount.format(held)}
      </span>
      <HoldingsButton name={name} onClick={onOpen} />
    </span>
  );
}

/** The magnifier that lists where a type is held. */
export function HoldingsButton({ name, onClick }: { name: string; onClick: () => void }) {
  return (
    <button
      aria-label={`Where you have ${name}`}
      className="grid h-5 w-5 shrink-0 place-items-center rounded text-muted transition hover:bg-panel-strong hover:text-foreground focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary"
      onClick={(event) => {
        // The row behind it selects or starts editing on a click.
        event.stopPropagation();
        onClick();
      }}
      title="Where you have it"
      type="button"
    >
      <Search aria-hidden="true" className="h-3.5 w-3.5" />
    </button>
  );
}

/** Where the workspace's characters hold one type, fitted or packaged. */
export function HoldingsDialog({ name, holdings, onClose }: { name: string; holdings: FitHolding[]; onClose: () => void }) {
  const titleId = useId();
  const total = holdings.reduce((sum, holding) => sum + holding.quantity, 0);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" onClick={onClose} role="presentation">
      <section
        aria-labelledby={titleId}
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-2xl p-4"
        onClick={(event) => event.stopPropagation()}
        role="dialog"
      >
        <div className="flex items-start gap-2">
          <div className="min-w-0">
            <p className="iw-eyebrow">Held by your characters</p>
            <h2 className="truncate text-base font-semibold" id={titleId}>{name}</h2>
          </div>
          <button aria-label="Close" className="ml-auto text-muted hover:text-foreground" onClick={onClose} type="button">
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>
        <div className="iw-dialog-body mt-3">
          {holdings.length === 0 ? (
            <p className="text-xs text-muted">None of your characters hold any.</p>
          ) : (
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b border-border text-left uppercase text-muted">
                  <th className="py-1.5 pr-2 font-semibold">Character</th>
                  <th className="py-1.5 pr-2 font-semibold">Location</th>
                  <th className="py-1.5 pl-2 text-right font-semibold">Quantity</th>
                </tr>
              </thead>
              <tbody>
                {holdings.map((holding) => (
                  <tr className="border-b border-border" key={`${holding.connectionId}:${holding.locationId}`}>
                    <td className="py-1.5 pr-2">
                      <CharacterName className="truncate" name={holding.characterName} />
                    </td>
                    <td className="py-1.5 pr-2 text-muted">{holding.locationName}</td>
                    <td className="whitespace-nowrap py-1.5 pl-2 text-right font-mono">{holding.quantity.toLocaleString("en-US")}</td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr className="font-semibold">
                  <td className="py-1.5 pr-2" colSpan={2}>Total</td>
                  <td className="whitespace-nowrap py-1.5 pl-2 text-right font-mono">{total.toLocaleString("en-US")}</td>
                </tr>
              </tfoot>
            </table>
          )}
        </div>
      </section>
    </div>
  );
}
