import { useCallback, useEffect, useRef, useState } from "react";

import { createBuild, updateBuild, type Build } from "../../../../api/industry";
import { apiMessage } from "../../../../api/api-error";

export type BuildDraftInput = Parameters<typeof createBuild>[0];

export interface BuildSaveQueueOptions {
  initialBuild: Build | null;
  /** Whether name, recipe and runs are all present. */
  complete: boolean;
  /** The draft to save; may throw when the planning state is unfinished. */
  draftInput: () => BuildDraftInput;
  /** The draft as autosave compares it ("" when there is nothing to save). */
  snapshot: string;
  /** A new Build was created (the first save of an unsaved editor). */
  onCreated: (build: Build) => void;
  setError: (message: string) => void;
}

/**
 * Saving the Build: one queue that serializes every save (autosave, "Build
 * this component"), plus the debounced autosave on top of it.
 */
export function useBuildSaveQueue({
  initialBuild,
  complete,
  draftInput,
  snapshot,
  onCreated,
  setError,
}: BuildSaveQueueOptions) {
  const [buildRevision, setBuildRevisionState] = useState(initialBuild?.revision ?? null);
  // Mirrors `buildRevision` for a queued save that runs before the next
  // render: it must send the latest revision.
  const buildRevisionRef = useRef<number | null>(initialBuild?.revision ?? null);
  const setBuildRevision = useCallback((revision: number | null) => {
    buildRevisionRef.current = revision;
    setBuildRevisionState(revision);
  }, []);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved">("idle");

  async function save(): Promise<Build | null> {
    if (!complete) {
      setError("Name, blueprint, and valid runs are required.");
      return null;
    }
    setSaveStatus("saving");
    setError("");
    let draft: Build | null = null;
    try {
      const input = draftInput();
      const expectedRevision = buildRevisionRef.current ?? buildRevision;
      draft = initialBuild && expectedRevision !== null
        ? await updateBuild(initialBuild.id, { ...input, expectedRevision })
        : await createBuild(input);
      setBuildRevision(draft.revision);
      setSaveStatus("saved");
      if (!initialBuild) onCreated(draft);
    } catch (requestError) {
      setSaveStatus("idle");
      setError(
        draft
          ? `Build saved, but commit did not finish. ${apiMessage(requestError)}`
          : apiMessage(requestError),
      );
    }
    return draft;
  }

  // Serializes every save request through one queue, always invoking the
  // freshest `save` closure -- so a save requested while another is still
  // in flight waits its turn instead of racing it with a stale
  // `expectedRevision`.
  const saveRef = useRef(save);
  saveRef.current = save;
  // The draft as last persisted, and the one the latest render would save.
  // Equal means a save would only bump the revision.
  const lastSavedSnapshotRef = useRef("");
  const currentSnapshotRef = useRef("");
  currentSnapshotRef.current = snapshot;
  const buildIdRef = useRef(initialBuild?.id);
  buildIdRef.current = initialBuild?.id;
  const saveQueueRef = useRef<Promise<unknown>>(Promise.resolve());
  function enqueue<T>(task: () => Promise<T>): Promise<T> {
    const queued = saveQueueRef.current.catch(() => null).then(task);
    saveQueueRef.current = queued;
    return queued;
  }
  async function saveRecordingSnapshot(): Promise<Build | null> {
    const saving = currentSnapshotRef.current;
    const draft = await saveRef.current();
    if (draft) lastSavedSnapshotRef.current = saving;
    return draft;
  }
  function queueSave(): Promise<Build | null> {
    return enqueue(saveRecordingSnapshot);
  }
  // Like `queueSave`, but an existing build with nothing unsaved is not
  // re-saved: resolves to its id either way, or null if the save failed.
  function queueSaveIfChanged(): Promise<string | null> {
    return enqueue(async () => {
      const buildId = buildIdRef.current;
      const current = currentSnapshotRef.current;
      if (buildId && current && current === lastSavedSnapshotRef.current) return buildId;
      return (await saveRecordingSnapshot())?.id ?? null;
    });
  }

  // Autosave: debounces ~800ms after any change to the draft, then persists
  // via the same queue everything else uses. The first time the snapshot
  // becomes computable for an *existing* build is just the already-saved
  // data settling in from `initialBuild`, not an edit, so it seeds the
  // baseline instead of triggering a save; a brand new build has no such
  // baseline (its name starts empty, so the first valid snapshot already
  // reflects a real keystroke) and saves immediately.
  const hasSeededSnapshotRef = useRef(false);
  useEffect(() => {
    if (!snapshot) return;
    if (!hasSeededSnapshotRef.current) {
      hasSeededSnapshotRef.current = true;
      if (initialBuild) {
        lastSavedSnapshotRef.current = snapshot;
        return;
      }
    }
    if (snapshot === lastSavedSnapshotRef.current) return;
    const timer = window.setTimeout(() => {
      void queueSave();
    }, 800);
    return () => window.clearTimeout(timer);
    // Only a snapshot change schedules an autosave; `queueSave` and `initialBuild` are read fresh and must not retrigger it.
  }, [snapshot]);

  return { saveStatus, setBuildRevision, queueSave, queueSaveIfChanged };
}
