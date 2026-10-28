// What a Type Info window keeps (spec 2026-10-28-type-info §5): the types it
// has shown, with back/forward like the client's « », plus its tab and pilot.
// JSON-serialisable: tool windows persist it.

/** The tool window kind Type Info registers as. */
export const TYPE_INFO_KIND = "type-info";
/** The most types one window remembers. */
export const MAX_HISTORY = 50;

export interface TypeInfoEntry {
  typeId: number;
  /** Known once loaded (or given by whoever opened it); titles the window. */
  name?: string;
  /** Known once loaded; tags the window's icon. */
  metaGroupId?: number | null;
}

export interface TypeInfoWindowState {
  history: TypeInfoEntry[];
  index: number;
  /** `undefined`: the first tab the type has. */
  tab?: string;
  /** `allV` (default), `allZero`, or a character's connection id. */
  pilot?: string;
}

export function newTypeInfoState(typeId: number, name?: string): TypeInfoWindowState {
  return { history: [name ? { typeId, name } : { typeId }], index: 0 };
}

export function currentEntry(state: TypeInfoWindowState): TypeInfoEntry {
  return state.history[Math.min(Math.max(state.index, 0), state.history.length - 1)];
}

export const canGoBack = (state: TypeInfoWindowState) => state.index > 0;
export const canGoForward = (state: TypeInfoWindowState) => state.index < state.history.length - 1;

/** Follows a link: shows `typeId` next, dropping anything forward of here. */
export function showType(state: TypeInfoWindowState, typeId: number, name?: string): TypeInfoWindowState {
  if (currentEntry(state).typeId === typeId) return state;
  const history = [...state.history.slice(0, state.index + 1), name ? { typeId, name } : { typeId }].slice(-MAX_HISTORY);
  return { ...state, history, index: history.length - 1, tab: undefined };
}

export function goBack(state: TypeInfoWindowState): TypeInfoWindowState {
  return canGoBack(state) ? { ...state, index: state.index - 1, tab: undefined } : state;
}

export function goForward(state: TypeInfoWindowState): TypeInfoWindowState {
  return canGoForward(state) ? { ...state, index: state.index + 1, tab: undefined } : state;
}

export function withTab(state: TypeInfoWindowState, tab: string): TypeInfoWindowState {
  return { ...state, tab };
}

export function withPilot(state: TypeInfoWindowState, pilot: string): TypeInfoWindowState {
  return { ...state, pilot };
}

/** An open from elsewhere, reusing this window: like following a link in it. */
export function mergeTypeInfo(current: TypeInfoWindowState, incoming: TypeInfoWindowState): TypeInfoWindowState {
  const entry = currentEntry(incoming);
  return showType(current, entry.typeId, entry.name);
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
  history[state.index] = { typeId: entry.typeId, name: loaded.name, metaGroupId: loaded.metaGroupId };
  return { ...state, history };
}
