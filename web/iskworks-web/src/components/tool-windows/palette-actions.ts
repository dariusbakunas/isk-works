import type { PaletteEntry } from "../navigation/palette-search";
import type { ToolWindowsApi } from "./tool-windows-context";

/**
 * ⌘K actions for the open and minimised tool windows (spec §4.3): "Window:
 * Harpy" brings that window to the front, restoring it if minimised.
 */
export function toolWindowPaletteActions(api: ToolWindowsApi | null): PaletteEntry[] {
  if (!api) return [];
  return api.windows.flatMap((window) => {
    const kind = api.kinds.get(window.kind);
    if (!kind) return [];
    const title = kind.title(window.state);
    return [
      {
        id: `window:${window.id}`,
        kind: "action" as const,
        label: `Window: ${title}`,
        group: "Actions",
        icon: kind.icon,
        keywords: ["window", "tool", title],
        run: () => (window.minimized ? api.restore(window.id) : api.focus(window.id)),
      },
    ];
  });
}
