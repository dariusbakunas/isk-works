import { useCallback, useEffect, useState } from "react";

import { getCharacter, listCharacters, type CharacterDetail, type CharacterRosterEntry } from "../../api/characters";
import { getSkillCatalogue, type SkillCatalogue } from "../../api/skills";

export interface Loaded<T> {
  /** The latest successful payload; kept while a refetch runs. */
  data: T | null;
  loading: boolean;
  error: Error | null;
  retry: () => void;
}

function useLoad<T>(load: ((signal: AbortSignal) => Promise<T>) | null, key: string): Loaded<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(load !== null);
  const [error, setError] = useState<Error | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (load === null) {
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    setLoading(true);
    setError(null);
    load(controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        setData(payload);
        setLoading(false);
      })
      .catch((reason: unknown) => {
        if (controller.signal.aborted) return;
        setError(reason instanceof Error ? reason : new Error("Skills request failed."));
        setLoading(false);
      });
    return () => controller.abort();
    // `load` is rebuilt every render; `key` is its identity.
  }, [key, attempt]);

  const retry = useCallback(() => setAttempt((current) => current + 1), []);
  return { data, loading, error, retry };
}

export function useCharacters(): Loaded<CharacterRosterEntry[]> {
  return useLoad(() => listCharacters(), "characters");
}

/** The catalogue, overlaid with `characterId`'s skills. `undefined` waits
 * (the character list hasn't loaded); `null` is the bare catalogue. */
export function useSkillCatalogue(characterId: string | null | undefined): Loaded<SkillCatalogue> {
  return useLoad(
    characterId === undefined ? null : (signal) => getSkillCatalogue(characterId, signal),
    `catalogue:${characterId === undefined ? "pending" : (characterId ?? "")}`,
  );
}

/** The character detail that feeds the training queue rail. */
export function useCharacterDetail(characterId: string | null): Loaded<CharacterDetail> {
  return useLoad(characterId ? () => getCharacter(characterId) : null, `detail:${characterId ?? ""}`);
}
