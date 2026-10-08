import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});

// React Flow (@xyflow/react) measures its container via ResizeObserver,
// which jsdom does not implement. A no-op keeps the canvas mountable in
// tests; layout maths are covered by the pure `layoutBuildGraph` tests.
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

if (typeof globalThis.DOMMatrixReadOnly === "undefined") {
  globalThis.DOMMatrixReadOnly = class {
    m22 = 1;
    constructor() {}
  } as unknown as typeof DOMMatrixReadOnly;
}
