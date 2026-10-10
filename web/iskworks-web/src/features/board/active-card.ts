import { useEffect, useRef } from "react";

/** The ring on the Board card whose details the inspector is showing. */
export const ACTIVE_CARD_CLASS = "ring-2 ring-primary";

/** Scrolls the card into view when it becomes the inspected one -- e.g.
 * opened from an Epic's ticket list or a `?ticket=` link -- so the
 * highlight isn't off screen. */
export function useScrollIntoViewWhenActive<T extends HTMLElement>(active: boolean) {
  const ref = useRef<T>(null);
  useEffect(() => {
    // jsdom has no scrollIntoView.
    if (active) ref.current?.scrollIntoView?.({ block: "nearest" });
  }, [active]);
  return ref;
}
