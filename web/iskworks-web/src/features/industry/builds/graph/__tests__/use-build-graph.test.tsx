import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useBuildGraph } from "../use-build-graph";

import {
  ROOT_BUILD_ID,
  branchingProjection,
  productionChild,
  productionNode,
  projection,
  unresolvedBuildChild,
} from "./fixtures";

const previewBuildGraph = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  previewBuildGraph: (...args: unknown[]) => previewBuildGraph(...args),
}));

const OVERLAY = {
  recipe: { mode: "manufacturing", blueprintTypeId: 6830 },
  runs: 2,
  componentResolutions: [],
};
const PREVIEW_KEY = JSON.stringify(OVERLAY);

function rootOnly() {
  return projection(
    productionNode({
      graphNodeId: `root:${ROOT_BUILD_ID}`,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      kind: "rootManufacturing",
    }),
  );
}

function rootWithUnresolved(typeId = 900) {
  return projection(
    productionNode({
      graphNodeId: `root:${ROOT_BUILD_ID}`,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      kind: "rootManufacturing",
      children: [
        unresolvedBuildChild({
          graphNodeId: `buy:${ROOT_BUILD_ID}:${typeId}`,
          parentBuildId: ROOT_BUILD_ID,
          typeId,
        }),
      ],
    }),
  );
}

function rootWithProduction(typeId = 900, linkedId = "linked-1") {
  return projection(
    productionNode({
      graphNodeId: `root:${ROOT_BUILD_ID}`,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      kind: "rootManufacturing",
      children: [
        productionChild(
          productionNode({
            graphNodeId: `build:${linkedId}`,
            buildId: linkedId,
            typeId,
            parentBuildId: ROOT_BUILD_ID,
            parentComponentTypeId: typeId,
          }),
          ROOT_BUILD_ID,
          typeId,
        ),
      ],
    }),
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  previewBuildGraph.mockReset();
});
afterEach(() => {
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

async function flush() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(300);
  });
}

describe("useBuildGraph", () => {
  it("fetches with the live planning overlay when active", async () => {
    previewBuildGraph.mockResolvedValue(rootOnly());
    renderHook(() =>
      useBuildGraph({
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {},
      }),
    );
    await flush();
    expect(previewBuildGraph).toHaveBeenCalledWith(
      ROOT_BUILD_ID,
      OVERLAY,
      expect.any(AbortSignal),
    );
  });

  it("does not fetch while the Graph view is inactive", async () => {
    previewBuildGraph.mockResolvedValue(rootOnly());
    renderHook(() =>
      useBuildGraph({
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: false,
        linkedBuildsByTypeId: {},
      }),
    );
    await flush();
    expect(previewBuildGraph).not.toHaveBeenCalled();
  });

  it("refetches once per settled planning change and never polls", async () => {
    previewBuildGraph.mockResolvedValue(rootOnly());
    const { rerender } = renderHook((props) => useBuildGraph(props), {
      initialProps: {
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {},
      },
    });
    await flush();
    expect(previewBuildGraph).toHaveBeenCalledTimes(1);

    // Idle time passing must not trigger more requests.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(60_000);
    });
    expect(previewBuildGraph).toHaveBeenCalledTimes(1);

    // A settled editor change (new previewKey) triggers exactly one more.
    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: JSON.stringify({ ...OVERLAY, runs: 5 }),
      active: true,
      linkedBuildsByTypeId: {},
    });
    await flush();
    expect(previewBuildGraph).toHaveBeenCalledTimes(2);
  });

  it("refetches when a linked build resolves, turning unresolved into production", async () => {
    previewBuildGraph.mockResolvedValueOnce(rootWithUnresolved(900));
    const { result, rerender } = renderHook((props) => useBuildGraph(props), {
      initialProps: {
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {} as Record<number, { id: string }>,
      },
    });
    await flush();
    expect(result.current.flow.positioned.map((node) => node.id)).toContain(
      `buy:${ROOT_BUILD_ID}:900`,
    );

    previewBuildGraph.mockResolvedValueOnce(rootWithProduction(900, "linked-1"));
    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: PREVIEW_KEY,
      active: true,
      linkedBuildsByTypeId: { 900: { id: "linked-1" } },
    });
    await flush();
    expect(previewBuildGraph).toHaveBeenCalledTimes(2);
    expect(result.current.flow.positioned.map((node) => node.id)).toContain("build:linked-1");
    expect(result.current.flow.positioned.map((node) => node.id)).not.toContain(
      `buy:${ROOT_BUILD_ID}:900`,
    );
  });

  it("keeps the last good projection when a later refresh fails", async () => {
    previewBuildGraph.mockResolvedValueOnce(rootOnly());
    const { result, rerender } = renderHook((props) => useBuildGraph(props), {
      initialProps: {
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {},
      },
    });
    await flush();
    expect(result.current.projection).not.toBeNull();

    previewBuildGraph.mockRejectedValueOnce(new Error("boom"));
    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: JSON.stringify({ ...OVERLAY, runs: 9 }),
      active: true,
      linkedBuildsByTypeId: {},
    });
    await flush();
    expect(result.current.projection).not.toBeNull();
    expect(result.current.refreshError).toBeTruthy();
    expect(result.current.hardError).toBeNull();
  });

  it("reports a hard error when the first load fails with nothing to keep", async () => {
    previewBuildGraph.mockRejectedValueOnce(new Error("nope"));
    const { result } = renderHook(() =>
      useBuildGraph({
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {},
      }),
    );
    await flush();
    expect(result.current.projection).toBeNull();
    expect(result.current.hardError).toBeTruthy();
  });

  it("keeps a selection whose node id is unchanged across a refresh", async () => {
    previewBuildGraph.mockResolvedValue(rootOnly());
    const { result, rerender } = renderHook((props) => useBuildGraph(props), {
      initialProps: {
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {},
      },
    });
    await flush();
    act(() => result.current.setSelectedGraphNodeId(`root:${ROOT_BUILD_ID}`));

    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: JSON.stringify({ ...OVERLAY, runs: 3 }),
      active: true,
      linkedBuildsByTypeId: {},
    });
    await flush();
    expect(result.current.selectedGraphNodeId).toBe(`root:${ROOT_BUILD_ID}`);
  });

  it("remaps a buy: selection to the build: node that fills the same slot", async () => {
    previewBuildGraph.mockResolvedValueOnce(rootWithUnresolved(900));
    const { result, rerender } = renderHook((props) => useBuildGraph(props), {
      initialProps: {
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {} as Record<number, { id: string }>,
      },
    });
    await flush();
    act(() => result.current.setSelectedGraphNodeId(`buy:${ROOT_BUILD_ID}:900`));

    previewBuildGraph.mockResolvedValueOnce(rootWithProduction(900, "linked-1"));
    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: PREVIEW_KEY,
      active: true,
      linkedBuildsByTypeId: { 900: { id: "linked-1" } },
    });
    await flush();
    expect(result.current.selectedGraphNodeId).toBe("build:linked-1");
  });

  it("clears a selection whose slot disappears entirely", async () => {
    previewBuildGraph.mockResolvedValueOnce(rootWithUnresolved(900));
    const { result, rerender } = renderHook((props) => useBuildGraph(props), {
      initialProps: {
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {},
      },
    });
    await flush();
    act(() => result.current.setSelectedGraphNodeId(`buy:${ROOT_BUILD_ID}:900`));

    previewBuildGraph.mockResolvedValueOnce(rootOnly());
    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: JSON.stringify({ ...OVERLAY, runs: 4 }),
      active: true,
      linkedBuildsByTypeId: {},
    });
    await flush();
    expect(result.current.selectedGraphNodeId).toBeNull();
  });

  // ---- collapse ------------------------------------------------------

  function branchingHook() {
    previewBuildGraph.mockResolvedValue(branchingProjection());
    return renderHook((props) => useBuildGraph(props), {
      initialProps: {
        buildId: ROOT_BUILD_ID,
        previewKey: PREVIEW_KEY,
        active: true,
        linkedBuildsByTypeId: {} as Record<number, { id: string }>,
      },
    });
  }

  it("non-root nodes with children start collapsed; toggleCollapse expands one", async () => {
    const { result } = branchingHook();
    await flush();

    // Default: build:A is collapsed -> B / C / its acquisition child hidden.
    expect([...result.current.collapsedNodeIds]).toEqual(["build:A"]);
    let visible = result.current.flow.positioned.map((n) => n.id);
    expect(visible).toContain("build:A");
    expect(visible).not.toContain("build:B");
    expect(result.current.flow.hiddenCountByNodeId.get("build:A")).toBe(3);
    // complete topology still has everything.
    expect(result.current.flow.complete.nodes.map((n) => n.id)).toContain("build:B");

    act(() => result.current.toggleCollapse("build:A"));
    visible = result.current.flow.positioned.map((n) => n.id);
    expect(visible).toContain("build:B");
    expect(visible).toContain("build:C");
    expect([...result.current.collapsedNodeIds]).toEqual([]);
  });

  it("re-collapsing a node moves a now-hidden selection up to it", async () => {
    const { result } = branchingHook();
    await flush();
    act(() => result.current.toggleCollapse("build:A")); // expand first
    act(() => result.current.setSelectedGraphNodeId("build:B"));
    act(() => result.current.toggleCollapse("build:A")); // collapse again
    expect(result.current.selectedGraphNodeId).toBe("build:A");
  });

  it("keeps a selected collapsed node selected when it is expanded", async () => {
    const { result } = branchingHook();
    await flush();
    act(() => result.current.setSelectedGraphNodeId("build:A"));
    act(() => result.current.toggleCollapse("build:A"));
    expect(result.current.selectedGraphNodeId).toBe("build:A");
  });

  it("an explicit expansion persists across an identical graph refresh", async () => {
    const { result, rerender } = branchingHook();
    await flush();
    act(() => result.current.toggleCollapse("build:A")); // now explicitly expanded

    previewBuildGraph.mockResolvedValue(branchingProjection());
    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: JSON.stringify({ ...OVERLAY, runs: 3 }),
      active: true,
      linkedBuildsByTypeId: {},
    });
    await flush();
    // build:A stays expanded across the refresh -> not in the collapsed set.
    expect([...result.current.collapsedNodeIds]).toEqual([]);
  });

  it("an unrelated graph change keeps an expansion; a disappeared node is pruned", async () => {
    const { result, rerender } = branchingHook();
    await flush();
    act(() => result.current.toggleCollapse("build:A")); // expanded

    // Next graph drops the D branch entirely; A is unchanged.
    previewBuildGraph.mockResolvedValueOnce(
      projection(
        productionNode({
          graphNodeId: `root:${ROOT_BUILD_ID}`,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
          children: [
            productionChild(
              productionNode({
                graphNodeId: "build:A",
                buildId: "A",
                typeId: 200,
                parentBuildId: ROOT_BUILD_ID,
                parentComponentTypeId: 200,
                children: [
                  productionChild(
                    productionNode({
                      graphNodeId: "build:B",
                      buildId: "B",
                      typeId: 201,
                      parentBuildId: "A",
                      parentComponentTypeId: 201,
                    }),
                    "A",
                    201,
                  ),
                ],
              }),
              ROOT_BUILD_ID,
              200,
            ),
          ],
        }),
      ),
    );
    rerender({
      buildId: ROOT_BUILD_ID,
      previewKey: JSON.stringify({ ...OVERLAY, runs: 9 }),
      active: true,
      linkedBuildsByTypeId: {},
    });
    await flush();
    // build:A survives and stays explicitly expanded (so not collapsed).
    expect([...result.current.collapsedNodeIds]).toEqual([]);
    expect(result.current.flow.positioned.map((n) => n.id)).toContain("build:B");
  });
});
