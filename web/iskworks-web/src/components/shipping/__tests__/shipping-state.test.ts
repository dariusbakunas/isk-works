import { expect, test } from "vitest";

import {
  applyResolved,
  emptyShippingState,
  mergeShipping,
  parseHold,
  parseItemList,
  planShipments,
  shipmentText,
  shippingStateWith,
  type ShippingItem,
} from "../shipping-state";

const item = (typeId: number, name: string, quantity: number, unitVolumeM3: string | null): ShippingItem => ({
  typeId,
  name,
  quantity,
  unitVolumeM3,
});

test("reads the list formats people paste", () => {
  const lines = parseItemList(
    [
      "Tritanium 1,000",
      "Pyerite x250",
      "5 x Datacore - Mechanical Engineering",
      "3 × Attainment Decryptor",
      "Rifter",
      "",
      "Mexallon\t1 500\tMineral\t\t15 m3",
      "Hulk\t\tExhumer",
      "Zydrine 2 000",
    ].join("\n"),
  );
  expect(lines.map((line) => [line.name, line.quantity])).toEqual([
    ["Tritanium", 1000],
    ["Pyerite", 250],
    ["Datacore - Mechanical Engineering", 5],
    ["Attainment Decryptor", 3],
    ["Rifter", 1],
    ["Mexallon", 1500],
    ["Hulk", 1],
    ["Zydrine", 2000],
  ]);
});

test("a name that ends in a number still resolves as one of that item", () => {
  const [line] = parseItemList("Hull Upgrades 5");
  expect(line).toEqual({ name: "Hull Upgrades", quantity: 5, alt: "Hull Upgrades 5" });
  const state = applyResolved(emptyShippingState(), [line], [{ typeId: 9, typeName: "Hull Upgrades 5", packagedVolumeM3: "0.01" }]);
  expect(state.items).toEqual([item(9, "Hull Upgrades 5", 1, "0.01")]);
});

test("resolved lines add to what is listed, and unknown names are kept to show", () => {
  const start = { ...emptyShippingState(), items: [item(34, "Tritanium", 100, "0.01")] };
  const lines = parseItemList("tritanium 50\nNot An Item 3\nRifter");
  const state = applyResolved({ ...start, pending: lines }, lines, [
    { typeId: 34, typeName: "Tritanium", packagedVolumeM3: "0.01" },
    { typeId: 587, typeName: "Rifter", packagedVolumeM3: "2500" },
  ]);
  expect(state.items).toEqual([item(34, "Tritanium", 150, "0.01"), item(587, "Rifter", 1, "2500")]);
  expect(state.unmatched).toEqual(["Not An Item 3"]);
  expect(state.pending).toEqual([]);
});

test("opening with more items queues them behind what the window has", () => {
  const current = { ...emptyShippingState(), holdM3: "5000", items: [item(34, "Tritanium", 1, "0.01")] };
  const merged = mergeShipping(current, shippingStateWith([{ name: "Pyerite", quantity: 2 }]));
  expect(merged.holdM3).toBe("5000");
  expect(merged.items).toEqual(current.items);
  expect(merged.pending).toEqual([{ name: "Pyerite", quantity: 2 }]);
});

test("reads a typed hold size", () => {
  expect(parseHold("62,500")).toBe(62_500_000_000);
  expect(parseHold(" 400.5 ")).toBe(400_500_000);
  expect(parseHold("")).toBeNull();
  expect(parseHold("0")).toBeNull();
  expect(parseHold("abc")).toBeNull();
});

test("a load that fits is one shipment", () => {
  const plan = planShipments([item(34, "Tritanium", 1000, "0.01"), item(35, "Pyerite", 500, "0.01")], parseHold("100"));
  expect(plan.totalMicro).toBe(15_000_000);
  expect(plan.shipments).toHaveLength(1);
  expect(plan.shipments[0].usedMicro).toBe(15_000_000);
});

test("splits stacks across shipments to fill each hold", () => {
  // 25 m³ of Tritanium and two 4 m³ crates into a 10 m³ hold: 33 m³, 4 trips.
  const plan = planShipments([item(34, "Tritanium", 2500, "0.01"), item(1, "Crate", 2, "4")], parseHold("10"));
  expect(plan.shipments).toHaveLength(4);
  expect(plan.shipments[0].lines).toEqual([
    { typeId: 1, name: "Crate", quantity: 2, volumeMicro: 8_000_000 },
    { typeId: 34, name: "Tritanium", quantity: 200, volumeMicro: 2_000_000 },
  ]);
  expect(plan.shipments.map((shipment) => shipment.usedMicro)).toEqual([10_000_000, 10_000_000, 10_000_000, 3_000_000]);
  const shipped = plan.shipments.flatMap((shipment) => shipment.lines).filter((line) => line.typeId === 34);
  expect(shipped.reduce((sum, line) => sum + line.quantity, 0)).toBe(2500);
});

test("leaves out what cannot be shipped and says why", () => {
  const plan = planShipments(
    [item(1, "Freighter", 1, "1300000"), item(2, "Mystery", 4, null), item(34, "Tritanium", 100, "0.01"), item(3, "Nothing", 0, "5")],
    parseHold("1000"),
  );
  expect(plan.oversized.map((entry) => entry.name)).toEqual(["Freighter"]);
  expect(plan.unknown.map((entry) => entry.name)).toEqual(["Mystery"]);
  expect(plan.shipments).toHaveLength(1);
  expect(plan.shipments[0].lines.map((line) => line.name)).toEqual(["Tritanium"]);
});

test("without a hold there is a total but no shipments", () => {
  const plan = planShipments([item(34, "Tritanium", 100, "0.01")], null);
  expect(plan.totalMicro).toBe(1_000_000);
  expect(plan.shipments).toEqual([]);
});

test("refuses to list an absurd number of shipments", () => {
  const plan = planShipments([item(34, "Tritanium", 1_000_000, "1")], parseHold("1"));
  expect(plan.shipments).toEqual([]);
  expect(plan.tooMany).toBe(1_000_000);
});

test("a shipment copies as a multibuy list", () => {
  const plan = planShipments([item(34, "Tritanium", 100, "0.01"), item(35, "Pyerite", 5, "0.01")], parseHold("100"));
  expect(shipmentText(plan.shipments[0])).toBe("Pyerite 5\nTritanium 100");
});
