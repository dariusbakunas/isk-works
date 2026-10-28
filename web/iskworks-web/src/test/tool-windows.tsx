import { Info } from "lucide-react";
import type { ReactNode } from "react";

import { ToolWindowLayer } from "../components/tool-windows/tool-window-layer";
import { ToolWindowsProvider, type ToolWindowKind } from "../components/tool-windows/tool-windows-context";
import { TYPE_INFO_KIND, currentEntry, mergeTypeInfo, type TypeInfoWindowState } from "../components/type-info/type-info-state";

/** A Type Info stand-in for page tests: titled by the type, no fetching. */
export const stubTypeInfoKind: ToolWindowKind<TypeInfoWindowState> = {
  kind: TYPE_INFO_KIND,
  icon: Info,
  title: (state) => currentEntry(state).name ?? `Type ${currentEntry(state).typeId}`,
  defaultSize: { w: 480, h: 560 },
  minSize: { w: 360, h: 280 },
  merge: mergeTypeInfo,
  render: ({ state }) => <p>Type {currentEntry(state).typeId}</p>,
};

/** Renders `children` with tool windows (the stub Type Info kind) available. */
export function WithToolWindows({ children }: { children: ReactNode }) {
  return (
    <ToolWindowsProvider kinds={[stubTypeInfoKind]}>
      {children}
      <ToolWindowLayer />
    </ToolWindowsProvider>
  );
}
