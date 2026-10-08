import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useGraphNodeDetail } from "../use-graph-node-detail";
import { toReactFlow, type BuildGraphNode } from "../to-react-flow";

import {
  ROOT_BUILD_ID,
  acquisitionChild,
  buildDetail,
  productionChild,
  productionNode,
  projection,
  unresolvedBuildChild,
} from "./fixtures";

const getBuild = vi.fn();
vi.mock("../../../../../api/industry", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getBuild: (...args: unknown[]) => getBuild(...args),
}));

beforeEach(() => getBuild.mockReset());
afterEach(() => vi.restoreAllMocks());

/** A projection: root -> linked production `build:<id>` (+ optionally an
 * actionable Buy and an unresolved Build), so we can pull real
 * `BuildGraphNode`s of every type out of `toReactFlow`. */
function graph(linkedId = "child-1") {
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
            typeId: 900,
            parentBuildId: ROOT_BUILD_ID,
            parentComponentTypeId: 900,
          }),
          ROOT_BUILD_ID,
          900,
        ),
        acquisitionChild({
          graphNodeId: `buy:${ROOT_BUILD_ID}:34`,
          parentBuildId: ROOT_BUILD_ID,
          typeId: 34,
        }),
        unresolvedBuildChild({
          graphNodeId: `buy:${ROOT_BUILD_ID}:77`,
          parentBuildId: ROOT_BUILD_ID,
          typeId: 77,
        }),
      ],
    }),
  );
}

function nodesOf(linkedId = "child-1") {
  const byId = new Map(
    toReactFlow(graph(linkedId)).nodes.map((node) => [node.id, node as BuildGraphNode]),
  );
  return {
    root: byId.get(`root:${ROOT_BUILD_ID}`)!,
    production: byId.get(`build:${linkedId}`)!,
    acquisition: byId.get(`buy:${ROOT_BUILD_ID}:34`)!,
    unresolvedBuild: byId.get(`buy:${ROOT_BUILD_ID}:77`)!,
  };
}

const N = nodesOf();

describe("useGraphNodeDetail — request behaviour", () => {
  it("selecting a linked production node issues exactly one getBuild(node.buildId)", async () => {
    getBuild.mockResolvedValue(buildDetail({ id: "child-1" }));
    const { result } = renderHook(() =>
      useGraphNodeDetail({ active: true, selectedNode: N.production }),
    );
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    expect(getBuild).toHaveBeenCalledTimes(1);
    expect(getBuild).toHaveBeenCalledWith("child-1", expect.any(AbortSignal));
  });

  it("selecting the root node issues zero getBuild calls", async () => {
    renderHook(() => useGraphNodeDetail({ active: true, selectedNode: N.root }));
    await new Promise((r) => setTimeout(r, 20));
    expect(getBuild).not.toHaveBeenCalled();
  });

  it("selecting an actionable Buy node issues zero getBuild calls", async () => {
    renderHook(() => useGraphNodeDetail({ active: true, selectedNode: N.acquisition }));
    await new Promise((r) => setTimeout(r, 20));
    expect(getBuild).not.toHaveBeenCalled();
  });

  it("selecting an unresolved Build node issues zero getBuild calls", async () => {
    renderHook(() => useGraphNodeDetail({ active: true, selectedNode: N.unresolvedBuild }));
    await new Promise((r) => setTimeout(r, 20));
    expect(getBuild).not.toHaveBeenCalled();
  });

  it("no selection issues zero getBuild calls", async () => {
    renderHook(() => useGraphNodeDetail({ active: true, selectedNode: undefined }));
    await new Promise((r) => setTimeout(r, 20));
    expect(getBuild).not.toHaveBeenCalled();
  });

  it("an inactive Graph issues zero getBuild calls even with a linked node selected", async () => {
    renderHook(() => useGraphNodeDetail({ active: false, selectedNode: N.production }));
    await new Promise((r) => setTimeout(r, 20));
    expect(getBuild).not.toHaveBeenCalled();
  });
});

describe("useGraphNodeDetail — cache", () => {
  it("re-selecting the same linked node (deselect, reselect) fetches only once", async () => {
    getBuild.mockResolvedValue(buildDetail({ id: "child-1" }));
    const { result, rerender } = renderHook((props) => useGraphNodeDetail(props), {
      initialProps: { active: true, selectedNode: N.production as BuildGraphNode | undefined },
    });
    await waitFor(() => expect(result.current.detail).not.toBeNull());

    rerender({ active: true, selectedNode: undefined });
    await waitFor(() => expect(result.current.detail).toBeNull());

    rerender({ active: true, selectedNode: N.production });
    await waitFor(() => expect(result.current.detail).not.toBeNull());

    expect(getBuild).toHaveBeenCalledTimes(1);
  });

  it("keeps the cached detail across Graph -> Worksheet -> Graph (component stays mounted)", async () => {
    getBuild.mockResolvedValue(buildDetail({ id: "child-1" }));
    const { result, rerender } = renderHook((props) => useGraphNodeDetail(props), {
      initialProps: { active: true, selectedNode: N.production as BuildGraphNode | undefined },
    });
    await waitFor(() => expect(result.current.detail).not.toBeNull());

    rerender({ active: false, selectedNode: N.production }); // -> Worksheet
    await waitFor(() => expect(result.current.detail).toBeNull());

    rerender({ active: true, selectedNode: N.production }); // -> Graph again
    await waitFor(() => expect(result.current.detail).not.toBeNull());

    expect(getBuild).toHaveBeenCalledTimes(1);
  });
});

describe("useGraphNodeDetail — race safety", () => {
  it("a late response for a since-changed selection never overwrites the current one", async () => {
    let resolveA: (v: unknown) => void = () => {};
    let resolveB: (v: unknown) => void = () => {};
    getBuild
      .mockImplementationOnce(() => new Promise((r) => (resolveA = r)))
      .mockImplementationOnce(() => new Promise((r) => (resolveB = r)));

    const A = nodesOf("A").production;
    const B = nodesOf("B").production;
    const { result, rerender } = renderHook((props) => useGraphNodeDetail(props), {
      initialProps: { active: true, selectedNode: A as BuildGraphNode | undefined },
    });
    await waitFor(() => expect(getBuild).toHaveBeenCalledTimes(1));

    rerender({ active: true, selectedNode: B });
    await waitFor(() => expect(getBuild).toHaveBeenCalledTimes(2));

    await act(async () => {
      resolveB(buildDetail({ id: "B", name: "B build" }));
    });
    expect(result.current.detail?.id).toBe("B");

    await act(async () => {
      resolveA(buildDetail({ id: "A", name: "A build" }));
    });
    expect(result.current.detail?.id).toBe("B");
  });

  it("aborts the in-flight request when the Graph goes inactive and ignores its late result", async () => {
    let resolveA: (v: unknown) => void = () => {};
    getBuild.mockImplementationOnce(() => new Promise((r) => (resolveA = r)));

    const { result, rerender } = renderHook((props) => useGraphNodeDetail(props), {
      initialProps: { active: true, selectedNode: N.production as BuildGraphNode | undefined },
    });
    await waitFor(() => expect(getBuild).toHaveBeenCalledTimes(1));
    const signal = getBuild.mock.calls[0][1] as AbortSignal;

    rerender({ active: false, selectedNode: N.production });
    expect(signal.aborted).toBe(true);

    await act(async () => {
      resolveA(buildDetail({ id: "child-1" }));
    });
    expect(result.current.detail).toBeNull();
  });

  it("resolves an unresolved-Build slot to a real linked node with exactly one request", async () => {
    getBuild.mockResolvedValue(buildDetail({ id: "linked-900" }));
    const { result, rerender } = renderHook((props) => useGraphNodeDetail(props), {
      initialProps: {
        active: true,
        selectedNode: N.unresolvedBuild as BuildGraphNode | undefined,
      },
    });
    await new Promise((r) => setTimeout(r, 20));
    expect(getBuild).not.toHaveBeenCalled();

    rerender({ active: true, selectedNode: nodesOf("linked-900").production });
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    expect(getBuild).toHaveBeenCalledTimes(1);
    expect(getBuild).toHaveBeenCalledWith("linked-900", expect.any(AbortSignal));
  });
});

describe("useGraphNodeDetail — error + retry", () => {
  it("surfaces a local error, keeps detail null, and retry re-fetches then clears the error", async () => {
    getBuild.mockRejectedValueOnce(new Error("boom"));
    const { result } = renderHook(() =>
      useGraphNodeDetail({ active: true, selectedNode: N.production }),
    );
    await waitFor(() => expect(result.current.error).toBe("boom"));
    expect(result.current.detail).toBeNull();
    expect(result.current.loading).toBe(false);

    getBuild.mockResolvedValueOnce(buildDetail({ id: "child-1" }));
    act(() => result.current.retry());
    await waitFor(() => expect(result.current.detail).not.toBeNull());
    expect(result.current.error).toBeNull();
    expect(getBuild).toHaveBeenCalledTimes(2);
  });
});
