// Which Epic the Build's Plan view shows. `?epic=<id>` in the URL wins
// (opening the Plan from an Epic lands in that Epic's view); otherwise the
// last choice for this Build, remembered per browser. Either is honored
// only while it names one of this Build's open Epics -- a stale id falls
// back to "No Epic" (free stock).

export const EPIC_PARAM = "epic";

const storageKey = (buildId: string) => `iskworks:build-epic:${buildId}`;

export function readEpicParam(params: URLSearchParams): string | null {
  const value = params.get(EPIC_PARAM);
  return value && value.trim() !== "" ? value : null;
}

/** `params` with `?epic=` set to `epicId`, or removed for "No Epic". */
export function withEpicParam(params: URLSearchParams, epicId: string | null): URLSearchParams {
  const next = new URLSearchParams(params);
  if (epicId) next.set(EPIC_PARAM, epicId);
  else next.delete(EPIC_PARAM);
  return next;
}

// Browser storage can be unavailable (private mode, blocked site data):
// remembering is a convenience, never required.
export function rememberedEpic(buildId: string): string | null {
  try {
    return window.localStorage.getItem(storageKey(buildId));
  } catch {
    return null;
  }
}

export function rememberEpic(buildId: string, epicId: string | null) {
  try {
    if (epicId) window.localStorage.setItem(storageKey(buildId), epicId);
    else window.localStorage.removeItem(storageKey(buildId));
  } catch {
    /* not remembered */
  }
}

/**
 * The Epic to show: the URL's if it is one of `openEpicIds`, else the
 * remembered one if it is, else `null` (No Epic). A URL naming a closed or
 * unknown Epic falls back to No Epic rather than to the remembered choice,
 * so a stale link never silently shows a different Epic.
 */
export function resolveEpicSelection(
  urlEpicId: string | null,
  remembered: string | null,
  openEpicIds: ReadonlySet<string>,
): string | null {
  if (urlEpicId !== null) return openEpicIds.has(urlEpicId) ? urlEpicId : null;
  return remembered !== null && openEpicIds.has(remembered) ? remembered : null;
}
