export interface RecentProduct {
  typeId: number;
  name: string;
}

const STORAGE_KEY = "iskworks.invention.recent";
const LIMIT = 3;

/** The last few quoted products, newest first. Empty when storage is
 * unavailable or holds something else. */
export function readRecentProducts(): RecentProduct[] {
  try {
    const stored: unknown = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "[]");
    if (!Array.isArray(stored)) return [];
    return stored
      .filter((entry): entry is RecentProduct =>
        typeof entry === "object" && entry !== null
        && Number.isSafeInteger((entry as RecentProduct).typeId)
        && typeof (entry as RecentProduct).name === "string")
      .slice(0, LIMIT);
  } catch {
    return [];
  }
}

/** Puts `product` first and returns the new list. */
export function rememberProduct(product: RecentProduct): RecentProduct[] {
  const next = [product, ...readRecentProducts().filter((entry) => entry.typeId !== product.typeId)].slice(0, LIMIT);
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
  } catch {
    // Storage is a convenience; the page works without it.
  }
  return next;
}

const CHARACTER_KEY = "iskworks.invention.character";

/** The character last chosen on the page, which becomes its default. */
export function readPreferredCharacter(): string | null {
  try {
    return window.localStorage.getItem(CHARACTER_KEY);
  } catch {
    return null;
  }
}

export function rememberPreferredCharacter(connectionId: string): string {
  try {
    window.localStorage.setItem(CHARACTER_KEY, connectionId);
  } catch {
    // Storage is a convenience; the page works without it.
  }
  return connectionId;
}
