import { describe, expect, it } from "vitest";

import { resolveGraphNodeIdentity } from "../graph-slot-identity";
import { toReactFlow } from "../to-react-flow";

import {
  ROOT_BUILD_ID,
  productionChild,
  productionNode,
  projection,
  unresolvedBuildChild,
} from "./fixtures";

const ROOT = `root:${ROOT_BUILD_ID}`;

function rootWithUnresolved() {
  return projection(
    productionNode({
      graphNodeId: ROOT,
      buildId: ROOT_BUILD_ID,
      typeId: 500,
      kind: "rootManufacturing",
      children: [
        unresolvedBuildChild({
          graphNodeId: `buy:${ROOT_BUILD_ID}:900`,
          parentBuildId: ROOT_BUILD_ID,
          typeId: 900,
        }),
      ],
    }),
  );
}

function rootWithProduction(linkedId = "linked-1") {
  return projection(
    productionNode({
      graphNodeId: ROOT,
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
      ],
    }),
  );
}

describe("resolveGraphNodeIdentity", () => {
  it("returns the same id when the node still exists", () => {
    const previous = toReactFlow(rootWithUnresolved());
    const next = toReactFlow(rootWithUnresolved());
    expect(resolveGraphNodeIdentity(ROOT, previous.slotIndex, next)).toBe(ROOT);
  });

  it("follows a buy: slot to the build: node that now fills it", () => {
    const previous = toReactFlow(rootWithUnresolved());
    const next = toReactFlow(rootWithProduction("linked-1"));
    expect(
      resolveGraphNodeIdentity(`buy:${ROOT_BUILD_ID}:900`, previous.slotIndex, next),
    ).toBe("build:linked-1");
  });

  it("returns null when the slot is gone entirely", () => {
    const previous = toReactFlow(rootWithUnresolved());
    const next = toReactFlow(
      projection(
        productionNode({
          graphNodeId: ROOT,
          buildId: ROOT_BUILD_ID,
          typeId: 500,
          kind: "rootManufacturing",
        }),
      ),
    );
    expect(
      resolveGraphNodeIdentity(`buy:${ROOT_BUILD_ID}:900`, previous.slotIndex, next),
    ).toBeNull();
  });

  it("never matches by item/type name -- only parentBuildId + componentTypeId", () => {
    const previous = toReactFlow(rootWithUnresolved());
    // Next graph has a production node for a *different* component (901),
    // same type name would be irrelevant anyway.
    const next = toReactFlow(rootWithProduction("linked-1"));
    // The 900 slot maps; a made-up 901 slot does not.
    expect(resolveGraphNodeIdentity(`buy:${ROOT_BUILD_ID}:901`, previous.slotIndex, next)).toBeNull();
  });
});
