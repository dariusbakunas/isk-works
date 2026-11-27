import type { ToolWindowsApi } from "./tool-windows-context";

/** Brings the kind's window forward (restoring it if minimised), or opens one. */
export function launchToolWindow(api: ToolWindowsApi, kind: string): void {
  const launcher = api.kinds.get(kind)?.launcher;
  if (!launcher) return;
  const minimized = [...api.windows].reverse().find((window) => window.kind === kind && window.minimized);
  const open = api.windows.some((window) => window.kind === kind && !window.minimized);
  if (!open && minimized) api.restore(minimized.id);
  else api.open(kind, launcher.initialState());
}
