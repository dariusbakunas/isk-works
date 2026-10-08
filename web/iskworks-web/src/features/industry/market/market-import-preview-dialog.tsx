import { CheckCircle2, RefreshCw, X } from "lucide-react";
import { useEffect, useRef } from "react";

import type { MarketImportPreview, MarketLocationResolution } from "../../../api/industry";
import { MoneyAmount } from "../../../components/money";
import { ButtonLink, InlineAlert, StatusBadge } from "../../../components/primitives";
import { formatDate, Metric } from "./shared";

function importButtonLabel(busy: boolean, preview: MarketImportPreview): string {
  if (busy) return "Importing...";
  if (preview.validFiles > 0) return "Import Market Observations";
  return preview.duplicateFiles > 0 && preview.invalidFiles === 0 ? "Already imported" : "No importable files";
}

export function MarketImportPreviewDialog({
  preview,
  busy,
  error,
  onCancel,
  onImport,
  resolution,
  resolvingLocations,
  onResolveLocations,
}: {
  preview: MarketImportPreview | null;
  busy: boolean;
  error: string;
  onCancel: () => void;
  onImport: () => void;
  resolution: MarketLocationResolution | null;
  resolvingLocations: boolean;
  onResolveLocations: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (preview && !element.open) {
      if (typeof element.showModal === "function") element.showModal();
      else element.setAttribute("open", "");
    }
    if (!preview && element.open) {
      if (typeof element.close === "function") element.close();
      else element.removeAttribute("open");
    }
  }, [preview]);

  return (
    <dialog
      aria-labelledby="market-import-preview-title"
      className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(1100px,calc(100vw-2rem))] overflow-y-auto p-0 text-foreground backdrop:bg-black/70"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onCancel();
      }}
      ref={dialog}
    >
      {preview ? (
        <div className="p-5">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="iw-eyebrow">Import confirmation</p>
              <h2 className="text-base font-semibold" id="market-import-preview-title">Preview Market Exports</h2>
              <p className="iw-muted mt-1">Review parsed orders before adding any market observations.</p>
            </div>
            <button
              aria-label="Close market import preview"
              className="iw-icon-button"
              disabled={busy}
              onClick={onCancel}
              title="Close"
              type="button"
            >
              <X className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>

          {error ? <div className="mt-4"><InlineAlert title="Market import unavailable">{error}</InlineAlert></div> : null}
          {resolution?.needsReconnection ? (
            <div className="mt-4">
              <InlineAlert title="Structure permission required" tone="info">
                Reconnect an EVE character to grant read-only structure access, then retry location names.
                <span className="ml-2"><ButtonLink to="/characters">Open Characters</ButtonLink></span>
              </InlineAlert>
            </div>
          ) : resolution && !resolution.configured ? (
            <div className="mt-4">
              <InlineAlert title="Structure names unavailable" tone="info">
                Configure EVE SSO to resolve private structure names. Numeric location IDs remain authoritative.
              </InlineAlert>
            </div>
          ) : resolution && resolution.unresolvedLocationIds.length > 0 && resolution.eligibleCharacterCount > 0 ? (
            <div className="mt-4">
              <InlineAlert title="Some structure names are private" tone="info">
                No connected character could access {resolution.unresolvedLocationIds.length} location{resolution.unresolvedLocationIds.length === 1 ? "" : "s"}. The numeric location ID remains authoritative for pricing.
              </InlineAlert>
            </div>
          ) : null}
          {resolution?.warnings.map((warning) => (
            <p className="mt-3 text-sm text-warning" key={warning}>{warning}</p>
          ))}

          <div className="mt-5 grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-7">
            <Metric label="Order books" value={preview.totalFiles} />
            <Metric label="Ready" value={preview.validFiles} />
            <Metric label="Invalid" value={preview.invalidFiles} />
            <Metric label="Duplicates" value={preview.duplicateFiles} />
            <Metric label="Items" value={preview.itemCount} />
            <Metric label="Locations" value={preview.locationCount} />
            <Metric label="Orders" value={preview.totalRows} />
          </div>

          <div className="mt-5 border-t border-border">
            {preview.files.map((file) => (
              <div
                className="grid gap-3 border-b border-border py-4 lg:grid-cols-[minmax(0,1.4fr)_repeat(4,minmax(100px,0.6fr))]"
                key={`${file.fileChecksum}-${file.typeId ?? "unknown"}-${file.locationId ?? "unknown"}`}
              >
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <strong className="break-all">{file.filename}</strong>
                    <StatusBadge>{file.canImport ? "Ready" : file.alreadyImported ? "Already imported" : "Invalid"}</StatusBadge>
                  </div>
                  <p className="iw-muted mt-1 text-sm">
                    {file.typeName ?? "Unknown item"} · {file.locationName ?? "Unknown location"}
                  </p>
                  <p className="mt-1 text-xs text-muted">Observed {formatDate(file.observedAt)}</p>
                  {file.warnings.map((warning) => <p className="mt-2 text-sm text-warning" key={warning}>{warning}</p>)}
                  {file.errors.map((problem) => (
                    <p className="mt-2 text-sm text-danger" key={`${problem.code}-${problem.row}`}>
                      {problem.row ? `Row ${problem.row}${problem.column ? `, ${problem.column}` : ""}: ` : ""}{problem.message}
                    </p>
                  ))}
                </div>
                <Metric label="Sell orders" value={file.sellOrderCount} />
                <Metric label="Lowest sell" value={file.lowestSell ? <MoneyAmount mode="detail" value={file.lowestSell} /> : "Unavailable"} />
                <Metric label="Buy orders" value={file.buyOrderCount} />
                <Metric label="Highest buy" value={file.highestBuy ? <MoneyAmount mode="detail" value={file.highestBuy} /> : "Unavailable"} />
              </div>
            ))}
          </div>

          {preview.validFiles === 0 && preview.duplicateFiles > 0 && preview.invalidFiles === 0 ? (
            <div className="mt-4">
              <InlineAlert title="Already imported" tone="info">
                Every selected file is already stored. No new market observations will be added.
              </InlineAlert>
            </div>
          ) : null}

          <div className="mt-5 flex flex-wrap justify-end gap-2">
            {preview.files.some((file) => file.locationId !== null && file.locationName === `Structure ${file.locationId}`) ? (
              <button
                className="iw-button-secondary"
                disabled={busy || resolvingLocations}
                onClick={onResolveLocations}
                type="button"
              >
                <RefreshCw className={`mr-2 h-4 w-4 ${resolvingLocations ? "animate-spin" : ""}`} />
                {resolvingLocations ? "Resolving..." : "Retry location names"}
              </button>
            ) : null}
            <button className="iw-button-secondary" disabled={busy} onClick={onCancel} type="button">Cancel</button>
            <button
              className="iw-button-primary"
              disabled={busy || preview.validFiles === 0}
              onClick={onImport}
              type="button"
            >
              <CheckCircle2 className="mr-2 h-4 w-4" />
              {importButtonLabel(busy, preview)}
            </button>
          </div>
        </div>
      ) : null}
    </dialog>
  );
}
