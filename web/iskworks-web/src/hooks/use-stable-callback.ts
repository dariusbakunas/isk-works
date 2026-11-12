import { useCallback, useLayoutEffect, useRef } from "react";

/** A function that always calls the latest `callback` but keeps one
 * identity, so an effect can call it without listing it as a dependency
 * that changes every render. React 18's stand-in for `useEffectEvent`:
 * call it from effects and event handlers, not during render. */
export function useStableCallback<Args extends unknown[], Result>(
  callback: (...args: Args) => Result,
): (...args: Args) => Result {
  const latest = useRef(callback);
  useLayoutEffect(() => {
    latest.current = callback;
  });
  return useCallback((...args: Args) => latest.current(...args), []);
}
