import { Upload } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import {
  importInventory,
  type InventoryExportItem,
  type InventoryImportItemResult,
} from "../../../api/inventory";
import { InlineAlert } from "../../../components/primitives";
import { apiMessage } from "./shared";

type ImportState =
  | { status: "select" }
  | { status: "ready"; items: InventoryExportItem[] }
  | { status: "importing"; items: InventoryExportItem[] }
  | { status: "done"; results: InventoryImportItemResult[] }
  | { status: "error"; message: string };

function readFileAsText(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ""));
    reader.onerror = () => reject(reader.error ?? new Error("Could not read file."));
    reader.readAsText(file);
  });
}

export function ImportInventoryModal({ onClose, onImported }: { onClose: () => void; onImported: () => void }) {
  const [state, setState] = useState<ImportState>({ status: "select" });
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const element = dialogRef.current;
    if (!element || element.open) return;
    if (typeof element.showModal === "function") element.showModal();
    else element.setAttribute("open", "");
  }, []);

  async function handleFile(file: File) {
    try {
      const text = await readFileAsText(file);
      const parsed = JSON.parse(text) as { items?: unknown };
      if (!Array.isArray(parsed.items)) throw new Error("missing items array");
      setState({ status: "ready", items: parsed.items as InventoryExportItem[] });
    } catch {
      setState({ status: "error", message: "That file doesn't look like an ISK Works inventory export." });
    }
  }

  async function runImport(items: InventoryExportItem[]) {
    setState({ status: "importing", items });
    try {
      const response = await importInventory(items);
      onImported();
      setState({ status: "done", results: response.results });
    } catch (error) {
      setState({ status: "error", message: apiMessage(error) });
    }
  }

  return (
    <dialog
      aria-labelledby="inventory-import-title"
      className="iw-dialog m-auto max-h-[calc(100vh-2rem)] w-[min(560px,calc(100vw-2rem))] overflow-y-auto p-5 text-foreground backdrop:bg-black/70"
      onCancel={(event) => { event.preventDefault(); onClose(); }}
      ref={dialogRef}
    >
      <div className="mb-4 flex items-start justify-between gap-3">
        <div>
          <p className="iw-eyebrow">Bulk balance import</p>
          <h2 className="text-lg font-semibold" id="inventory-import-title">Import Inventory</h2>
        </div>
        <button className="iw-button-secondary" onClick={onClose} type="button">Close</button>
      </div>
      {state.status === "select" ? (
        <div className="grid gap-3">
          <p className="iw-muted text-sm">
            Select an inventory export file. Each item becomes an opening balance in this workspace at its
            exported quantity and average cost — the source system&apos;s purchase-by-purchase history is not
            replayed.
          </p>
          <label className="text-sm font-semibold">
            Export file
            <input
              accept="application/json"
              className="iw-input mt-1"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) void handleFile(file);
              }}
              type="file"
            />
          </label>
        </div>
      ) : null}
      {state.status === "error" ? <div className="mt-3"><InlineAlert title="Import not ready">{state.message}</InlineAlert></div> : null}
      {state.status === "ready" ? (
        <div className="grid gap-3">
          <p className="text-sm">
            <strong>{state.items.length}</strong> item{state.items.length === 1 ? "" : "s"} ready to import as
            opening balances.
          </p>
          <button className="iw-button-primary justify-self-start" onClick={() => runImport(state.items)} type="button">
            <Upload className="mr-2 h-4 w-4" aria-hidden="true" />
            Import {state.items.length} Item{state.items.length === 1 ? "" : "s"}
          </button>
        </div>
      ) : null}
      {state.status === "importing" ? <p className="iw-muted text-sm" role="status">Importing {state.items.length} items...</p> : null}
      {state.status === "done" ? (
        <div className="grid gap-3">
          <p className="text-sm">
            <strong className="text-positive">{state.results.filter((result) => result.imported).length} imported</strong>
            {state.results.some((result) => !result.imported) ? (
              <>, <strong className="text-warning">{state.results.filter((result) => !result.imported).length} skipped</strong></>
            ) : null}
          </p>
          <div className="grid max-h-64 gap-1 overflow-y-auto">
            {state.results.map((result) => (
              <div
                className={`flex items-center justify-between gap-3 border-b border-border py-1 text-sm last:border-b-0 ${result.imported ? "" : "text-warning"}`}
                key={result.typeId}
              >
                <span className="min-w-0 truncate">{result.typeName}</span>
                <span className="shrink-0 text-xs">{result.imported ? "Imported" : result.message}</span>
              </div>
            ))}
          </div>
          <button className="iw-button-primary justify-self-start" onClick={onClose} type="button">Done</button>
        </div>
      ) : null}
    </dialog>
  );
}
