import type { FitDetail, FitDraft, FitItemView, ModuleState, SlotKind } from "../../api/fittings";

/** A fit being edited: what the editor shows and saves. */
export interface Draft {
  name: string;
  description: string;
  items: FitItemView[];
}

/** A type the browser hands the editor. */
export interface PickedType {
  typeId: number;
  name: string;
}

/** How many slots each rack has, from the ship. */
export interface RackSizes {
  high: number;
  med: number;
  low: number;
  rig: number;
}

type Rack = keyof RackSizes;
const RACKS: Rack[] = ["high", "med", "low", "rig"];
const STATES: ModuleState[] = ["offline", "online", "active", "overload"];

export function draftFrom(fit: FitDetail): Draft {
  return { name: fit.name, description: fit.description, items: fit.items };
}

function moduleItem(type: PickedType, slotKind: SlotKind, slotIndex: number): FitItemView {
  return {
    typeId: type.typeId,
    slotKind,
    slotIndex,
    quantity: 1,
    // Asked for active; the next calculation brings a passive module down
    // to online (`capStates`).
    state: "active",
    chargeTypeId: null,
    name: type.name,
    groupName: null,
    chargeName: null,
  };
}

const isRack = (kind: SlotKind): kind is Rack => (RACKS as SlotKind[]).includes(kind);

function freeSlots(draft: Draft, kind: Rack, sizes: RackSizes): number[] {
  const taken = new Set(draft.items.filter((item) => item.slotKind === kind).map((item) => item.slotIndex));
  return Array.from({ length: sizes[kind] }, (_, index) => index).filter((index) => !taken.has(index));
}

/** Puts `type` in the first free slot of `kind`; a full rack takes nothing. */
export function addToFreeSlot(draft: Draft, type: PickedType, kind: SlotKind, sizes: RackSizes) {
  const free = isRack(kind) ? freeSlots(draft, kind, sizes)[0] : undefined;
  if (free === undefined) return { draft, placed: false };
  return { draft: { ...draft, items: [...draft.items, moduleItem(type, kind, free)] }, placed: true };
}

/** Puts `type` at one slot, replacing whatever is there. */
export function placeAt(draft: Draft, type: PickedType, kind: SlotKind, slotIndex: number): Draft {
  const items = draft.items.filter((item) => !(item.slotKind === kind && item.slotIndex === slotIndex));
  return { ...draft, items: [...items, moduleItem(type, kind, slotIndex)] };
}

/** Puts `type` in every empty slot of `kind`. */
export function fillRack(draft: Draft, type: PickedType, kind: SlotKind, sizes: RackSizes): Draft {
  if (!isRack(kind)) return draft;
  const added = freeSlots(draft, kind, sizes).map((index) => moduleItem(type, kind, index));
  return added.length ? { ...draft, items: [...draft.items, ...added] } : draft;
}

function update(draft: Draft, position: number, change: (item: FitItemView) => FitItemView): Draft {
  return { ...draft, items: draft.items.map((item, index) => (index === position ? change(item) : item)) };
}

export function removeItem(draft: Draft, position: number): Draft {
  return { ...draft, items: draft.items.filter((_, index) => index !== position) };
}

export function setState(draft: Draft, position: number, state: ModuleState): Draft {
  return update(draft, position, (item) => ({ ...item, state }));
}

/** The next state a click asks for, wrapping to offline past what the
 * module can reach. */
export function cycleState(state: ModuleState, maxState: ModuleState): ModuleState {
  const next = STATES.indexOf(state) + 1;
  return next > STATES.indexOf(maxState) ? "offline" : STATES[next];
}

export function setCharge(draft: Draft, position: number, charge: PickedType | null): Draft {
  return update(draft, position, (item) => ({
    ...item,
    chargeTypeId: charge?.typeId ?? null,
    chargeName: charge?.name ?? null,
  }));
}

/** Swaps the item for another type (a variation), keeping its slot,
 * state and charge. */
export function replaceType(draft: Draft, position: number, type: PickedType): Draft {
  return update(draft, position, (item) => ({ ...item, typeId: type.typeId, name: type.name, groupName: null }));
}

/** Drones (launched) and cargo stack by type. */
export function addStack(draft: Draft, type: PickedType, kind: "drone_bay" | "cargo", quantity: number): Draft {
  const position = draft.items.findIndex((item) => item.slotKind === kind && item.typeId === type.typeId);
  if (position >= 0) {
    return update(draft, position, (item) => ({ ...item, quantity: item.quantity + quantity }));
  }
  return {
    ...draft,
    items: [
      ...draft.items,
      {
        typeId: type.typeId,
        slotKind: kind,
        slotIndex: null,
        quantity,
        state: kind === "drone_bay" ? "active" : "offline",
        chargeTypeId: null,
        name: type.name,
        groupName: null,
        chargeName: null,
      },
    ],
  };
}

export function setQuantity(draft: Draft, position: number, quantity: number): Draft {
  return update(draft, position, (item) => ({ ...item, quantity: Math.max(1, Math.floor(quantity)) }));
}

/** Lowers asked-for states the last calculation says a module can't
 * reach; the same draft when nothing changes. */
export function capStates(draft: Draft, itemStats: Array<{ maxState: ModuleState } | null>): Draft {
  let changed = false;
  const items = draft.items.map((item, index) => {
    const max = itemStats[index]?.maxState;
    if (max && STATES.indexOf(item.state) > STATES.indexOf(max)) {
      changed = true;
      return { ...item, state: max };
    }
    return item;
  });
  return changed ? { ...draft, items } : draft;
}

/** A slot (racks) or a type (stacks): what one change is about. */
const keyOf = (item: FitItemView) =>
  item.slotIndex === null ? `${item.slotKind}:type:${item.typeId}` : `${item.slotKind}:${item.slotIndex}`;

const signature = (item: FitItemView) =>
  [item.typeId, item.state, item.quantity, item.chargeTypeId ?? ""].join("|");

/** How many slots, stacks and fields differ from the saved fit. */
export function changeCount(saved: Draft, draft: Draft): number {
  const before = new Map(saved.items.map((item) => [keyOf(item), signature(item)]));
  const after = new Map(draft.items.map((item) => [keyOf(item), signature(item)]));
  let count = 0;
  for (const key of new Set([...before.keys(), ...after.keys()])) {
    if (before.get(key) !== after.get(key)) count += 1;
  }
  if (saved.name !== draft.name) count += 1;
  if (saved.description !== draft.description) count += 1;
  return count;
}

/** What a save (or a calculation) sends: the items without their names. */
export function toFitDraft(draft: Draft, shipTypeId: number): FitDraft {
  return {
    name: draft.name,
    description: draft.description,
    shipTypeId,
    items: draft.items.map(({ typeId, slotKind, slotIndex, quantity, state, chargeTypeId, mutation }) => ({
      typeId,
      slotKind,
      slotIndex,
      quantity,
      state,
      chargeTypeId,
      ...(mutation ? { mutation } : {}),
    })),
  };
}
