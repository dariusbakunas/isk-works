import type { PaletteEntry } from "../navigation/palette-search";
import { launchToolWindow } from "./launch";
import type { ToolWindowsApi } from "./tool-windows-context";

/**
 * ⌘K actions for tool windows: "Open: Shipping calculator" for each tool
 * that opens on its own, and (spec §4.3) "Window: Harpy" for each open or
 * minimised window, bringing it to the front and restoring it if minimised.
 */
export function toolWindowPaletteActions(api: ToolWindowsApi | null): PaletteEntry[] {
  if (!api) return [];
  const launchers = [...api.kinds.values()].flatMap((kind) =>
    kind.launcher
      ? [
          {
            id: `tool:${kind.kind}`,
            kind: "action" as const,
            label: `Open: ${kind.launcher.label}`,
            group: "Actions",
            icon: kind.icon,
            keywords: ["tool", "window", ...kind.launcher.keywords],
            run: () => launchToolWindow(api, kind.kind),
          },
        ]
      : [],
  );
  return [...launchers, ...windowActions(api)];
}

function windowActions(api: ToolWindowsApi): PaletteEntry[] {
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
