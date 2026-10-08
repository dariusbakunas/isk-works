import { describe, expect, it } from "vitest";
import { parseStructureReference } from "../structure-reference";

describe("parseStructureReference", () => {
  it("parses a pasted EVE client Show Info link", () => {
    expect(
      parseStructureReference("<url=showinfo:35825//1030000000001>X-7OMU - Example Raitaru</url>"),
    ).toBe(1_030_000_000_001);
  });

  it("parses a link copied with its chat sender prefix", () => {
    expect(
      parseStructureReference(
        "Valka > <url=showinfo:35825//1030000000001>X-7OMU - Example Raitaru</url>",
      ),
    ).toBe(1_030_000_000_001);
  });

  it("parses a bare numeric structure ID", () => {
    expect(parseStructureReference("1030000000001")).toBe(1_030_000_000_001);
    expect(parseStructureReference("  1030000000001  ")).toBe(1_030_000_000_001);
  });

  it("rejects free text, empty input, and non-positive numbers", () => {
    expect(parseStructureReference("")).toBeNull();
    expect(parseStructureReference("   ")).toBeNull();
    expect(parseStructureReference("Raitaru")).toBeNull();
    expect(parseStructureReference("0")).toBeNull();
    expect(parseStructureReference("-5")).toBeNull();
  });
});
