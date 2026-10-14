import { useEffect, useRef } from "react";

/** The tinted background on the Board card whose details the inspector is
 * showing (see `.iw-card-active` in styles.css). */
export const ACTIVE_CARD_CLASS = "iw-card-active";

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
