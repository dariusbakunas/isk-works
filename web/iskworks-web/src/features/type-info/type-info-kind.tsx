import { ArrowLeft, ArrowRight, Info } from "lucide-react";

import { TitleButton } from "../../components/tool-windows/tool-window";
import type { ToolWindowKind } from "../../components/tool-windows/tool-windows-context";
import {
  TYPE_INFO_KIND,
  canGoBack,
  canGoForward,
  currentEntry,
  goBack,
  goForward,
  mergeTypeInfo,
  type TypeInfoWindowState,
} from "../../components/type-info/type-info-state";
import { TypeIcon } from "./meta-tag";
import { TypeInfoWindow } from "./type-info-window";

function title(state: TypeInfoWindowState): string {
  const entry = currentEntry(state);
  return entry.name ?? `Type ${entry.typeId}`;
}

/** The Type Info tool window: EVE's "Show Info" for any item. */
export const typeInfoKind: ToolWindowKind<TypeInfoWindowState> = {
  kind: TYPE_INFO_KIND,
  icon: Info,
  title,
  titleIcon: false,
  scrollsOwnContent: true,
  renderIcon: (state, size) => {
    const entry = currentEntry(state);
    return <TypeIcon name={title(state)} size={size} typeId={entry.typeId} />;
  },
  renderTitleControls: ({ state, setState }) => (
    <>
      <TitleButton disabled={!canGoBack(state)} label="Back" onClick={() => setState(goBack(state))}>
        <ArrowLeft aria-hidden="true" className="h-3 w-3" />
      </TitleButton>
      <TitleButton disabled={!canGoForward(state)} label="Forward" onClick={() => setState(goForward(state))}>
        <ArrowRight aria-hidden="true" className="h-3 w-3" />
      </TitleButton>
    </>
  ),
  defaultSize: { w: 480, h: 560 },
  minSize: { w: 360, h: 280 },
  merge: mergeTypeInfo,
  render: (props) => <TypeInfoWindow {...props} />,
};
