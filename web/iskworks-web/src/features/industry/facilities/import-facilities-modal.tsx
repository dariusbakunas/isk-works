import { Upload, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import {
  importFacilities,
  previewFacilityImport,
  type FacilityImportAction,
  type FacilityImportItemResult,
  type FacilityImportPreviewItem,
  type FacilityInput,
} from "../../../api/industry";
import { InlineAlert, StatusBadge } from "../../../components/primitives";
import { apiMessage } from "../shared/api-error";

type ImportState =
  | { status: "select" }
  | { status: "previewing" }
  | { status: "review"; inputs: FacilityInput[]; preview: FacilityImportPreviewItem[]; choices: Record<number, "skip" | "replace"> }
  | { status: "importing" }
  | { status: "done"; results: FacilityImportItemResult[] }
  | { status: "error"; message: string };

export function ImportFacilitiesModal({ onClose, onImported }: { onClose: () => void; onImported: () => void }) {
  const [state, setState] = useState<ImportState>({ status: "select" });
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog || dialog.open) return;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else dialog.setAttribute("open", "");
  }, []);

  async function handleFile(file: File) {
    setState({ status: "previewing" });
    try {
      const parsed = JSON.parse(await readFileAsText(file)) as { items?: unknown };
      if (!Array.isArray(parsed.items)) throw new Error("missing items array");
      const inputs = parsed.items as FacilityInput[];
      const response = await previewFacilityImport(inputs);
      setState({ status: "review", inputs, preview: response.items, choices: {} });
    } catch (error) {
      setState({
        status: "error",
        message: error instanceof SyntaxError
          ? "That file doesn't look like an ISK Works facilities export."
          : apiMessage(error),
      });
    }
  }

  function setAllDuplicates(choice: "skip" | "replace") {
    if (state.status !== "review") return;
    setState({
      ...state,
      choices: Object.fromEntries(
        state.preview
          .filter((item) => item.classification === "duplicate")
          .map((item) => [item.index, choice]),
      ),
    });
  }

  const actions = useMemo<FacilityImportAction[]>(() => {
    if (state.status !== "review") return [];
    return state.preview
      .filter((item) => item.classification !== "invalid")
      .map((item) => {
        const input = state.inputs[item.index];
        if (item.classification === "new") return { action: "create", item: input };
        const choice = state.choices[item.index] ?? "skip";
        return choice === "replace"
          ? {
              action: "replace",
              item: input,
              existingId: item.existingId ?? undefined,
              expectedRevision: item.existingRevision ?? undefined,
            }
          : { action: "skip", item: input };
      });
  }, [state]);
  const writeCount = actions.filter((action) => action.action !== "skip").length;

  async function executeImport() {
    if (writeCount === 0) return;
    setState({ status: "importing" });
    try {
      const response = await importFacilities(actions);
      onImported();
      setState({ status: "done", results: response.results });
    } catch (error) {
      setState({ status: "error", message: apiMessage(error) });
    }
  }

  return (
    <dialog
      aria-labelledby="facility-import-title"
      className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(680px,calc(100vw-2rem))] overflow-y-auto p-5 text-foreground backdrop:bg-black/70"
      onCancel={(event) => { event.preventDefault(); onClose(); }}
      ref={dialogRef}
    >
      <div className="mb-4 flex items-start justify-between gap-3">
        <div><p className="iw-eyebrow">Bulk facility import</p><h2 className="text-base font-semibold" id="facility-import-title">Import Facilities</h2></div>
        <button aria-label="Close import" className="iw-icon-button border-0" onClick={onClose} type="button"><X className="h-4 w-4" /></button>
      </div>
      {state.status === "select" ? (
        <label className="grid gap-2 text-sm font-semibold">
          Export file
          <input accept="application/json" className="iw-input" onChange={(event) => { const file = event.target.files?.[0]; if (file) void handleFile(file); }} type="file" />
        </label>
      ) : null}
      {state.status === "previewing" ? <p className="iw-muted text-sm" role="status">Checking facilities...</p> : null}
      {state.status === "error" ? <InlineAlert title="Import not ready">{state.message}</InlineAlert> : null}
      {state.status === "review" ? (
        <div className="grid gap-3">
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <strong>{state.preview.filter((item) => item.classification === "new").length} new</strong>
            <span className="text-muted">·</span>
            <strong className="text-warning">{state.preview.filter((item) => item.classification === "duplicate").length} duplicates</strong>
            <span className="text-muted">·</span>
            <strong className="text-negative">{state.preview.filter((item) => item.classification === "invalid").length} invalid</strong>
          </div>
          {state.preview.some((item) => item.classification === "duplicate") ? (
            <div className="flex gap-2"><button className="iw-button-secondary" onClick={() => setAllDuplicates("skip")} type="button">Skip all duplicates</button><button className="iw-button-secondary" onClick={() => setAllDuplicates("replace")} type="button">Replace all duplicates</button></div>
          ) : null}
          <div className="grid max-h-80 gap-1 overflow-y-auto border-y border-border py-1">
            {state.preview.map((item) => (
              <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-border px-1 py-2 last:border-b-0" key={item.index}>
                <div className="min-w-0"><div className="flex items-center gap-2"><strong className="truncate text-sm">{item.name}</strong><StatusBadge>{item.classification}</StatusBadge></div>{item.existingName ? <p className="iw-muted mt-0.5 text-xs">Matches {item.existingName} · {item.matchBasis === "eveLocation" ? "EVE location" : "name and system"}</p> : null}{item.message ? <p className="mt-0.5 text-xs text-negative">{item.message}</p> : null}</div>
                {item.classification === "duplicate" ? <div className="flex"><button className={(state.choices[item.index] ?? "skip") === "skip" ? "iw-button-primary" : "iw-button-secondary"} onClick={() => setState({ ...state, choices: { ...state.choices, [item.index]: "skip" } })} type="button">Skip</button><button className={state.choices[item.index] === "replace" ? "iw-button-primary" : "iw-button-secondary"} onClick={() => setState({ ...state, choices: { ...state.choices, [item.index]: "replace" } })} type="button">Replace</button></div> : null}
              </div>
            ))}
          </div>
          <button className="iw-button-primary justify-self-start" disabled={writeCount === 0} onClick={() => void executeImport()} type="button"><Upload className="mr-2 h-4 w-4" />Import {writeCount} facilit{writeCount === 1 ? "y" : "ies"}</button>
        </div>
      ) : null}
      {state.status === "importing" ? <p className="iw-muted text-sm" role="status">Importing facilities...</p> : null}
      {state.status === "done" ? <div className="grid gap-3"><div className="grid max-h-64 gap-1 overflow-y-auto">{state.results.map((result, index) => <div className="flex justify-between gap-3 border-b border-border py-1 text-sm" key={`${result.name}-${index}`}><span className="truncate">{result.name}</span><span className={result.status === "failed" ? "text-negative" : "text-muted"}>{result.status}{result.message ? ` · ${result.message}` : ""}</span></div>)}</div><button className="iw-button-primary justify-self-start" onClick={onClose} type="button">Done</button></div> : null}
    </dialog>
  );
}

function readFileAsText(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error ?? new Error("Could not read file."));
    reader.readAsText(file);
  });
}
