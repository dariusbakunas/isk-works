// What a Type Info window keeps (spec 2026-10-28-type-info §5): the types it
// has shown, with back/forward like the client's « », plus its tab and pilot.
// JSON-serialisable: tool windows persist it.

/** The tool window kind Type Info registers as. */
export const TYPE_INFO_KIND = "type-info";
/** The most types one window remembers. */
export const MAX_HISTORY = 50;

/** One owned blueprint, when whoever opened the window knows which (Assets,
 * the Blueprint library). Type info is otherwise about the type alone. */
export interface OwnedBlueprint {
  kind: "original" | "copy";
  /** Runs left on a copy; `null` for an original, or when unknown. */
  runs: number | null;
  materialEfficiency: number;
  timeEfficiency: number;
}

/** An observed blueprint as type info wants it; `undefined` unless it is
 * known to be an original or a copy. */
export function ownedBlueprint(
  observed: { kind: string; licensedRuns: number | null; materialEfficiency: number; timeEfficiency: number } | null | undefined,
): OwnedBlueprint | undefined {
  if (observed?.kind !== "original" && observed?.kind !== "copy") return undefined;
  return {
    kind: observed.kind,
    runs: observed.kind === "copy" ? observed.licensedRuns : null,
    materialEfficiency: observed.materialEfficiency,
    timeEfficiency: observed.timeEfficiency,
  };
}

export interface TypeInfoEntry {
  typeId: number;
  /** Known once loaded (or given by whoever opened it); titles the window. */
  name?: string;
  /** Known once loaded; tags the window's icon. */
  metaGroupId?: number | null;
  /** The blueprint item this entry was opened from, if any. */
  blueprint?: OwnedBlueprint;
}

export interface TypeInfoWindowState {
  history: TypeInfoEntry[];
  index: number;
  /** `undefined`: the first tab the type has. */
  tab?: string;
  /** A blueprint's selected activity; `undefined`: the first it has. */
  activity?: string;
  /** `allV` (default), `allZero`, or a character's connection id. */
  pilot?: string;
}

function entryOf(typeId: number, name?: string, blueprint?: OwnedBlueprint): TypeInfoEntry {
  return { typeId, ...(name ? { name } : {}), ...(blueprint ? { blueprint } : {}) };
}

function sameBlueprint(a: OwnedBlueprint | undefined, b: OwnedBlueprint | undefined): boolean {
  return (
    a?.kind === b?.kind &&
    a?.runs === b?.runs &&
    a?.materialEfficiency === b?.materialEfficiency &&
    a?.timeEfficiency === b?.timeEfficiency
  );
}

export function newTypeInfoState(typeId: number, name?: string, blueprint?: OwnedBlueprint): TypeInfoWindowState {
  return { history: [entryOf(typeId, name, blueprint)], index: 0 };
}

export function currentEntry(state: TypeInfoWindowState): TypeInfoEntry {
  return state.history[Math.min(Math.max(state.index, 0), state.history.length - 1)];
}

export const canGoBack = (state: TypeInfoWindowState) => state.index > 0;
export const canGoForward = (state: TypeInfoWindowState) => state.index < state.history.length - 1;

/** Follows a link: shows `typeId` next, dropping anything forward of here.
 * The type already showing stays put, taking on `blueprint` when given: the
 * same blueprint type opened from another owned copy. */
export function showType(state: TypeInfoWindowState, typeId: number, name?: string, blueprint?: OwnedBlueprint): TypeInfoWindowState {
  const entry = currentEntry(state);
  if (entry.typeId === typeId) {
    if (!blueprint || sameBlueprint(entry.blueprint, blueprint)) return state;
    const history = [...state.history];
    history[state.history.indexOf(entry)] = { ...entry, blueprint };
    return { ...state, history };
  }
  const history = [...state.history.slice(0, state.index + 1), entryOf(typeId, name, blueprint)].slice(-MAX_HISTORY);
  return { ...state, history, index: history.length - 1, tab: undefined, activity: undefined };
}

export function goBack(state: TypeInfoWindowState): TypeInfoWindowState {
  return canGoBack(state) ? { ...state, index: state.index - 1, tab: undefined, activity: undefined } : state;
}

export function goForward(state: TypeInfoWindowState): TypeInfoWindowState {
  return canGoForward(state) ? { ...state, index: state.index + 1, tab: undefined, activity: undefined } : state;
}

export function withTab(state: TypeInfoWindowState, tab: string): TypeInfoWindowState {
  return { ...state, tab };
}

export function withActivity(state: TypeInfoWindowState, activity: string): TypeInfoWindowState {
  return { ...state, activity };
}

export function withPilot(state: TypeInfoWindowState, pilot: string): TypeInfoWindowState {
  return { ...state, pilot };
}

/** An open from elsewhere, reusing this window: like following a link in it. */
export function mergeTypeInfo(current: TypeInfoWindowState, incoming: TypeInfoWindowState): TypeInfoWindowState {
  const entry = currentEntry(incoming);
  return showType(current, entry.typeId, entry.name, entry.blueprint);
}

/** Records the loaded name and meta group on the current entry. */
export function rememberLoaded(
  state: TypeInfoWindowState,
  loaded: { typeId: number; name: string; metaGroupId: number | null },
): TypeInfoWindowState {
  const entry = currentEntry(state);
  if (entry.typeId !== loaded.typeId) return state;
  if (entry.name === loaded.name && entry.metaGroupId === loaded.metaGroupId) return state;
  const history = [...state.history];
  history[state.index] = { ...entry, name: loaded.name, metaGroupId: loaded.metaGroupId };
  return { ...state, history };
}
