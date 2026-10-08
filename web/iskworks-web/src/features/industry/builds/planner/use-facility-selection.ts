import { useEffect, useMemo, useRef, useState } from "react";

import { type AutomaticEiv, type FacilityProfile, type FacilityRole } from "../../../../api/industry";
import { apiMessage } from "../../shared/api-error";

/**
 * One role-filtered facility slot (manufacturing or reaction), sliced from
 * an already-fetched facility list (the caller fetches `listFacilities()`
 * once and shares it across both slots, rather than each slot fetching its
 * own copy). Automatic-EIV fetching is opt-in via `fetchAutomaticEiv`,
 * since only the root job's own slot has a client-editable EIV -- a
 * build-resolved sub-component's EIV is always resolved automatically
 * server-side.
 */
export function useFacilitySelection({
  allFacilities,
  enabled = true,
  fetchAutomaticEiv,
  initialEstimatedItemValue = "",
  initialFacilityProfileId = "",
  initialManualEiv = false,
  role,
  runs,
}: {
  allFacilities: FacilityProfile[];
  enabled?: boolean;
  fetchAutomaticEiv?: (runs: number) => Promise<AutomaticEiv>;
  initialEstimatedItemValue?: string;
  initialFacilityProfileId?: string;
  initialManualEiv?: boolean;
  role: FacilityRole;
  runs: number | null;
}) {
  const facilities = useMemo(
    () => allFacilities.filter((item) => !item.archivedAt && item.role === role),
    [allFacilities, role],
  );
  const [facilityId, setFacilityId] = useState(initialFacilityProfileId);
  const [estimatedItemValue, setEstimatedItemValue] = useState(initialEstimatedItemValue);
  const [automaticEiv, setAutomaticEiv] = useState<AutomaticEiv | null>(null);
  const [eivLoading, setEivLoading] = useState(false);
  const [eivError, setEivError] = useState("");
  const [manualEiv, setManualEiv] = useState(initialManualEiv);

  const selectedFacility = facilities.find((item) => item.id === facilityId);

  // `fetchAutomaticEiv` is typically a fresh closure every render (it
  // captures the caller's `selected` product). Reading it via a ref -- kept
  // current on every render, but never a dependency itself -- means this
  // effect only re-runs when something that should actually trigger a
  // re-fetch changes, instead of looping forever (each fetch sets new state
  // -> re-render -> new closure -> effect deps changed -> fetch again).
  const fetchAutomaticEivRef = useRef(fetchAutomaticEiv);
  useEffect(() => {
    fetchAutomaticEivRef.current = fetchAutomaticEiv;
  });

  useEffect(() => {
    const fetchEiv = fetchAutomaticEivRef.current;
    if (!enabled || !fetchEiv || !selectedFacility || !runs) return;
    const controller = new AbortController();
    setEivLoading(true);
    setEivError("");
    fetchEiv(runs)
      .then((result) => {
        if (controller.signal.aborted) return;
        setAutomaticEiv(result);
        if (!manualEiv && result.value) setEstimatedItemValue(result.value);
      })
      .catch((requestError) => {
        if (controller.signal.aborted) return;
        setAutomaticEiv(null);
        setEivError(apiMessage(requestError));
      })
      .finally(() => {
        if (!controller.signal.aborted) setEivLoading(false);
      });
    return () => controller.abort();
  }, [enabled, manualEiv, runs, selectedFacility?.id]);

  function reset(next: { estimatedItemValue?: string; facilityProfileId?: string; manualEiv?: boolean } = {}) {
    setFacilityId(next.facilityProfileId ?? "");
    setEstimatedItemValue(next.estimatedItemValue ?? "");
    setManualEiv(next.manualEiv ?? false);
    setAutomaticEiv(null);
    setEivError("");
  }

  return {
    automaticEiv,
    eivError,
    eivLoading,
    estimatedItemValue,
    facilities,
    facilityId,
    manualEiv,
    reset,
    selectedFacility,
    setEstimatedItemValue,
    setFacilityId,
    setManualEiv,
  };
}

