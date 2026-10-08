// The single deterministic "old graph node -> new graph node" identity
// resolver, keyed by the slot a node fills (`<parentBuildId>:<componentTypeId>`
// via `slotIndex`). Used to carry interaction state (selection today,
// collapse if it ever applies) across a graph refresh -- notably the
// `buy:<parent>:<type>` -> `build:<linkedId>` transition when a linked
// Build is persisted.
//
// No fuzzy matching: identity is `parentBuildId + componentTypeId`, never
// item/type name.

import type { BuildGraphFlow } from "./to-react-flow";

/**
 * Where `oldId` lives in `nextFlow`:
 *   - the same id, if it still exists (root ids and unchanged `build:` ids);
 *   - the id now filling the same parent/component slot (`buy:` -> `build:`);
 *   - `null` if that slot is gone entirely.
 */
export function resolveGraphNodeIdentity(
  oldId: string,
  previousSlotIndex: ReadonlyMap<string, string>,
  nextFlow: Pick<BuildGraphFlow, "nodes" | "slotIndex">,
): string | null {
  if (nextFlow.nodes.some((node) => node.id === oldId)) return oldId;
  const slotKey = [...previousSlotIndex.entries()].find(([, id]) => id === oldId)?.[0];
  if (slotKey === undefined) return null;
  return nextFlow.slotIndex.get(slotKey) ?? null;
}
