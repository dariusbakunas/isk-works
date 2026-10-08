import { describe, expect, it } from "vitest";

import { jobCount, jobSplitLabel } from "../job-split";

describe("job split", () => {
  it("splits runs into one job per copy's licensed runs", () => {
    expect(jobCount(4, 1)).toBe(4);
    expect(jobCount(5, 2)).toBe(3);
    expect(jobCount(3, 10)).toBe(1);
  });

  it("labels only a real split", () => {
    expect(jobSplitLabel(4, 1)).toBe("4 jobs");
    expect(jobSplitLabel(3, 10)).toBeNull();
    expect(jobSplitLabel(4, null)).toBeNull();
    expect(jobSplitLabel(4, 0)).toBeNull();
  });
});
