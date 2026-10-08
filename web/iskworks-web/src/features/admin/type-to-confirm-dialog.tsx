import { useEffect, useState, type ReactNode } from "react";

/**
 * A destructive confirmation that only enables the action once the operator
 * has typed `expected` (case-insensitive, trimmed). The server re-checks the
 * same value, so this is a guard against slips, not the enforcement.
 */
export function TypeToConfirmDialog({
  open,
  title,
  children,
  expected,
  confirmLabel,
  busy = false,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  title: string;
  children: ReactNode;
  expected: string;
  confirmLabel: string;
  busy?: boolean;
  onCancel: () => void;
  onConfirm: (typed: string) => void;
}) {
  const [typed, setTyped] = useState("");

  useEffect(() => {
    if (open) setTyped("");
  }, [open]);

  if (!open) return null;
  const matches = typed.trim().toLowerCase() === expected.trim().toLowerCase() && expected !== "";

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="type-confirm-title"
        aria-modal="true"
        className="iw-dialog w-full max-w-md p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="type-confirm-title">
          {title}
        </h2>
        <div className="iw-muted mt-2">{children}</div>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (matches && !busy) onConfirm(typed);
          }}
        >
          <label className="mt-3 block text-sm font-semibold" htmlFor="type-confirm-input">
            Type <strong>{expected}</strong> to confirm
          </label>
          <input
            autoComplete="off"
            autoFocus
            className="iw-input mt-1"
            id="type-confirm-input"
            onChange={(event) => setTyped(event.target.value)}
            spellCheck={false}
            value={typed}
          />
          <div className="mt-4 flex justify-end gap-2">
            <button className="iw-button-secondary" onClick={onCancel} type="button">
              Cancel
            </button>
            <button className="iw-button-danger" disabled={!matches || busy} type="submit">
              {confirmLabel}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}
