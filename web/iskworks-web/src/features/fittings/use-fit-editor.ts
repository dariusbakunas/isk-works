import { useEffect, useMemo, useState } from "react";

import { calculateFit, updateFit, type FitDetail, type FitStatsView, type PilotChoice } from "../../api/fittings";
import { capStates, changeCount, draftFrom, toFitDraft, type Draft } from "./fit-draft";

const RECALCULATE_DELAY_MS = 250;

const storageKey = (fitId: string) => `iw:fit-draft:${fitId}`;

function readStored(fitId: string): Draft | null {
  try {
    const raw = window.sessionStorage.getItem(storageKey(fitId));
    return raw ? (JSON.parse(raw) as Draft) : null;
  } catch {
    return null;
  }
}

function store(fitId: string, draft: Draft | null) {
  try {
    if (draft) window.sessionStorage.setItem(storageKey(fitId), JSON.stringify(draft));
    else window.sessionStorage.removeItem(storageKey(fitId));
  } catch {
    // Storage off (private window): the draft just isn't kept across visits.
  }
}

/** What a calculation is for: the items and the pilot and profile. */
const keyOf = (draft: Draft, shipTypeId: number, pilot: PilotChoice, profile: string) =>
  JSON.stringify([toFitDraft(draft, shipTypeId).items, pilot, profile]);

/**
 * Editing one saved fit: a draft (kept in session storage while it has
 * changes, so leaving the page doesn't lose it), recalculated a moment
 * after each change for the chosen pilot and profile.
 */
export function useFitEditor({
  fit,
  pilot,
  profile,
  engineAvailable,
  onSaved,
}: {
  fit: FitDetail;
  pilot: PilotChoice;
  profile: string;
  engineAvailable: boolean;
  onSaved: () => void;
}) {
  const saved = useMemo(() => draftFrom(fit), [fit]);
  const [draft, setDraft] = useState<Draft | null>(() => readStored(fit.id));
  const [restored, setRestored] = useState(() => draft !== null);
  const [calculation, setCalculation] = useState<{ key: string; view: FitStatsView } | null>(null);
  const [calculating, setCalculating] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const changes = draft ? changeCount(saved, draft) : 0;
  const key = draft ? keyOf(draft, fit.ship.typeId, pilot, profile) : null;
  const savedKey = keyOf(saved, fit.ship.typeId, pilot, profile);

  // Keep a draft with changes across visits; forget it when there are none.
  useEffect(() => {
    store(fit.id, draft && changes > 0 ? draft : null);
  }, [fit.id, draft, changes]);

  useEffect(() => {
    if (changes === 0) return;
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault();
    };
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [changes]);

  useEffect(() => {
    if (!draft || !key || !engineAvailable || key === savedKey || calculation?.key === key) {
      setCalculating(false);
      return;
    }
    const sent = draft;
    const controller = new AbortController();
    setCalculating(true);
    const timer = window.setTimeout(() => {
      const { shipTypeId, items } = toFitDraft(sent, fit.ship.typeId);
      calculateFit({ fit: { shipTypeId, items }, pilot, profile }, controller.signal)
        .then((view) => {
          // States the module can't reach come down to its most; the result
          // stands for the capped draft too.
          const capped = capStates(sent, view.items);
          setCalculation({ key: keyOf(capped, fit.ship.typeId, pilot, profile), view });
          if (capped !== sent) setDraft((current) => (current === sent ? capped : current));
          setCalculating(false);
          setError(null);
        })
        .catch((reason: unknown) => {
          if (controller.signal.aborted) return;
          setError(reason instanceof Error ? reason.message : "Couldn't calculate the draft.");
          setCalculating(false);
        });
    }, RECALCULATE_DELAY_MS);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
    // `key` covers the draft's items, the pilot and the profile.
  }, [key, savedKey, engineAvailable]);

  const close = () => {
    setDraft(null);
    setRestored(false);
    setCalculation(null);
    setError(null);
  };

  return {
    editing: draft !== null,
    draft,
    setDraft: (change: (current: Draft) => Draft) => setDraft((current) => (current ? change(current) : current)),
    changes,
    /** The draft calculates differently from the saved fit. */
    differs: draft !== null && key !== savedKey,
    restored,
    calculating,
    saving,
    error,
    /** The calculation for the draft as it is now; `null` while it's out
     * of date (or matches the saved fit, whose stats the page already has). */
    current: calculation && calculation.key === key ? calculation.view : null,
    /** The latest calculation, even if the draft has moved on since. */
    latest: draft && key !== savedKey ? (calculation?.view ?? null) : null,
    start: () => {
      setDraft(saved);
      setRestored(false);
    },
    discard: close,
    save: async () => {
      if (!draft) return;
      setSaving(true);
      setError(null);
      try {
        await updateFit(fit.id, toFitDraft(draft, fit.ship.typeId));
        store(fit.id, null);
        close();
        onSaved();
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : "Couldn't save the fit.");
      } finally {
        setSaving(false);
      }
    },
  };
}
