import { Info } from "lucide-react";
import type { KeyboardEvent, MouseEvent } from "react";

import { useToolWindows } from "../tool-windows/tool-windows-context";
import { TYPE_INFO_KIND, newTypeInfoState } from "./type-info-state";

/**
 * EVE's (i): opens `typeId` in a Type Info window. A plain click reuses the
 * window in front; Shift- or ⌘/Ctrl-click opens another (spec §5.1).
 * Hidden until its row (a `group`) is hovered or focused, unless
 * `alwaysVisible`. Renders nothing outside the tool windows provider.
 */
export function TypeInfoButton({
  typeId,
  name,
  alwaysVisible = false,
  className = "",
}: {
  typeId: number;
  name: string;
  alwaysVisible?: boolean;
  className?: string;
}) {
  const windows = useToolWindows();
  if (!windows) return null;

  function handleClick(event: MouseEvent<HTMLButtonElement>) {
    // Rows activate on click; showing info isn't selecting the row.
    event.stopPropagation();
    windows?.open(TYPE_INFO_KIND, newTypeInfoState(typeId, name), {
      newWindow: event.shiftKey || event.metaKey || event.ctrlKey,
    });
  }

  return (
    <button
      aria-label={`Show info: ${name}`}
      className={`inline-grid h-5 w-5 shrink-0 place-items-center align-middle text-subtle hover:text-bright focus-visible:opacity-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-primary ${
        alwaysVisible ? "" : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
      } ${className}`}
      onClick={handleClick}
      // Enter on the button shouldn't also activate the row.
      onKeyDown={(event: KeyboardEvent<HTMLButtonElement>) => event.stopPropagation()}
      title={`Show info: ${name}`}
      type="button"
    >
      <Info aria-hidden="true" className="h-3.5 w-3.5" />
    </button>
  );
}
