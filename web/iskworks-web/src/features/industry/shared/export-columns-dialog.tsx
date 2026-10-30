import { useEffect, useState, type ReactNode } from "react";

import { FinanceCheckbox } from "../../finance/finance-filter-controls";
import { apiMessage } from "./api-error";

export interface ExportColumnOption<K extends string> {
  key: K;
  label: string;
}

/** The last export's columns, remembered per browser; unknown keys are dropped. */
export function readStoredColumns<K extends string>(storageKey: string, columns: readonly ExportColumnOption<K>[], fallback: readonly K[]): K[] {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(storageKey) ?? "null");
    if (!Array.isArray(stored)) return [...fallback];
    const keys = columns.map((column) => column.key).filter((key) => stored.includes(key));
    return keys.length > 0 ? keys : [...fallback];
  } catch {
    return [...fallback];
  }
}

export function storeColumns(storageKey: string, keys: readonly string[]): void {
  try {
    window.localStorage.setItem(storageKey, JSON.stringify(keys));
  } catch {
    // Storage off: it just won't be remembered.
  }
}

// Picks a CSV's columns before exporting. It starts from the columns this
// browser last exported (`storageKey`), else `tableColumns`, and hands
// `onExport` the picks in `columns` order. `onExport` fetches and saves the
// file; the dialog remembers the picks and closes once it resolves, and stays
// open with the error on failure.
export function ExportColumnsDialog<K extends string>({
  open,
  title,
  description,
  columns,
  tableColumns,
  storageKey,
  disabled = false,
  onClose,
  onExport,
}: {
  open: boolean;
  title: string;
  description: ReactNode;
  columns: readonly ExportColumnOption<K>[];
  tableColumns: readonly K[];
  storageKey: string;
  /** Nothing to export. */
  disabled?: boolean;
  onClose: () => void;
  onExport: (keys: K[]) => Promise<void>;
}) {
  const [selected, setSelected] = useState<K[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setSelected(readStoredColumns(storageKey, columns, tableColumns));
      setError(null);
    }
    // Only opening resets the picks; the caller's arrays change identity freely.
  }, [open]);

  if (!open) return null;

  const ordered = columns.map((column) => column.key).filter((key) => selected.includes(key));

  function toggle(key: K) {
    setSelected((current) => current.includes(key) ? current.filter((entry) => entry !== key) : [...current, key]);
  }

  async function exportCsv() {
    setBusy(true);
    setError(null);
    try {
      await onExport(ordered);
      storeColumns(storageKey, ordered);
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
        aria-labelledby="export-columns-title"
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-md p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="export-columns-title">{title}</h2>
        <p className="iw-muted mt-1">{description}</p>

        <div className="iw-dialog-body">
          <div className="mt-3 flex items-center gap-3 text-xs">
            <span className="font-semibold">Columns</span>
            <button
              className="ml-auto text-primary disabled:text-muted"
              disabled={busy}
              onClick={() => setSelected(columns.map((column) => column.key))}
              type="button"
            >
              All
            </button>
            <button className="text-primary disabled:text-muted" disabled={busy} onClick={() => setSelected([])} type="button">
              None
            </button>
            <button className="text-primary disabled:text-muted" disabled={busy} onClick={() => setSelected([...tableColumns])} type="button">
              Table columns
            </button>
          </div>
          <fieldset aria-label="Export columns" className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1.5 rounded-md border border-border p-3" disabled={busy}>
            {columns.map((column) => (
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
            disabled={busy || ordered.length === 0 || disabled}
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
