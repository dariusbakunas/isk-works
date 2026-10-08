export type EveTypeImageVariation = "icon" | "render" | "bp" | "bpc" | "relic";

export function eveTypeImageUrl(
  typeId: number,
  variation: EveTypeImageVariation = "icon",
  size = 64,
): string {
  return `https://images.evetech.net/types/${typeId}/${variation}?size=${size}`;
}

export function eveCharacterPortraitUrl(characterId: number, size = 64): string {
  return `https://images.evetech.net/characters/${characterId}/portrait?size=${size}`;
}
