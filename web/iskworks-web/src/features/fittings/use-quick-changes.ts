import { useEffect, useRef, useState } from "react";

import { updateFit, type FitDetail, type FitItemView } from "../../api/fittings";
import { draftFrom, toFitDraft } from "./fit-draft";

/**
 * Changes made outside the editor, like a module's state: shown at once and
 * saved straight away, one save at a time so each carries the ones before.
 * A save that fails puts the saved fit back and says so.
 */
export function useQuickChanges({ fit, onSaved }: { fit: FitDetail; onSaved: () => void }) {
  const [items, setItems] = useState<FitItemView[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef<FitItemView[] | null>(null);
  const pending = useRef(0);
  const queue = useRef<Promise<void>>(Promise.resolve());

  // Once nothing is waiting to save, the reloaded fit holds every change.
  useEffect(() => {
    if (pending.current === 0) {
      latest.current = null;
      setItems(null);
    }
  }, [fit]);

  const change = (update: (current: FitItemView[]) => FitItemView[]) => {
    const next = update(latest.current ?? fit.items);
    latest.current = next;
    setItems(next);
    setError(null);
    pending.current += 1;
    queue.current = queue.current.then(async () => {
      try {
        await updateFit(fit.id, toFitDraft({ ...draftFrom(fit), items: next }, fit.ship.typeId));
      } catch (reason) {
        latest.current = null;
        setItems(null);
        setError(reason instanceof Error ? reason.message : "The fit wasn't saved.");
      }
      pending.current -= 1;
      if (pending.current === 0) onSaved();
    });
  };

  return { items, error, change };
}
