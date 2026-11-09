import type * as InspectorCollapse from "../inspector-collapse";

/**
 * A test double for `inspector-collapse` whose sections start expanded.
 * For tests about what an inspector section *contains*: inspector sections
 * start collapsed, and expanding each one first would bury the point.
 * Toggling and persistence behave as in the real module. Use with:
 *
 *   vi.mock("<path>/inspector-collapse", () => expandedInspectorCollapse());
 */
export async function expandedInspectorCollapse(): Promise<typeof InspectorCollapse> {
  const actual = await import("../inspector-collapse");
  return {
    ...actual,
    useInspectorCollapse: () => {
      const store = actual.useInspectorCollapse();
      return {
        isExpanded: (sectionId) => store.isExpanded(sectionId, true),
        toggle: (sectionId) => store.toggle(sectionId, true),
      };
    },
  };
}
