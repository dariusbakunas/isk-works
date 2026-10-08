import { describe, expect, test } from "vitest";

import { computeSankey } from "../sankey-layout";

const row = (category: string, total: string) => ({ category, total, previous: null });

describe("computeSankey", () => {
  test("returns nothing when there is no money to show", () => {
    expect(computeSankey([], [])).toBeNull();
    expect(computeSankey([row("Ships", "0")], [row("Materials", "0")])).toBeNull();
  });

  test("income above spending ends in a Net saved node that balances the right column", () => {
    const layout = computeSankey([row("Ships", "800"), row("Modules", "200")], [row("Materials", "300")])!;

    expect(layout.left.map((node) => node.label)).toEqual(["Ships", "Modules"]);
    expect(layout.right.map((node) => node.label)).toEqual(["Materials", "Net saved"]);
    const net = layout.right.find((node) => node.kind === "net")!;
    expect(net.value).toBe(700);
    expect(layout.total).toBe(1000);
  });

  test("spending above income adds a Drawdown source instead of a negative saving", () => {
    const layout = computeSankey([row("Ships", "300")], [row("Materials", "1000")])!;

    expect(layout.left.map((node) => node.label)).toEqual(["Ships", "Drawdown"]);
    expect(layout.left.find((node) => node.kind === "drawdown")!.value).toBe(700);
    expect(layout.right.map((node) => node.kind)).toEqual(["category"]);
    expect(layout.right.some((node) => node.kind === "net")).toBe(false);
  });

  test("equal income and spending needs neither balancing node", () => {
    const layout = computeSankey([row("Ships", "500")], [row("Materials", "500")])!;
    expect(layout.left).toHaveLength(1);
    expect(layout.right).toHaveLength(1);
  });

  test("node heights are proportional to value and stack top to bottom with gaps", () => {
    const layout = computeSankey([row("A", "750"), row("B", "250")], [row("C", "1000")], { height: 202, gap: 2 })!;
    const [a, b] = layout.left;
    // 202 - one gap of 2 leaves 200 of usable height for a total of 1000.
    expect(a.h).toBeCloseTo(150);
    expect(b.h).toBeCloseTo(50);
    expect(a.y).toBe(0);
    expect(b.y).toBeCloseTo(152);
  });

  test("tiny nodes keep a visible minimum height", () => {
    const layout = computeSankey([row("Big", "1000000"), row("Tiny", "1")], [row("X", "1000001")])!;
    expect(layout.left[1].h).toBeGreaterThanOrEqual(3);
  });

  test("ribbons conserve value: each column's wallet slices tile the whole bar", () => {
    const layout = computeSankey([row("Ships", "800"), row("Modules", "200")], [row("Materials", "300")], { height: 260 })!;
    const sliceTotal = (links: typeof layout.leftLinks) => links.reduce((sum, link) => sum + link.targetH, 0);
    expect(sliceTotal(layout.leftLinks)).toBeCloseTo(260);
    const rightTotal = layout.rightLinks.reduce((sum, link) => sum + link.sourceH, 0);
    expect(rightTotal).toBeCloseTo(260);
    // The first ribbon starts at the top of the wallet bar.
    expect(layout.leftLinks[0].targetY).toBe(0);
    expect(layout.leftLinks[1].targetY).toBeCloseTo(layout.leftLinks[0].targetH);
  });

  test("ribbon paths are closed cubic curves between the columns", () => {
    const layout = computeSankey([row("Ships", "500")], [row("Materials", "500")])!;
    const path = layout.leftLinks[0].path;
    expect(path.startsWith("M")).toBe(true);
    expect(path.endsWith("Z")).toBe(true);
    expect(path.match(/C/g)).toHaveLength(2);
  });

  test("zero and negative rows are dropped", () => {
    const layout = computeSankey([row("Ships", "500"), row("Dust", "0")], [row("Materials", "500"), row("Refund", "-5")])!;
    expect(layout.left.map((node) => node.label)).toEqual(["Ships"]);
    expect(layout.right.map((node) => node.label)).toEqual(["Materials"]);
  });

  test("only spending: the whole flow is a drawdown", () => {
    const layout = computeSankey([], [row("Materials", "400")])!;
    expect(layout.left.map((node) => node.kind)).toEqual(["drawdown"]);
    expect(layout.total).toBe(400);
  });
});
