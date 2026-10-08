import { useCallback, useEffect, useRef, useState } from "react";

import {
  getBuild,
  setBuildBlueprintSelection,
  setBuildFacility,
  setBuildPricing,
  type BlueprintSelection,
  type Build,
  type MarketPricingPolicy,
  type MarketScope,
} from "../../../api/industry";
import { apiMessage } from "../shared/api-error";

export interface BuildFacilityPatch {
  facilityProfileId: string | null;
  estimatedItemValue?: string | null;
}

export interface BuildPricingPatch {
  materialScope: MarketScope;
  outputScope: MarketScope;
  materialPricingPolicy: MarketPricingPolicy;
  outputPricingPolicy: MarketPricingPolicy;
  manualPriceListId?: string | null;
  expectedManualPriceListRevision?: number | null;
  facilityEivManual?: boolean;
}

export interface BuildSettings {
  /** The persisted Build whose settings this hook edits. `null` until the
   * first load resolves (or `buildId` is `null`). */
  build: Build | null;
  loading: boolean;
  /** A settings mutation is in flight. */
  pending: boolean;
  error: string | null;
  reload: () => void;
  updateBlueprintSelection: (selection: BlueprintSelection | null) => Promise<void>;
  updateFacility: (patch: BuildFacilityPatch) => Promise<void>;
  updatePricing: (patch: BuildPricingPatch) => Promise<void>;
}

/**
 * Edit ONE arbitrary Build's own "common settings" (its blueprint selection,
 * its facility for its recipe kind, its pricing configuration) in place,
 * against the Build-ID-addressable PATCH endpoints.
 *
 * Deliberately NOT `useBuildWorksheetEditor`: no staged draft, no preview
 * pipeline, no autosave/debounce, no linked-build lifecycle. Just a
 * revision-checked read-modify-write -- load the Build once, send one narrow
 * patch per edit, adopt the returned Build (and its bumped revision).
 * `onChanged` fires after every successful patch so a host (e.g. the Graph
 * view) can re-project.
 */
export function useBuildSettings(
  buildId: string | null,
  onChanged?: () => void,
  /** An already-loaded Build for `buildId` (e.g. the Graph node detail the
   * inspector already fetched). Passing this argument at all -- even `null`
   * -- switches the hook to seed mode: it adopts the seed as its value and
   * never issues its own GET except on an explicit `reload()`. Omit the
   * argument entirely to have the hook fetch on mount. */
  seed?: Build | null,
): BuildSettings {
  const usesSeed = seed !== undefined;
  const seedForId = seed && seed.id === buildId ? seed : null;
  const [build, setBuild] = useState<Build | null>(seedForId);
  const [loading, setLoading] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false);
  const [reloadToken, setReloadToken] = useState(0);

  // Seed mode: adopt the seed whenever it (or the target id) changes; the
  // Graph node detail resolves asynchronously and again on each selection.
  useEffect(() => {
    if (!usesSeed) return;
    setBuild(!buildId ? null : seedForId);
    setError(null);
  }, [usesSeed, buildId, seedForId]);

  useEffect(() => {
    if (!buildId) {
      if (!usesSeed) setBuild(null);
      return;
    }
    // Seed mode fetches only when explicitly reloaded.
    if (usesSeed && reloadToken === 0) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    getBuild(buildId)
      .then((next) => {
        if (!cancelled) setBuild(next);
      })
      .catch((requestError) => {
        if (!cancelled) setError(apiMessage(requestError));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // `usesSeed` is left out: seed mode is adopted above and only fetches on an explicit `reloadToken` bump.
  }, [buildId, reloadToken]);

  const reload = useCallback(() => {
    setBuild(null);
    setReloadToken((token) => token + 1);
  }, []);

  const run = useCallback(
    async (work: (current: Build) => Promise<Build>) => {
      if (inFlight.current || !build) return;
      inFlight.current = true;
      setPending(true);
      setError(null);
      try {
        const next = await work(build);
        setBuild(next);
        onChanged?.();
      } catch (requestError) {
        setError(apiMessage(requestError));
      } finally {
        inFlight.current = false;
        setPending(false);
      }
    },
    [build, onChanged],
  );

  const updateBlueprintSelection = useCallback(
    (selection: BlueprintSelection | null) =>
      run((current) =>
        setBuildBlueprintSelection(current.id, {
          expectedRevision: current.revision,
          blueprintSelection: selection,
        }),
      ),
    [run],
  );

  const updateFacility = useCallback(
    (patch: BuildFacilityPatch) =>
      run((current) =>
        setBuildFacility(current.id, {
          expectedRevision: current.revision,
          facilityProfileId: patch.facilityProfileId,
          estimatedItemValue: patch.estimatedItemValue,
        }),
      ),
    [run],
  );

  const updatePricing = useCallback(
    (patch: BuildPricingPatch) =>
      run((current) =>
        setBuildPricing(current.id, { expectedRevision: current.revision, ...patch }),
      ),
    [run],
  );

  return {
    build,
    loading,
    pending,
    error,
    reload,
    updateBlueprintSelection,
    updateFacility,
    updatePricing,
  };
}
