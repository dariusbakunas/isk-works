import { CheckCircle2, Upload, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  importMarketExports,
  previewMarketExports,
  resolveMarketLocations,
  type MarketImportPreview,
  type MarketImportResult,
  type MarketLocationResolution,
} from "../../../api/industry";
import { ButtonLink, InlineAlert } from "../../../components/primitives";
import { apiMessage, formatAge } from "./shared";

type Phase =
  | { status: "idle" }
  | { status: "detecting" }
  | { status: "confirm"; preview: MarketImportPreview }
  | { status: "importing"; preview: MarketImportPreview }
  | { status: "error"; message: string }
  | { status: "done"; result: MarketImportResult };

interface DetectedScope {
  key: string;
  regionName: string | null;
  locationName: string | null;
  itemTypeCount: number;
  orderCount: number;
  latestObservedAt: string | null;
}

// The import modal's design only ever shows one scope card, but the parser
// itself already supports one file dropping multiple distinct (region,
// location) identities, so group by scope and render one card per group
// rather than assuming a single one.
function groupByScope(preview: MarketImportPreview): DetectedScope[] {
  const groups = new Map<
    string,
    { regionName: string | null; locationName: string | null; itemTypeIds: Set<number>; orderCount: number; latestObservedAt: string | null }
  >();
  for (const file of preview.files) {
    if (!file.canImport) continue;
    const key = `${file.regionId ?? "unknown"}:${file.locationId ?? "unknown"}`;
    const orderCount = file.buyOrderCount + file.sellOrderCount;
    const existing = groups.get(key);
    if (existing) {
      if (file.typeId !== null) existing.itemTypeIds.add(file.typeId);
      existing.orderCount += orderCount;
      if (file.observedAt && (!existing.latestObservedAt || file.observedAt > existing.latestObservedAt)) {
        existing.latestObservedAt = file.observedAt;
      }
    } else {
      groups.set(key, {
        regionName: file.regionName,
        locationName: file.locationName,
        itemTypeIds: new Set(file.typeId !== null ? [file.typeId] : []),
        orderCount,
        latestObservedAt: file.observedAt,
      });
    }
  }
  return Array.from(groups.entries()).map(([key, group]) => ({
    key,
    regionName: group.regionName,
    locationName: group.locationName,
    itemTypeCount: group.itemTypeIds.size,
    orderCount: group.orderCount,
    latestObservedAt: group.latestObservedAt,
  }));
}

/**
 * "Import market export": drop/pick EVE client export files, auto-detect
 * region/location, confirm, import -- no Price Source creation step. Runs
 * the full drop -> detecting -> confirm flow.
 */
export function MarketImportDialog({
  open,
  onClose,
  onImported,
}: {
  open: boolean;
  onClose: () => void;
  onImported: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [phase, setPhase] = useState<Phase>({ status: "idle" });
  const [files, setFiles] = useState<File[]>([]);
  const [resolution, setResolution] = useState<MarketLocationResolution | null>(null);

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (open && !element.open) {
      if (typeof element.showModal === "function") element.showModal();
      else element.setAttribute("open", "");
    }
    if (!open && element.open) {
      if (typeof element.close === "function") element.close();
      else element.removeAttribute("open");
    }
  }, [open]);

  useEffect(() => {
    if (open) return;
    setPhase({ status: "idle" });
    setFiles([]);
    setResolution(null);
    if (inputRef.current) inputRef.current.value = "";
  }, [open]);

  async function runPreview(selected: File[]) {
    setFiles(selected);
    setPhase({ status: "detecting" });
    try {
      const preview = await previewMarketExports(selected);
      setPhase({ status: "confirm", preview });
      void runLocationResolution(preview);
    } catch (error) {
      setPhase({ status: "error", message: apiMessage(error) });
    }
  }

  async function runLocationResolution(preview: MarketImportPreview) {
    const locationIds = Array.from(
      new Set(
        preview.files
          .filter((file) => file.locationId !== null && file.locationName === `Structure ${file.locationId}`)
          .map((file) => file.locationId as number),
      ),
    );
    if (locationIds.length === 0) return;
    try {
      const next = await resolveMarketLocations(locationIds);
      const names = new Map(next.resolved.map((location) => [location.locationId, location.locationName]));
      setPhase((current) =>
        current.status === "confirm"
          ? {
              status: "confirm",
              preview: {
                ...current.preview,
                files: current.preview.files.map((file) => ({
                  ...file,
                  locationName: file.locationId === null ? file.locationName : (names.get(file.locationId) ?? file.locationName),
                })),
              },
            }
          : current,
      );
      setResolution(next);
    } catch (error) {
      setResolution({
        configured: true,
        resolved: [],
        unresolvedLocationIds: locationIds,
        eligibleCharacterCount: 0,
        needsReconnection: false,
        warnings: [apiMessage(error)],
      });
    }
  }

  async function runImport() {
    if (phase.status !== "confirm") return;
    setPhase({ status: "importing", preview: phase.preview });
    try {
      const result = await importMarketExports(files);
      setPhase({ status: "done", result });
      onImported();
    } catch (error) {
      setPhase({ status: "error", message: apiMessage(error) });
    }
  }

  const scopes = phase.status === "confirm" || phase.status === "importing" ? groupByScope(phase.preview) : [];
  const busy = phase.status === "importing";

  return (
    <dialog
      aria-labelledby="market-import-dialog-title"
      className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(480px,calc(100vw-2rem))] overflow-y-auto p-0 text-foreground backdrop:bg-black/70"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
      ref={dialog}
    >
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <span className="text-sm font-semibold" id="market-import-dialog-title">
          Import EVE Market Export
        </span>
        <button aria-label="Close import dialog" className="iw-icon-button" disabled={busy} onClick={onClose} type="button">
          <X aria-hidden="true" className="h-4 w-4" />
        </button>
      </div>
      <div className="p-5">
        {phase.status === "idle" ? (
          <div className="grid place-items-center border-2 border-dashed border-border p-6 text-center">
            <Upload aria-hidden="true" className="mx-auto h-6 w-6 text-muted" />
            <p className="mt-2 text-sm font-semibold">Drop EVE market exports here</p>
            <p className="iw-muted mt-1 text-xs">Up to 20 .txt or .csv files, 5 MiB each and 25 MiB total.</p>
            <button className="iw-button-secondary mt-3" onClick={() => inputRef.current?.click()} type="button">
              Choose files
            </button>
            <input
              accept=".txt,.csv,text/plain,text/csv"
              className="sr-only"
              multiple
              onChange={(event) => {
                const selected = Array.from(event.target.files ?? []);
                if (selected.length > 0) void runPreview(selected);
              }}
              ref={inputRef}
              type="file"
            />
          </div>
        ) : null}

        {phase.status === "detecting" ? (
          <p className="iw-muted py-6 text-center text-sm" role="status">
            Detecting region and location...
          </p>
        ) : null}

        {phase.status === "error" ? <InlineAlert title="Market import unavailable">{phase.message}</InlineAlert> : null}

        {phase.status === "confirm" || phase.status === "importing" ? (
          <>
            {resolution?.needsReconnection ? (
              <div className="mb-3">
                <InlineAlert title="Structure permission required" tone="info">
                  Reconnect an EVE character to grant read-only structure access, then retry.
                  <span className="ml-2">
                    <ButtonLink to="/characters">Open Characters</ButtonLink>
                  </span>
                </InlineAlert>
              </div>
            ) : resolution && resolution.unresolvedLocationIds.length > 0 ? (
              <div className="mb-3">
                <InlineAlert title="Some structure names are private" tone="info">
                  Numeric location IDs remain authoritative for pricing regardless.
                </InlineAlert>
              </div>
            ) : null}

            {scopes.length === 0 ? (
              <div className="mb-3">
                <InlineAlert title="No importable data" tone="warning">
                  {phase.status === "confirm" && phase.preview.duplicateFiles > 0
                    ? "Every selected file is already stored."
                    : "Selected files could not be parsed."}
                </InlineAlert>
              </div>
            ) : (
              scopes.map((scope) => (
                <div className="mb-3" key={scope.key}>
                  <InlineAlert title="Market export detected" tone="success">
                    <dl className="mt-1 grid gap-0.5">
                      <Row label="Region" value={scope.regionName ?? "Unknown"} />
                      <Row label="Location" value={scope.locationName ?? "Unknown"} />
                      <Row label="Item types" value={scope.itemTypeCount.toLocaleString()} />
                      <Row label="Orders" value={scope.orderCount.toLocaleString()} />
                      <Row label="Export age" value={formatAge(scope.latestObservedAt)} />
                    </dl>
                  </InlineAlert>
                </div>
              ))
            )}

            <p className="iw-muted mb-3 text-xs">
              These observations are merged into the market data for the scope{scopes.length === 1 ? "" : "s"} above.
              No Price Override is created.
            </p>

            <div className="flex gap-2">
              <button
                className="iw-button-primary flex-1"
                disabled={busy || scopes.length === 0}
                onClick={() => void runImport()}
                type="button"
              >
                {busy ? "Importing..." : "Import"}
              </button>
              <button className="iw-button-secondary" disabled={busy} onClick={onClose} type="button">
                Cancel
              </button>
            </div>
          </>
        ) : null}

        {phase.status === "done" ? (
          <div>
            <div className="flex items-start gap-3">
              <CheckCircle2 aria-hidden="true" className="mt-0.5 h-5 w-5 text-positive" />
              <div>
                <h3 className="text-sm font-semibold">{phase.result.batch ? "Market observations imported" : "No new observations"}</h3>
                <p className="iw-muted mt-1 text-sm">
                  {phase.result.importedObservations.toLocaleString()} observations across {phase.result.importedFiles} order
                  book{phase.result.importedFiles === 1 ? "" : "s"}.
                  {phase.result.skippedDuplicateFiles > 0
                    ? ` ${phase.result.skippedDuplicateFiles} duplicate order book${phase.result.skippedDuplicateFiles === 1 ? "" : "s"} skipped.`
                    : ""}
                </p>
              </div>
            </div>
            <div className="mt-4 flex justify-end">
              <button className="iw-button-primary" onClick={onClose} type="button">
                Done
              </button>
            </div>
          </div>
        ) : null}
      </div>
    </dialog>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-2 text-xs">
      <dt className="text-foreground/70">{label}</dt>
      <dd className="font-mono text-foreground">{value}</dd>
    </div>
  );
}
