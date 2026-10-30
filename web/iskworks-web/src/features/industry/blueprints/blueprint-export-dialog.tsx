import { useEffect, useState } from "react";

import { FinanceCheckbox } from "../../finance/finance-filter-controls";
import { apiMessage } from "../shared/api-error";
import {
  BLUEPRINT_EXPORT_COLUMNS,
  DEFAULT_EXPORT_COLUMNS,
  readExportColumns,
  storeExportColumns,
  type BlueprintExportColumnKey,
} from "./blueprint-export";

// Picks the CSV's columns before exporting. `onExport` fetches and saves the
// file; the dialog closes once it resolves and stays open on failure.
export function BlueprintExportDialog({
  open,
  total,
  onClose,
  onExport,
}: {
  open: boolean;
  total: number;
  onClose: () => void;
  onExport: (keys: BlueprintExportColumnKey[]) => Promise<void>;
}) {
  const [selected, setSelected] = useState<BlueprintExportColumnKey[]>(readExportColumns);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setSelected(readExportColumns());
      setError(null);
    }
  }, [open]);

  if (!open) return null;

  function toggle(key: BlueprintExportColumnKey) {
    setSelected((current) => current.includes(key) ? current.filter((entry) => entry !== key) : [...current, key]);
  }

  async function exportCsv() {
    setBusy(true);
    setError(null);
    try {
      await onExport(selected);
      storeExportColumns(selected);
      onClose();
    } catch (exportError) {
      setError(apiMessage(exportError));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="blueprint-export-title"
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-md p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="blueprint-export-title">Export blueprints</h2>
        <p className="iw-muted mt-1">
          A CSV of the {total.toLocaleString()} blueprint{total === 1 ? "" : "s"} matching the current filters, in the
          current sort order.
        </p>

        <div className="iw-dialog-body">
          <div className="mt-3 flex items-center gap-3 text-xs">
            <span className="font-semibold">Columns</span>
            <button
              className="ml-auto text-primary disabled:text-muted"
              disabled={busy}
              onClick={() => setSelected(BLUEPRINT_EXPORT_COLUMNS.map((column) => column.key))}
              type="button"
            >
              All
            </button>
            <button className="text-primary disabled:text-muted" disabled={busy} onClick={() => setSelected([])} type="button">
              None
            </button>
            <button className="text-primary disabled:text-muted" disabled={busy} onClick={() => setSelected(DEFAULT_EXPORT_COLUMNS)} type="button">
              Table columns
            </button>
          </div>
          <fieldset aria-label="Export columns" className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1.5 rounded-md border border-border p-3" disabled={busy}>
            {BLUEPRINT_EXPORT_COLUMNS.map((column) => (
              <FinanceCheckbox checked={selected.includes(column.key)} key={column.key} onChange={() => toggle(column.key)}>
                <span className="text-xs">{column.label}</span>
              </FinanceCheckbox>
            ))}
          </fieldset>
          {error ? <p className="mt-2 text-xs text-danger" role="alert">{error}</p> : null}
        </div>

        <div className="iw-dialog-footer items-center justify-end gap-2">
          <button className="iw-button-secondary" disabled={busy} onClick={onClose} type="button">Cancel</button>
          <button
            className="iw-button-primary"
            disabled={busy || selected.length === 0 || total === 0}
            onClick={() => void exportCsv()}
            type="button"
          >
            {busy ? "Exporting…" : "Export CSV"}
          </button>
        </div>
      </section>
    </div>
  );
}
