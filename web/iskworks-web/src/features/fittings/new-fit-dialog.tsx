import { Search, X } from "lucide-react";
import { useEffect, useId, useState } from "react";

import { createFit, searchFitCatalogue, type CatalogueItem } from "../../api/fittings";
import { EveTypeImage } from "../../components/eve-type-image";
import { InlineAlert } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";

const SEARCH_DELAY_MS = 250;

/** Starts an empty fit on a ship; the caller opens it in the editor. */
export function NewFitDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const titleId = useId();
  const nameId = useId();
  const [query, setQuery] = useState("");
  const [ships, setShips] = useState<CatalogueItem[]>([]);
  const [ship, setShip] = useState<CatalogueItem | null>(null);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      searchFitCatalogue({ kind: "ships" }, { q: query }, controller.signal)
        .then((view) => setShips(view.entries))
        .catch((reason: unknown) => {
          if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : "Search failed.");
        });
    }, SEARCH_DELAY_MS);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [query]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  async function create() {
    if (!ship) return;
    setSaving(true);
    setError(null);
    try {
      const fit = await createFit({ name: name.trim(), description: "", shipTypeId: ship.typeId, items: [] });
      onCreated(fit.id);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Couldn't create the fit.");
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog iw-dialog-frame w-full max-w-md p-4" role="dialog">
        <div className="flex items-center gap-2">
          <h2 className="text-base font-semibold" id={titleId}>New fit</h2>
          <button aria-label="Close" className="ml-auto text-muted hover:text-foreground" onClick={onClose} type="button">
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>
        <div className="iw-dialog-body mt-3 space-y-2">
          <label className="flex items-center gap-2 rounded-md border border-border bg-background px-2">
            <Search aria-hidden="true" className="h-3.5 w-3.5 text-muted" />
            <input
              aria-label="Search ships"
              className="h-8 w-full bg-transparent text-xs outline-none"
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Ship name"
              type="search"
              value={query}
            />
          </label>
          <ul className="max-h-64 overflow-y-auto">
            {ships.map((entry) => (
              <li key={entry.typeId}>
                <button
                  aria-label={entry.name}
                  aria-pressed={ship?.typeId === entry.typeId}
                  className={`flex w-full items-center gap-2 rounded px-2 py-1 text-left text-xs hover:bg-panel-strong ${
                    ship?.typeId === entry.typeId ? "bg-primary/15" : ""
                  }`}
                  onClick={() => {
                    setShip(entry);
                    setName((current) => (current && current !== `${ship?.name} fit` ? current : `${entry.name} fit`));
                  }}
                  type="button"
                >
                  <EveTypeImage size={24} typeId={entry.typeId} typeName={entry.name} />
                  <span className="flex-1">{entry.name}</span>
                  <span className="text-[0.625rem] text-muted">{entry.marketPath.at(-1)}</span>
                </button>
              </li>
            ))}
          </ul>
          <label className="block space-y-1">
            <span className="iw-eyebrow block" id={nameId}>Fit name</span>
            <input
              {...PRIVATE_ATTR}
              aria-labelledby={nameId}
              className="iw-input w-full"
              onChange={(event) => setName(event.target.value)}
              value={name}
            />
          </label>
          {error ? <InlineAlert title="Couldn't create the fit">{error}</InlineAlert> : null}
        </div>
        <div className="iw-dialog-footer">
          <button className="iw-button-secondary" onClick={onClose} type="button">Cancel</button>
          <button
            className="iw-button-primary"
            disabled={!ship || !name.trim() || saving}
            onClick={() => void create()}
            type="button"
          >
            Create fit
          </button>
        </div>
      </section>
    </div>
  );
}
