import { useCallback, useState } from "react";

// Per-browser convenience, like the market scope recents: an unscoped key.
const STORAGE_KEY = "iskworks:nav-pinned";

function readPinned(): boolean {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function writePinned(pinned: boolean): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, String(pinned));
  } catch {
    // Storage unavailable (private browsing, blocked site data) -- the pin
    // still works for this page load, it just isn't remembered.
  }
}

/** Whether the navigation rail is pinned open, remembered per browser. */
export function useNavPinned(): [pinned: boolean, togglePinned: () => void] {
  const [pinned, setPinned] = useState(readPinned);

  const togglePinned = useCallback(() => {
    setPinned((current) => {
      writePinned(!current);
      return !current;
    });
  }, []);

  return [pinned, togglePinned];
}
