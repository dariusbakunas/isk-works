import { describe, expect, test } from "vitest";

import type { FitItemView } from "../../../api/fittings";
import {
  addStack,
  addToFreeSlot,
  capStates,
  changeCount,
  draftFrom,
  fillRack,
  placeAt,
  removeItem,
  replaceType,
  setCharge,
  setQuantity,
  setState,
  toggleDrones,
  toggleState,
  toFitDraft,
} from "../fit-draft";
import { fitDetail, item } from "./fixtures";

const dcu = { typeId: 2048, name: "Damage Control II" };
const gun = { typeId: 2873, name: "200mm AutoCannon II" };
const racks = { high: 3, med: 3, low: 3, rig: 2 };

const slots = (items: FitItemView[], kind: string) =>
  items.filter((entry) => entry.slotKind === kind).map((entry) => [entry.slotIndex, entry.name]);

describe("the fit draft", () => {
  test("a module goes in the first free slot of its rack, active until the engine says otherwise", () => {
    const draft = draftFrom(fitDetail());
    // The fixture's low rack has slot 0 taken.
    const result = addToFreeSlot(draft, dcu, "low", racks);
    expect(result.placed).toBe(true);
    expect(slots(result.draft.items, "low")).toEqual([
      [0, "Damage Control II"],
      [1, "Damage Control II"],
    ]);
    expect(result.draft.items.at(-1)?.state).toBe("active");
  });

  test("a full rack takes nothing more", () => {
    const draft = draftFrom({ ...fitDetail(), items: [item({ slotIndex: 0 }), item({ slotIndex: 1 }), item({ slotIndex: 2 })] });
    const result = addToFreeSlot(draft, dcu, "low", racks);
    expect(result.placed).toBe(false);
    expect(result.draft).toBe(draft);
  });

  test("placing on a taken slot replaces what's there", () => {
    const draft = placeAt(draftFrom(fitDetail()), gun, "low", 0);
    expect(slots(draft.items, "low")).toEqual([[0, "200mm AutoCannon II"]]);
  });

  test("filling a rack puts the module in every empty slot", () => {
    const draft = fillRack(draftFrom(fitDetail()), gun, "high", racks);
    // High slot 0 holds the fixture's gun; 1 and 2 are filled.
    expect(slots(draft.items, "high").map(([index]) => index)).toEqual([0, 1, 2]);
  });

  test("removing, replacing and loading work on one item", () => {
    let draft = draftFrom(fitDetail());
    const gunIndex = draft.items.findIndex((entry) => entry.typeId === 2873);
    draft = setCharge(draft, gunIndex, { typeId: 186, name: "Phased Plasma S" });
    expect(draft.items[gunIndex]).toMatchObject({ chargeTypeId: 186, chargeName: "Phased Plasma S" });
    draft = setCharge(draft, gunIndex, null);
    expect(draft.items[gunIndex]).toMatchObject({ chargeTypeId: null, chargeName: null });

    draft = replaceType(draft, gunIndex, { typeId: 2881, name: "200mm AutoCannon I" });
    expect(draft.items[gunIndex]).toMatchObject({ typeId: 2881, name: "200mm AutoCannon I", slotKind: "high", slotIndex: 0 });

    const before = draft.items.length;
    draft = removeItem(draft, gunIndex);
    expect(draft.items).toHaveLength(before - 1);
    expect(draft.items.some((entry) => entry.typeId === 2881)).toBe(false);
  });

  test("a click turns a module on or off: active ↔ online, or online ↔ offline for a passive one", () => {
    expect(toggleState("active", "overload", "toggle")).toBe("online");
    expect(toggleState("overload", "overload", "toggle")).toBe("online");
    expect(toggleState("online", "overload", "toggle")).toBe("active");
    expect(toggleState("offline", "overload", "toggle")).toBe("active");
    expect(toggleState("active", "active", "toggle")).toBe("online");
    expect(toggleState("online", "online", "toggle")).toBe("offline");
    expect(toggleState("offline", "online", "toggle")).toBe("online");

    const draft = setState(draftFrom(fitDetail()), 0, "offline");
    expect(draft.items[0].state).toBe("offline");
  });

  test("drones launch or go back to the bay, whatever the click", () => {
    expect(toggleDrones("active")).toBe("offline");
    expect(toggleDrones("offline")).toBe("active");
    // Online means nothing to a drone: it's in the bay.
    expect(toggleDrones("online")).toBe("active");
  });

  test("shift overheats a module that can, and alt takes it offline", () => {
    expect(toggleState("active", "overload", "overload")).toBe("overload");
    expect(toggleState("online", "overload", "overload")).toBe("overload");
    expect(toggleState("overload", "overload", "overload")).toBe("active");
    // Nothing to overheat: a plain toggle.
    expect(toggleState("active", "active", "overload")).toBe("online");

    expect(toggleState("active", "overload", "offline")).toBe("offline");
    expect(toggleState("offline", "overload", "offline")).toBe("online");
  });

  test("after a calculation, asked-for states above what a module reaches come down to its most", () => {
    const draft = draftFrom(fitDetail());
    const capped = capStates(draft, [{ maxState: "online" }, null, { maxState: "overload" }]);
    // Item 0 is online already; item 1 has no stats; item 2 stays active.
    expect(capped).toBe(draft);
    const raised = setState(draft, 0, "active");
    expect(capStates(raised, [{ maxState: "online" }]).items[0].state).toBe("online");
  });

  test("drones and cargo stack by type", () => {
    let draft = draftFrom(fitDetail());
    draft = addStack(draft, { typeId: 2456, name: "Hobgoblin II" }, "drone_bay", 2);
    expect(draft.items.filter((entry) => entry.slotKind === "drone_bay")).toEqual([
      expect.objectContaining({ typeId: 2456, quantity: 7, state: "active" }),
    ]);
    draft = addStack(draft, { typeId: 34, name: "Tritanium" }, "cargo", 1000);
    expect(draft.items.at(-1)).toMatchObject({ slotKind: "cargo", slotIndex: null, quantity: 1000, state: "offline" });

    const stack = draft.items.findIndex((entry) => entry.typeId === 34);
    expect(setQuantity(draft, stack, 250).items[stack].quantity).toBe(250);
    expect(setQuantity(draft, stack, 0).items[stack].quantity).toBe(1);
  });

  test("changes count what differs from the saved fit, and the draft saves without names", () => {
    const saved = draftFrom(fitDetail());
    expect(changeCount(saved, saved)).toBe(0);
    let draft = removeItem(saved, 0);
    draft = addToFreeSlot(draft, dcu, "low", racks).draft;
    draft = { ...draft, name: "Renamed" };
    expect(changeCount(saved, draft)).toBe(2);

    const body = toFitDraft(draft, 12731);
    expect(body.name).toBe("Renamed");
    expect(body.shipTypeId).toBe(12731);
    expect(body.items[0]).toEqual({
      typeId: 5975,
      slotKind: "med",
      slotIndex: 0,
      quantity: 1,
      state: "active",
      chargeTypeId: null,
    });
  });
});
