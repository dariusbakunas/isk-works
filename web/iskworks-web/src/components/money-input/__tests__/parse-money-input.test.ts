import { describe, expect, it } from "vitest";

import { formatMoneyInputDisplay, MONEY_INPUT_MESSAGES, parseMoneyInput } from "../parse-money-input";

describe("parseMoneyInput -- while typing (final: false)", () => {
  it.each([
    ["", "empty"],
    ["   ", "empty"],
  ])("classifies %j as empty", (raw, status) => {
    expect(parseMoneyInput(raw)).toEqual({ status });
  });

  it.each([
    ["0", "0"],
    ["0.25", "0.25"],
    ["1000", "1000"],
    ["1000000", "1000000"],
    ["1,000", "1000"],
    ["1,000,000", "1000000"],
    ["1234.56", "1234.56"],
    ["1,234.56", "1234.56"],
    ["1234.5678", "1234.5678"],
    ["1,234.5678", "1234.5678"],
    ["1,234,567.89", "1234567.89"],
    ["  1,000  ", "1000"],
    ["007", "7"],
    ["00", "0"],
    ["1234", "1234"],
    [".5", "0.5"],
    ["0.0000", "0.0000"],
    ["1.50000", "1.50000"],
  ])("accepts %j and canonicalizes to %j", (raw, canonical) => {
    expect(parseMoneyInput(raw)).toEqual({ status: "valid", canonical });
  });

  it.each([
    ["."],
    ["0."],
    ["1000."],
    ["1,000."],
    ["1,234."],
    ["1,"],
    ["1,0"],
    ["1,00"],
    ["12,"],
    ["123,"],
    ["100,"],
    ["1,000,"],
    ["1,234,56"],
  ])("treats %j as incomplete (no commit, no error)", (raw) => {
    expect(parseMoneyInput(raw)).toEqual({ status: "incomplete" });
  });

  it.each([
    ["-1", MONEY_INPUT_MESSAGES.negative],
    ["-100", MONEY_INPUT_MESSAGES.negative],
    ["-1,000", MONEY_INPUT_MESSAGES.negative],
    ["abc", MONEY_INPUT_MESSAGES.characters],
    ["1a", MONEY_INPUT_MESSAGES.characters],
    ["$1,000", MONEY_INPUT_MESSAGES.characters],
    ["1 000", MONEY_INPUT_MESSAGES.characters],
    ["1_000", MONEY_INPUT_MESSAGES.characters],
    ["1e5", MONEY_INPUT_MESSAGES.characters],
    ["1..2", MONEY_INPUT_MESSAGES.decimalPoint],
    ["1.2.3", MONEY_INPUT_MESSAGES.decimalPoint],
    ["1,234.56789", MONEY_INPUT_MESSAGES.decimals],
    ["1.234567", MONEY_INPUT_MESSAGES.decimals],
    ["1,00,000", MONEY_INPUT_MESSAGES.grouping],
    ["1,0000", MONEY_INPUT_MESSAGES.grouping],
    ["12345,678", MONEY_INPUT_MESSAGES.grouping],
    [",100", MONEY_INPUT_MESSAGES.grouping],
    ["1,,000", MONEY_INPUT_MESSAGES.grouping],
  ])("rejects %j locally with a message", (raw, message) => {
    expect(parseMoneyInput(raw)).toEqual({ status: "invalid", message });
  });
});

describe("parseMoneyInput -- on blur / Enter (final: true)", () => {
  it.each([
    ["1000.", "1000"],
    ["1,000.", "1000"],
    ["1,234.", "1234"],
    ["0.", "0"],
    ["1,000", "1000"],
    ["1,234.56", "1234.56"],
    [".5", "0.5"],
  ])("finalizes %j to %j", (raw, canonical) => {
    expect(parseMoneyInput(raw, { final: true })).toEqual({ status: "valid", canonical });
  });

  it.each([
    ["1,"],
    ["1,0"],
    ["1,00"],
    ["1,000,"],
    ["100,"],
    ["1,234,56"],
  ])("rejects unfinished grouping %j on finalize", (raw) => {
    expect(parseMoneyInput(raw, { final: true })).toEqual({
      status: "invalid",
      message: MONEY_INPUT_MESSAGES.grouping,
    });
  });

  it.each([["."], ["0"], [""]])("leaves %j resolvable-or-empty without an error", (raw) => {
    const result = parseMoneyInput(raw, { final: true });
    expect(result.status === "empty" || result.status === "valid" || result.status === "incomplete").toBe(true);
    expect(result.message).toBeUndefined();
  });

  it.each([
    ["-1", MONEY_INPUT_MESSAGES.negative],
    ["1,234.56789", MONEY_INPUT_MESSAGES.decimals],
  ])("still rejects %j on finalize", (raw, message) => {
    expect(parseMoneyInput(raw, { final: true })).toEqual({ status: "invalid", message });
  });
});

describe("parseMoneyInput -- tightened edge cases", () => {
  // incomplete === "a valid monetary string is still reachable by appending
  // characters". Anything a suffix can never repair is invalid immediately.
  it.each([
    "1,,",
    "12,,",
    "100,,",
    "1,000,,",
    ",,",
    "1,,0",
    "1,,000",
    "1,234,,",
    "12,34,",
    "1,2,3",
    ",1",
    "1234,",
  ])("rejects unrepairable grouping %j immediately", (raw) => {
    expect(parseMoneyInput(raw)).toEqual({
      status: "invalid",
      message: MONEY_INPUT_MESSAGES.grouping,
    });
  });

  it.each([
    "1,",
    "1,0",
    "1,00",
    "12,",
    "100,",
    "1,000,",
    "1,234,5",
    "1,234,56",
    "12,34",
  ])("keeps repairable grouping prefix %j incomplete while focused", (raw) => {
    expect(parseMoneyInput(raw)).toEqual({ status: "incomplete" });
    // property: some appended suffix makes it valid
    const repaired = ["0", "00", "000", "5", "56", "567"].some(
      (suffix) => parseMoneyInput(raw + suffix).status === "valid",
    );
    expect(repaired).toBe(true);
  });

  it("uses non-negative wording for the negative message", () => {
    expect(MONEY_INPUT_MESSAGES.negative).toBe("Enter zero or a positive amount.");
    expect(parseMoneyInput("0")).toEqual({ status: "valid", canonical: "0" });
    expect(parseMoneyInput("-1")).toEqual({
      status: "invalid",
      message: "Enter zero or a positive amount.",
    });
  });

  it.each([
    ["0.", "0"],
    ["1.", "1"],
    ["1000.", "1000"],
    ["1,000.", "1000"],
  ])("finalizes trailing decimal %j to %j", (raw, canonical) => {
    expect(parseMoneyInput(raw)).toEqual({ status: "incomplete" });
    expect(parseMoneyInput(raw, { final: true })).toEqual({ status: "valid", canonical });
  });

  it("keeps a lone dot incomplete even on finalize", () => {
    expect(parseMoneyInput(".")).toEqual({ status: "incomplete" });
    expect(parseMoneyInput(".", { final: true })).toEqual({ status: "incomplete" });
  });

  it.each(["1,00.", "1,00.5", "12,3.4", "1,000,.5"])(
    "rejects a half-typed group frozen by a decimal point: %j",
    (raw) => {
      expect(parseMoneyInput(raw)).toEqual({
        status: "invalid",
        message: MONEY_INPUT_MESSAGES.grouping,
      });
    },
  );
});

describe("formatMoneyInputDisplay", () => {
  it.each([
    ["", ""],
    ["0", "0"],
    ["1000", "1,000"],
    ["1000000", "1,000,000"],
    ["1234.56", "1,234.56"],
    ["1000000.0000", "1,000,000"],
    ["1234.5600", "1,234.56"],
    ["1234.5000", "1,234.5"],
  ])("re-groups %j as %j and drops insignificant zeros", (canonical, display) => {
    expect(formatMoneyInputDisplay(canonical)).toBe(display);
  });

  it("round-trips through the parser", () => {
    for (const canonical of ["0", "1000", "1000000", "1234.56", "1234.5678"]) {
      const display = formatMoneyInputDisplay(canonical);
      expect(parseMoneyInput(display)).toEqual({ status: "valid", canonical });
    }
  });
});
