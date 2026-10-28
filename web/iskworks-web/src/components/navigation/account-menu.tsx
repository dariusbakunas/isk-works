import { ChevronDown, Info, LogOut } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { CharacterName, PRIVATE_ATTR } from "../../observability/private";

function initials(name: string): string {
  const letters = name
    .split(/\s+/)
    .filter(Boolean)
    .map((word) => word[0]);
  return (letters.length > 1 ? letters[0] + letters[letters.length - 1] : (letters[0] ?? "?")).toUpperCase();
}

const rowFocus = "focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary";

/**
 * The rail's account row: the signed-in character and the API version, with
 * a menu (opening upward, from the bottom of the rail) holding Sign out.
 */
export function AccountMenu({
  characterName,
  apiVersion,
  expanded,
  onSignOut,
  signingOut,
}: {
  characterName: string;
  apiVersion: string;
  expanded: boolean;
  onSignOut: () => void;
  signingOut: boolean;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handlePointerDown(event: PointerEvent) {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div className="relative" ref={rootRef}>
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        className={`flex h-9 w-full items-center overflow-hidden text-left hover:bg-hover ${rowFocus}`}
        onClick={() => setOpen((current) => !current)}
        type="button"
      >
        <span className="grid w-[51px] shrink-0 place-items-center">
          <span
            aria-hidden="true"
            className="grid h-6 w-6 place-items-center rounded-full border border-[#778797] bg-[#4c5661] text-[8px] font-semibold text-bright"
            {...PRIVATE_ATTR}
          >
            {initials(characterName)}
          </span>
        </span>
        <span className={expanded ? "min-w-0 flex-1" : "sr-only"}>
          <span className="sr-only">Account: </span>
          <CharacterName className="block truncate text-[10px] text-foreground" name={characterName} />
          <span className="block truncate font-mono text-[8px] text-subtle" title={`API ${apiVersion}`}>
            {apiVersion}
          </span>
        </span>
        {expanded ? <ChevronDown aria-hidden="true" className="mr-2 h-2.5 w-2.5 shrink-0 text-subtle" /> : null}
      </button>
      {open ? (
        <div
          className="absolute bottom-full left-[10px] z-50 mb-1 w-[190px] border border-border bg-[#151b23] shadow-2xl"
          role="menu"
        >
          <div className="border-b border-border p-3">
            <CharacterName className="block text-[11px] font-medium text-bright" name={characterName} />
            <div className="mt-0.5 font-mono text-[8px] text-subtle">{apiVersion}</div>
          </div>
          <button
            className={`flex h-9 w-full items-center gap-2 px-3 text-[10px] text-foreground hover:bg-hover disabled:opacity-60 ${rowFocus}`}
            disabled={signingOut}
            onClick={onSignOut}
            role="menuitem"
            type="button"
          >
            <LogOut aria-hidden="true" className="h-[13px] w-[13px] text-muted" />
            {signingOut ? "Signing out..." : "Sign out"}
          </button>
        </div>
      ) : null}
    </div>
  );
}

/** The API version for sessions without a character (the legacy workspace gate). */
export function ApiVersionLine({ apiVersion, expanded }: { apiVersion: string; expanded: boolean }) {
  return (
    <div className="flex h-8 w-full items-center text-muted">
      <span className="grid w-[51px] shrink-0 place-items-center">
        <Info aria-hidden="true" className="h-4 w-4" />
      </span>
      <span className={expanded ? "truncate font-mono text-[10px] text-subtle" : "sr-only"} title={`API ${apiVersion}`}>
        {apiVersion}
      </span>
    </div>
  );
}
