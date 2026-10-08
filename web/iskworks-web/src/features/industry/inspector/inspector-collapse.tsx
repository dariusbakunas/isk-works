import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";

/**
 * Section expansion state for the unified inspector, keyed by a stable
 * section id (`"coverage"`, `"pricing"`, ...). It lives *above* any one
 * selected object so a user who expands FACILITY and COST keeps them
 * expanded while clicking through several graph nodes / worksheet rows --
 * only sections that don't apply to the new selection simply don't render.
 *
 * Each section declares its own default (collapsed vs expanded). The store
 * records only *deviations* from that default, so a section that was never
 * touched follows its default even as the default varies by object kind.
 */
interface InspectorCollapseStore {
  isExpanded: (sectionId: string, defaultExpanded: boolean) => boolean;
  toggle: (sectionId: string, defaultExpanded: boolean) => void;
}

const InspectorCollapseContext = createContext<InspectorCollapseStore | null>(null);

export function InspectorCollapseProvider({ children }: { children: ReactNode }) {
  // sectionId -> explicit user choice; absent === follow the section default.
  const [overrides, setOverrides] = useState<ReadonlyMap<string, boolean>>(() => new Map());

  const isExpanded = useCallback(
    (sectionId: string, defaultExpanded: boolean) =>
      overrides.has(sectionId) ? (overrides.get(sectionId) as boolean) : defaultExpanded,
    [overrides],
  );

  const toggle = useCallback(
    (sectionId: string, defaultExpanded: boolean) => {
      setOverrides((prev) => {
        const current = prev.has(sectionId) ? (prev.get(sectionId) as boolean) : defaultExpanded;
        const next = new Map(prev);
        next.set(sectionId, !current);
        return next;
      });
    },
    [],
  );

  const store = useMemo<InspectorCollapseStore>(() => ({ isExpanded, toggle }), [isExpanded, toggle]);
  return (
    <InspectorCollapseContext.Provider value={store}>{children}</InspectorCollapseContext.Provider>
  );
}

/** Falls back to an ephemeral per-hook store when no provider is mounted, so
 * a section is usable in isolation (and in unit tests) without ceremony. */
export function useInspectorCollapse(): InspectorCollapseStore {
  const fromContext = useContext(InspectorCollapseContext);
  const [fallback, setFallback] = useState<ReadonlyMap<string, boolean>>(() => new Map());
  const fallbackStore = useMemo<InspectorCollapseStore>(
    () => ({
      isExpanded: (sectionId, defaultExpanded) =>
        fallback.has(sectionId) ? (fallback.get(sectionId) as boolean) : defaultExpanded,
      toggle: (sectionId, defaultExpanded) =>
        setFallback((prev) => {
          const current = prev.has(sectionId)
            ? (prev.get(sectionId) as boolean)
            : defaultExpanded;
          const next = new Map(prev);
          next.set(sectionId, !current);
          return next;
        }),
    }),
    [fallback],
  );
  return fromContext ?? fallbackStore;
}
