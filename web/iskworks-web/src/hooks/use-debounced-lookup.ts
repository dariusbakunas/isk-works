import { useEffect, useRef, useState } from "react";

const MIN_QUERY_LENGTH = 2;
const DEBOUNCE_MS = 250;

export interface DebouncedLookupState<T> {
  results: T[];
  searching: boolean;
}

export interface DebouncedLookupOptions {
  /** Minimum trimmed query length before searching. Defaults to 2. */
  minLength?: number;
  /**
   * When false, clears results and skips searching entirely regardless of
   * query length -- for callers that only want to search while e.g. a
   * dropdown is open or nothing is already selected.
   */
  enabled?: boolean;
}

export function useDebouncedLookup<T>(
  query: string,
  search: (query: string) => Promise<T[]>,
  onError: (error: unknown) => void,
  { minLength = MIN_QUERY_LENGTH, enabled = true }: DebouncedLookupOptions = {},
): DebouncedLookupState<T> {
  const [results, setResults] = useState<T[]>([]);
  const [searching, setSearching] = useState(false);
  const searchRef = useRef(search);
  const onErrorRef = useRef(onError);
  const sequenceRef = useRef(0);
  searchRef.current = search;
  onErrorRef.current = onError;

  useEffect(() => {
    const normalized = query.trim();
    const sequence = ++sequenceRef.current;
    setSearching(false);

    if (!enabled || normalized.length < minLength) {
      setResults([]);
      return;
    }

    const timer = window.setTimeout(() => {
      setSearching(true);
      void searchRef.current(normalized)
        .then((nextResults) => {
          if (sequence !== sequenceRef.current) return;
          setResults(nextResults);
        })
        .catch((error) => {
          if (sequence !== sequenceRef.current) return;
          setResults([]);
          onErrorRef.current(error);
        })
        .finally(() => {
          if (sequence === sequenceRef.current) setSearching(false);
        });
    }, DEBOUNCE_MS);

    return () => {
      window.clearTimeout(timer);
      if (sequence === sequenceRef.current) sequenceRef.current += 1;
    };
  }, [query, minLength, enabled]);

  return { results, searching };
}
