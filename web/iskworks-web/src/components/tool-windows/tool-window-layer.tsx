import { ToolWindow } from "./tool-window";
import { useToolWindows } from "./tool-windows-context";

/**
 * Where the open tool windows render: one fixed layer above the rail (z-30)
 * and below dropdowns (z-40/50), dialogs (z-50) and the palette (z-80).
 * Inside it, windows stack in focus order.
 */
export function ToolWindowLayer() {
  const api = useToolWindows();
  if (!api) return null;
  const open = api.windows.filter((window) => !window.minimized);
  if (open.length === 0) return null;
  // Stable DOM order (by id) so focusing a window doesn't move its node and
  // lose scroll or focus inside it; z-index carries the stacking.
  const stable = [...open].sort((a, b) => Number(a.id.slice(1)) - Number(b.id.slice(1)));
  return (
    <div className="pointer-events-none fixed inset-0 z-[35]" data-testid="tool-window-layer">
      {stable.map((window) => {
        const kind = api.kinds.get(window.kind);
        if (!kind) return null;
        return (
          <ToolWindow
            api={api}
            focused={window === api.front}
            key={window.id}
            kind={kind}
            window={window}
            zIndex={open.indexOf(window) + 1}
          />
        );
      })}
    </div>
  );
}
