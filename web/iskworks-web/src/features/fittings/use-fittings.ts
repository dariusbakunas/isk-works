import { useSearchParams } from "react-router";

import { listCharacters, type CharacterRosterEntry } from "../../api/characters";
import {
  getFit,
  getFitAssets,
  getFitCost,
  getFitPilots,
  getFitStats,
  getFittingsMeta,
  listFits,
  type FitAssetsView,
  type FitCostView,
  type FitDetail,
  type FitPilot,
  type FitStatsView,
  type FitSummary,
  type FittingsMeta,
  type PilotChoice,
} from "../../api/fittings";
import { useLoad, type Loaded } from "../../hooks/use-load";

import { workspaceTags } from "./fit-tags";

export function useFittingsMeta(): Loaded<FittingsMeta> {
  return useLoad((signal) => getFittingsMeta(signal), "meta");
}

/** `revision` refetches after an import, a copy or a delete. */
export function useFits(revision: number): Loaded<FitSummary[]> {
  return useLoad((signal) => listFits(signal), `fits:${revision}`);
}

/** Every tag in the workspace, for suggestions; refetches when `version` changes. */
export function useWorkspaceTags(version: string): string[] {
  const fits = useLoad((signal) => listFits(signal), `fit-tags:${version}`);
  return workspaceTags(fits.data ?? []);
}

export function useFit(id: string): Loaded<FitDetail> {
  return useLoad((signal) => getFit(id, signal), `fit:${id}`);
}

/** Stats wait (`null`) while the engine is unavailable. */
export function useFitStats(id: string, pilot: PilotChoice, profile: string, enabled: boolean): Loaded<FitStatsView> {
  return useLoad(
    enabled ? (signal) => getFitStats(id, { pilot, profile }, signal) : null,
    `stats:${id}:${pilot}:${profile}:${enabled}`,
  );
}

export function useFitPilots(id: string, profile: string): Loaded<FitPilot[]> {
  return useLoad((signal) => getFitPilots(id, profile, signal), `pilots:${id}:${profile}`);
}

/** `version` (the fit's `updatedAt`) refetches after a save. */
export function useFitCost(id: string, version: string): Loaded<FitCostView> {
  return useLoad((signal) => getFitCost(id, signal), `cost:${id}:${version}`);
}

export function useFitAssets(id: string, version: string): Loaded<FitAssetsView> {
  return useLoad((signal) => getFitAssets(id, signal), `assets:${id}:${version}`);
}

/** For portraits: connection id → EVE character id. */
export function useRoster(): Loaded<CharacterRosterEntry[]> {
  return useLoad(() => listCharacters(), "roster");
}

/** The pilot (`?pilot=`, All V by default) and damage profile
 * (`?profile=`, uniform by default) every value on the page uses. */
export function useFitParams() {
  const [params, setParams] = useSearchParams();
  const pilot: PilotChoice = params.get("pilot") ?? "allV";
  const profile = params.get("profile") ?? "uniform";
  const set = (key: "pilot" | "profile", value: string, fallback: string) =>
    setParams(
      (current) => {
        const next = new URLSearchParams(current);
        if (value === fallback) next.delete(key);
        else next.set(key, value);
        return next;
      },
      { replace: true },
    );
  return {
    pilot,
    profile,
    setPilot: (value: PilotChoice) => set("pilot", value, "allV"),
    setProfile: (value: string) => set("profile", value, "uniform"),
  };
}
