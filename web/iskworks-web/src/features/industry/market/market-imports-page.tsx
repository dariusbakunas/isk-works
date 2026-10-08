import { CheckCircle2, FileUp, RefreshCw, Upload } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  importMarketExports,
  listMarketImports,
  previewMarketExports,
  resolveMarketLocations,
  type MarketImportBatch,
  type MarketImportPreview,
  type MarketImportResult,
  type MarketLocationResolution,
} from "../../../api/industry";
import { EmptyState, InlineAlert, PageHeader, Panel } from "../../../components/primitives";
import { MarketImportPreviewDialog } from "./market-import-preview-dialog";
import { apiMessage, formatDate, number } from "./shared";

export function MarketImportsPage() {
  const inputRef = useRef<HTMLInputElement>(null);
  const [files, setFiles] = useState<File[]>([]);
  const [preview, setPreview] = useState<MarketImportPreview | null>(null);
  const [result, setResult] = useState<MarketImportResult | null>(null);
  const [imports, setImports] = useState<MarketImportBatch[]>([]);
  const [resolution, setResolution] = useState<MarketLocationResolution | null>(null);
  const [resolvingLocations, setResolvingLocations] = useState(false);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState("");

  function loadImports() {
    listMarketImports().then(setImports).catch((requestError) => setError(apiMessage(requestError)));
  }
  useEffect(loadImports, []);

  function selectFiles(next: File[]) {
    setFiles(next);
    setPreview(null);
    setResult(null);
    setResolution(null);
    setError("");
    if (next.length > 0) void runPreview(next);
  }

  async function runPreview(selectedFiles = files) {
    setBusy(true);
    setError("");
    try {
      const parsed = await previewMarketExports(selectedFiles);
      setPreview(parsed);
      await runLocationResolution(parsed);
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  async function runLocationResolution(currentPreview = preview) {
    if (!currentPreview) return;
    const locationIds = Array.from(new Set(
      currentPreview.files
        .filter((file) => file.locationId !== null && file.locationName === `Structure ${file.locationId}`)
        .map((file) => file.locationId as number),
    ));
    if (locationIds.length === 0) {
      setResolution(null);
      return;
    }
    setResolvingLocations(true);
    try {
      const next = await resolveMarketLocations(locationIds);
      const names = new Map(next.resolved.map((location) => [location.locationId, location.locationName]));
      setPreview((value) => value ? {
        ...value,
        files: value.files.map((file) => ({
          ...file,
          locationName: file.locationId === null ? file.locationName : names.get(file.locationId) ?? file.locationName,
        })),
      } : value);
      setResolution(next);
    } catch (requestError) {
      setResolution({
        configured: true,
        resolved: [],
        unresolvedLocationIds: locationIds,
        eligibleCharacterCount: 0,
        needsReconnection: false,
        warnings: [apiMessage(requestError)],
      });
    } finally {
      setResolvingLocations(false);
    }
  }

  async function runImport() {
    setBusy(true);
    setError("");
    try {
      const imported = await importMarketExports(files);
      setResult(imported);
      setPreview(null);
      setFiles([]);
      if (inputRef.current) inputRef.current.value = "";
      loadImports();
    } catch (requestError) {
      setError(apiMessage(requestError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageHeader eyebrow="Client market evidence" title="Market Imports">
        Preview EVE client market exports, then explicitly import location-specific order-book observations. Imported orders are queried by region/location like any other market data -- no Price Override needs to be created.
      </PageHeader>
      {error && !preview ? <InlineAlert title="Market import unavailable">{error}</InlineAlert> : null}

      <Panel className="mb-4">
        <div
          className={`grid min-h-40 place-items-center border border-dashed p-5 text-center ${dragging ? "border-primary bg-primary/10" : "border-border bg-background/30"}`}
          onDragEnter={(event) => {
            event.preventDefault();
            setDragging(true);
          }}
          onDragOver={(event) => event.preventDefault()}
          onDragLeave={() => setDragging(false)}
          onDrop={(event) => {
            event.preventDefault();
            setDragging(false);
            selectFiles(Array.from(event.dataTransfer.files));
          }}
        >
          <div>
            <Upload className="mx-auto h-7 w-7 text-primary" aria-hidden="true" />
            <strong className="mt-2 block">Drop EVE market exports here</strong>
            <p className="iw-muted mt-1 text-sm">Up to 20 .txt or .csv files, 5 MiB each and 25 MiB total.</p>
            <button className="iw-button-secondary mt-3" onClick={() => inputRef.current?.click()} type="button">
              <FileUp className="mr-2 h-4 w-4" /> Choose files
            </button>
            <input
              ref={inputRef}
              accept=".txt,.csv,text/plain,text/csv"
              className="sr-only"
              multiple
              onChange={(event) => selectFiles(Array.from(event.target.files ?? []))}
              type="file"
            />
          </div>
        </div>
        {files.length > 0 ? (
          <div className="mt-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <strong>{files.length} file{files.length === 1 ? "" : "s"} selected</strong>
              {busy && !preview ? <span className="text-sm text-muted" role="status">Parsing market exports...</span> : null}
              {error && !busy && !preview ? (
                <button className="iw-button-secondary" onClick={() => void runPreview()} type="button">
                  Retry preview
                </button>
              ) : null}
            </div>
            <div className="mt-2 divide-y divide-border">
              {files.map((file) => (
                <div className="flex min-w-0 justify-between gap-3 py-2 text-sm" key={`${file.name}-${file.size}`}>
                  <span className="min-w-0 break-all">{file.name}</span>
                  <span className="shrink-0 font-mono text-muted">{number.format(file.size)} B</span>
                </div>
              ))}
            </div>
          </div>
        ) : null}
      </Panel>

      <MarketImportPreviewDialog
        busy={busy}
        error={preview ? error : ""}
        onCancel={() => {
          setPreview(null);
          setError("");
        }}
        onImport={() => void runImport()}
        preview={preview}
        resolution={resolution}
        resolvingLocations={resolvingLocations}
        onResolveLocations={() => void runLocationResolution()}
      />

      {result ? (
        <Panel className="mb-4">
          <div className="flex items-start gap-3">
            <CheckCircle2 className="mt-0.5 h-5 w-5 text-success" aria-hidden="true" />
            <div>
              <h2 className="font-semibold">
                {result.batch ? "Market observations imported" : "No new observations"}
              </h2>
              <p className="iw-muted mt-1">
                {number.format(result.importedObservations)} observations across {result.importedFiles} location order books.
                {result.skippedDuplicateFiles > 0 ? ` ${result.skippedDuplicateFiles} duplicate order books skipped.` : ""}
              </p>
            </div>
          </div>
        </Panel>
      ) : null}

      <Panel>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h2 className="font-semibold">Recent import batches</h2>
          <button className="iw-icon-button" onClick={loadImports} title="Refresh imports" type="button">
            <RefreshCw className="h-4 w-4" /><span className="sr-only">Refresh imports</span>
          </button>
        </div>
        {imports.length === 0 ? (
          <EmptyState title="No market imports">
            Export market orders from the EVE client, then upload the files here.
          </EmptyState>
        ) : (
          <div className="mt-3 divide-y divide-border">
            {imports.map((batch) => (
              <div className="grid gap-2 py-3 sm:grid-cols-[minmax(0,1fr)_auto]" key={batch.id}>
                <div>
                  <strong>{batch.itemCount} item{batch.itemCount === 1 ? "" : "s"} · {number.format(batch.observationCount)} orders</strong>
                  <p className="iw-muted mt-1 text-sm">{batch.files.map((file) => file.typeName).join(", ")}</p>
                </div>
                <div className="sm:text-right">
                  <span className="text-sm">{formatDate(batch.importedAt)}</span>
                  <p className="text-xs text-muted">{batch.fileCount} files · {batch.locationCount} locations</p>
                </div>
              </div>
            ))}
          </div>
        )}
      </Panel>
    </>
  );
}
