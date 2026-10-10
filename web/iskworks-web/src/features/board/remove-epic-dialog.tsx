import { useEffect, useState } from "react";

export type RemoveEpicChoice = "archive" | "delete";

const choices: Array<{ value: RemoveEpicChoice; label: string; detail: string }> = [
  {
    value: "archive",
    label: "Archive",
    detail: "Hide it from the Board and keep its history. Restore it anytime and pick up where you left off.",
  },
  {
    value: "delete",
    label: "Delete permanently",
    detail:
      "Removes the Epic and its frozen plan. Its tickets stay on the Board and inventory already recorded from them is kept. This cannot be undone.",
  },
];

/** The Epic's one way out: archive (the default, reversible) or delete. */
export function RemoveEpicDialog({
  open,
  canArchive,
  onCancel,
  onConfirm,
}: {
  open: boolean;
  /** `false` for an already archived Epic: only Delete remains. */
  canArchive: boolean;
  onCancel: () => void;
  onConfirm: (choice: RemoveEpicChoice) => void;
}) {
  const [choice, setChoice] = useState<RemoveEpicChoice>("archive");

  useEffect(() => {
    if (open) setChoice(canArchive ? "archive" : "delete");
  }, [open, canArchive]);

  if (!open) return null;
  const available = canArchive ? choices : choices.filter((option) => option.value === "delete");

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/70 p-4" role="presentation">
      <section
        aria-labelledby="remove-epic-title"
        aria-modal="true"
        className="iw-dialog w-full max-w-md p-4"
        role="dialog"
      >
        <h2 className="text-base font-semibold" id="remove-epic-title">
          Remove this Epic?
        </h2>
        <fieldset className="mt-3 grid gap-2">
          <legend className="sr-only">How to remove it</legend>
          {available.map((option) => (
            <label
              className={`block rounded-md border p-3 text-sm ${
                choice === option.value ? "border-primary bg-primary/10" : "border-border"
              }`}
              key={option.value}
            >
              <input
                checked={choice === option.value}
                className="mr-2"
                name="remove-epic"
                onChange={() => setChoice(option.value)}
                type="radio"
              />
              <span className="font-semibold">{option.label}</span>
              <span className="iw-muted mt-1 block">{option.detail}</span>
            </label>
          ))}
        </fieldset>
        <p className="iw-muted mt-3 text-sm">Either way, stock the Epic reserved goes back to free inventory.</p>
        <div className="mt-4 flex justify-end gap-2">
          <button className="iw-button-secondary" onClick={onCancel} type="button">
            Cancel
          </button>
          <button
            className={choice === "delete" ? "iw-button-danger" : "iw-button-primary"}
            onClick={() => onConfirm(choice)}
            type="button"
          >
            {choice === "delete" ? "Delete Epic" : "Archive Epic"}
          </button>
        </div>
      </section>
    </div>
  );
}
