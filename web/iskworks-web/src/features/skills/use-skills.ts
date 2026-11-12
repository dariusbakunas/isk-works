import { getCharacter, listCharacters, type CharacterDetail, type CharacterRosterEntry } from "../../api/characters";
import { getSkillCatalogue, type SkillCatalogue } from "../../api/skills";
import { useLoad, type Loaded } from "../../hooks/use-load";

export type { Loaded };

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
