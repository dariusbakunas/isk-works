import type { MouseEvent } from "react";

import type { RichSegment } from "../../api/types";

/** Follows a `showinfo:` link; `newWindow` on Shift/⌘/Ctrl-click. */
export type OpenType = (typeId: number, name: string, newWindow: boolean) => void;

export const linkClass = "text-left text-primary hover:underline focus-visible:underline focus-visible:outline-none";

export function TypeLink({ typeId, name, onOpen, children }: { typeId: number; name: string; onOpen: OpenType; children?: React.ReactNode }) {
  return (
    <button
      className={linkClass}
      onClick={(event: MouseEvent<HTMLButtonElement>) => onOpen(typeId, name, event.shiftKey || event.metaKey || event.ctrlKey)}
      type="button"
    >
      {children ?? name}
    </button>
  );
}

/** CCP markup as parsed by the API: text, item links, web links, breaks. Never HTML. */
export function RichText({ segments, onOpen }: { segments: RichSegment[]; onOpen: OpenType }) {
  return (
    <>
      {segments.map((segment, index) => {
        switch (segment.kind) {
          case "text":
            return segment.bold ? <strong key={index}>{segment.text}</strong> : <span key={index}>{segment.text}</span>;
          case "link":
            return <TypeLink key={index} name={segment.text} onOpen={onOpen} typeId={segment.typeId} />;
          case "url":
            return (
              <a className={linkClass} href={segment.href} key={index} rel="noopener noreferrer" target="_blank">
                {segment.text}
              </a>
            );
          case "break":
            return <br key={index} />;
        }
      })}
    </>
  );
}
