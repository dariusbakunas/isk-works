import { AlertTriangle, ClipboardPaste, X } from "lucide-react";
import { useEffect, useId, useState } from "react";

import { importFitText, previewFitText, type FitTextPreview, type SlotKind } from "../../api/fittings";
import { EveTypeImage } from "../../components/eve-type-image";
import { InlineAlert } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import { rackLabel } from "./fittings-format";

/** The racks a preview lists, in the client's order. */
const PREVIEW_RACKS: SlotKind[] = ["high", "med", "low", "rig", "subsystem", "service", "drone_bay", "fighter_bay", "cargo"];

const PREVIEW_DELAY_MS = 300;

/** How many of each rack a preview holds; stacks count their quantity. */
function rackCounts(fit: NonNullable<FitTextPreview["fit"]>) {
  const counts = new Map<SlotKind, { count: number; names: string[] }>();
  for (const item of fit.items) {
    const entry = counts.get(item.slotKind) ?? { count: 0, names: [] };
    entry.count += item.slotKind === "drone_bay" || item.slotKind === "cargo" || item.slotKind === "fighter_bay" ? item.quantity : 1;
    const name = item.name ?? `Type ${item.typeId}`;
    if (!entry.names.includes(name)) entry.names.push(name);
    counts.set(item.slotKind, entry);
  }
  return PREVIEW_RACKS.filter((kind) => counts.has(kind)).map((kind) => ({ kind, ...counts.get(kind)! }));
}

export function ImportDialog({ onClose, onImported }: { onClose: () => void; onImported: (id: string) => void }) {
  const titleId = useId();
  const textId = useId();
  const nameId = useId();
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<FitTextPreview | null>(null);
  const [name, setName] = useState("");
  // The user typed a name: don't overwrite it with the text's.
  const [nameEdited, setNameEdited] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  useEffect(() => {
    if (!text.trim()) {
      setPreview(null);
      return;
    }
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      previewFitText(text, controller.signal)
        .then((result) => {
          setPreview(result);
          if (result.fit && !nameEdited) setName(result.fit.name);
        })
        .catch((reason: unknown) => {
          if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : "Preview failed.");
        });
    }, PREVIEW_DELAY_MS);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
    // Only the text drives a new preview.
  }, [text]);

  const unknown = preview?.unknownLines ?? [];
  const canSave = Boolean(preview?.fit) && unknown.length === 0 && !preview?.error && name.trim() !== "" && !saving;

  async function save() {
    setSaving(true);
    setError(null);
    try {
      const fit = await importFitText({ text, name: name.trim() });
      onImported(fit.id);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Import failed.");
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section aria-labelledby={titleId} aria-modal="true" className="iw-dialog iw-dialog-frame w-full max-w-4xl p-4" role="dialog">
        <div className="flex items-center gap-2">
          <ClipboardPaste aria-hidden="true" className="h-4 w-4 text-primary" />
          <h2 className="text-base font-semibold" id={titleId}>Import EFT fitting</h2>
          {unknown.length ? (
            <span className="rounded-[2px] border border-danger/60 px-1.5 py-px text-[0.625rem] text-danger">
              {unknown.length} unknown {unknown.length === 1 ? "line" : "lines"}
            </span>
          ) : preview?.fit ? (
            <span className="rounded-[2px] border border-positive/60 px-1.5 py-px text-[0.625rem] text-positive">Ready to import</span>
          ) : null}
          <button aria-label="Close" className="ml-auto text-muted hover:text-foreground" onClick={onClose} type="button">
            <X aria-hidden="true" className="h-4 w-4" />
          </button>
        </div>

        <div className="iw-dialog-body mt-3 grid gap-3 md:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
          <div className="space-y-2">
            <label className="iw-eyebrow block" htmlFor={textId}>Paste from EVE or pyfa</label>
            <textarea
              {...PRIVATE_ATTR}
              className="iw-input h-72 w-full resize-y font-mono text-xs"
              id={textId}
              onChange={(event) => setText(event.target.value)}
              placeholder={"[Bustard, Null Transport]\nWarp Core Stabilizer II\n…"}
              spellCheck={false}
              value={text}
            />
            {unknown.length ? (
              <ul className="space-y-1 rounded-md border border-danger/60 bg-danger/10 px-3 py-2 text-xs text-danger">
                {unknown.map((line) => (
                  <li className="flex items-start gap-1.5" key={line.line}>
                    <AlertTriangle aria-hidden="true" className="mt-0.5 h-3 w-3 shrink-0" />
                    <span>Line {line.line}: “{line.text}” — no matching type</span>
                  </li>
                ))}
              </ul>
            ) : null}
            {preview?.error ? <InlineAlert title="Not a fit">{preview.error}</InlineAlert> : null}
            {error ? <InlineAlert title="Couldn't import">{error}</InlineAlert> : null}
          </div>

          <div className="space-y-3">
            {preview?.fit ? (
              <>
                <div className="flex items-center gap-2 border-b border-border pb-2">
                  <EveTypeImage size={48} typeId={preview.fit.ship.typeId} typeName={preview.fit.ship.name ?? "Ship"} />
                  <div>
                    <p className="text-[0.625rem] text-muted">Detected hull</p>
                    <p className="font-semibold">{preview.fit.ship.name ?? `Type ${preview.fit.ship.typeId}`}</p>
                    <p className="text-[0.6875rem] text-positive">
                      {preview.fit.items.length} {preview.fit.items.length === 1 ? "item" : "items"} recognized
                    </p>
                  </div>
                </div>
                <div>
                  <p className="iw-eyebrow mb-1">Live preview</p>
                  {rackCounts(preview.fit).map((rack) => (
                    <div className="grid grid-cols-[72px_44px_1fr] items-center border-b border-border py-1 text-[0.6875rem]" key={rack.kind}>
                      <span className="font-semibold text-muted">{rackLabel(rack.kind)}</span>
                      <span className="font-mono text-primary" data-testid={`preview-rack-${rack.kind}`}>
                        {rack.count.toLocaleString("en-US")}
                      </span>
                      <span className="truncate text-muted">{rack.names.join(" · ")}</span>
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <p className="text-xs text-muted">The preview appears once the text is a fit.</p>
            )}
            <label className="block space-y-1">
              <span className="iw-eyebrow block" id={nameId}>Fit name</span>
              <input
                {...PRIVATE_ATTR}
                aria-labelledby={nameId}
                className="iw-input w-full"
                onChange={(event) => {
                  setName(event.target.value);
                  setNameEdited(true);
                }}
                value={name}
              />
            </label>
          </div>
        </div>

        <div className="iw-dialog-footer">
          <button className="iw-button-secondary" onClick={onClose} type="button">Cancel</button>
          <button className="iw-button-primary" disabled={!canSave} onClick={() => void save()} type="button">
            {saving ? "Saving…" : "Save fit"}
          </button>
        </div>
      </section>
    </div>
  );
}
