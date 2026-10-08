const SHOW_INFO_PATTERN = /showinfo:\d+\/\/(\d+)/;

/**
 * Parses a structure ID out of pasted input: either a full EVE client
 * "Show Info" link (produced by shift-dragging or right-click > Copy on a
 * structure into chat, mail, or notes -- the standard in-game way to get a
 * structure's numeric ID, since it's not shown anywhere directly),
 * e.g. `<url=showinfo:35825//1030000000001>Structure Name</url>`, or a bare
 * numeric structure ID. Returns null if neither pattern matches.
 */
export function parseStructureReference(input: string): number | null {
  const trimmed = input.trim();
  if (trimmed === "") return null;

  const linkMatch = SHOW_INFO_PATTERN.exec(trimmed);
  if (linkMatch) {
    const id = Number(linkMatch[1]);
    return Number.isSafeInteger(id) && id > 0 ? id : null;
  }

  if (/^\d+$/.test(trimmed)) {
    const id = Number(trimmed);
    return Number.isSafeInteger(id) && id > 0 ? id : null;
  }

  return null;
}
