import { useId, useState, type FormEvent } from "react";

import { apiMessage } from "../../../../api/api-error";
import { InlineAlert } from "../../../../components/primitives";

/** Names a plan: used to save one and to rename one. */
export function PlanNameDialog({
  initialName,
  onCancel,
  onSubmit,
  submitLabel,
  title,
}: {
  initialName: string;
  onCancel: () => void;
  /** Resolves once saved; a rejection shows its message and keeps the dialog. */
  onSubmit: (name: string) => Promise<void>;
  submitLabel: string;
  title: string;
}) {
  const titleId = useId();
  const [name, setName] = useState(initialName);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const trimmed = name.trim();

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!trimmed || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit(trimmed);
    } catch (failure) {
      setError(apiMessage(failure, "The plan could not be saved."));
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <form
        aria-labelledby={titleId}
        aria-modal="true"
        className="iw-dialog iw-dialog-frame w-full max-w-md p-4"
        onSubmit={submit}
        role="dialog"
      >
        <h2 className="text-base font-semibold" id={titleId}>
          {title}
        </h2>
        <div className="iw-dialog-body mt-3 space-y-2">
          <label className="block text-xs text-muted">
            Name
            <input
              autoFocus
              className="iw-input mt-1 w-full"
              data-private=""
              maxLength={120}
              onChange={(event) => setName(event.target.value)}
              value={name}
            />
          </label>
          {error ? <InlineAlert title="Not saved">{error}</InlineAlert> : null}
        </div>
        <div className="iw-dialog-footer">
          <button className="iw-button-secondary" onClick={onCancel} type="button">
            Cancel
          </button>
          <button className="iw-button-primary" disabled={!trimmed || busy} type="submit">
            {submitLabel}
          </button>
        </div>
      </form>
    </div>
  );
}
