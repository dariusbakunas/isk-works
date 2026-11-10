import { useCallback, useEffect, useRef, useState } from "react";

import { createLinkedBuild, type Build } from "../../../../api/industry";
import type { ComponentResolutionState } from "../components/planner-panels";

export interface LinkedBuildsOptions {
  /** The saved Build's id; nothing is linked before the first save. */
  buildId: string | undefined;
  /** A saved Build's recipe is still hydrating: hold off until it has. */
  initializing: boolean;
  /** Direct materials resolved to Build, by type id. */
  componentResolutions: Record<number, ComponentResolutionState>;
  /** Saves the parent if it has unsaved changes; its id, or null on failure. */
  queueSaveIfChanged: () => Promise<string | null>;
}

/**
 * The active linked-build lifecycle. Every direct material currently
 * resolved to "Build" needs a real linked Build. This lives on editor state,
 * rather than in an effect inside `AcquisitionControl`, so it runs whether
 * or not the inspector / `AcquisitionControl` is mounted.
 * `AcquisitionControl` is purely presentational with respect to it -- it
 * reads `linkedBuildsByTypeId` / `linkedBuildPending` / `linkedBuildErrors`
 * and never issues the calls itself.
 */
export function useLinkedBuilds({
  buildId,
  initializing,
  componentResolutions,
  queueSaveIfChanged,
}: LinkedBuildsOptions) {
  const [linkedBuildsByTypeId, setLinkedBuildsByTypeId] = useState<Record<number, Build>>({});
  const [linkedBuildPending, setLinkedBuildPending] = useState<Record<number, boolean>>({});
  const linkedBuildsSettling = Object.keys(linkedBuildPending).length > 0;
  const [linkedBuildErrors, setLinkedBuildErrors] = useState<Record<number, string>>({});
  // Per-type in-flight guard -- a ref, so effect reruns / StrictMode double
  // invokes can't launch a second create/resync for the same component.
  const linkedBuildInFlightRef = useRef<Set<number>>(new Set());

  async function buildLinkedComponent(typeId: number): Promise<Build> {
    const savedId = await queueSaveIfChanged();
    if (!savedId) throw new Error("Save the build before creating a linked build.");
    return createLinkedBuild(savedId, { componentTypeId: typeId });
  }

  // The one call that create-or-reuses (and, server-side, resyncs `runs` on)
  // a linked Build. `buildLinkedComponent` saves the parent through the
  // shared queue first, then hits `create_or_reuse_linked_build`, which
  // reuses a retained/inactive linked Build rather than duplicating it.
  function runLinkedBuildCall(typeId: number) {
    if (!buildId) return;
    if (linkedBuildInFlightRef.current.has(typeId)) return;
    linkedBuildInFlightRef.current.add(typeId);
    setLinkedBuildPending((current) => ({ ...current, [typeId]: true }));
    setLinkedBuildErrors((current) => {
      if (!(typeId in current)) return current;
      const next = { ...current };
      delete next[typeId];
      return next;
    });
    buildLinkedComponent(typeId)
      .then((build) => {
        setLinkedBuildsByTypeId((current) => ({ ...current, [typeId]: build }));
      })
      .catch(() => {
        setLinkedBuildErrors((current) => ({
          ...current,
          [typeId]: "Could not create the linked build. Try again.",
        }));
      })
      .finally(() => {
        linkedBuildInFlightRef.current.delete(typeId);
        setLinkedBuildPending((current) => {
          const next = { ...current };
          delete next[typeId];
          return next;
        });
      });
  }
  function ensureLinkedBuild(typeId: number) {
    if (linkedBuildsByTypeId[typeId]) return;
    runLinkedBuildCall(typeId);
  }
  const ensureLinkedBuildRef = useRef(ensureLinkedBuild);
  ensureLinkedBuildRef.current = ensureLinkedBuild;

  // Adopt a freshly-loaded producer Build into the map -- e.g. after the
  // Graph inspector patches that Build's own settings by id. Keeps the
  // Worksheet inspector (which reads this map) in step with a cross-view
  // edit. Only a producer already in the map is replaced.
  const adoptLinkedBuild = useCallback((build: Build) => {
    setLinkedBuildsByTypeId((current) => {
      const entry = Object.entries(current).find(([, linked]) => linked.id === build.id);
      if (!entry || entry[1] === build) return current;
      return { ...current, [entry[0]]: build };
    });
  }, []);

  // Ensure + prune: keep the linked-Build map in step with which direct
  // materials are currently Build-resolved.
  useEffect(() => {
    if (!buildId || initializing) return;
    const activeTypeIds = new Set(Object.keys(componentResolutions).map(Number));

    const stale = Object.keys(linkedBuildsByTypeId)
      .map(Number)
      .filter((typeId) => !activeTypeIds.has(typeId));
    if (stale.length > 0) {
      // Build -> Buy: forget the linked Build locally. Nothing is deleted
      // server-side -- it stays retained/inactive and is reused if the row
      // flips back to Build.
      setLinkedBuildsByTypeId((current) => {
        const next = { ...current };
        for (const typeId of stale) delete next[typeId];
        return next;
      });
      setLinkedBuildErrors((current) => {
        if (!stale.some((typeId) => typeId in current)) return current;
        const next = { ...current };
        for (const typeId of stale) delete next[typeId];
        return next;
      });
    }

    for (const typeId of activeTypeIds) ensureLinkedBuildRef.current(typeId);
    // `linkedBuildPending`/`linkedBuildErrors` are deliberately *not* deps:
    // updating them must not re-run this effect (that would auto-retry a
    // just-failed creation in a loop). A genuine Buy->Build toggle changes
    // `componentResolutions` and does re-run it, so retry stays possible.
  }, [buildId, initializing, componentResolutions, linkedBuildsByTypeId]);

  return {
    linkedBuildsByTypeId,
    linkedBuildPending,
    linkedBuildsSettling,
    linkedBuildErrors,
    adoptLinkedBuild,
    buildLinkedComponent,
  };
}
