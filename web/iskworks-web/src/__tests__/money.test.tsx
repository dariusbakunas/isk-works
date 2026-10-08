import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import {
  formatIskCompact,
  formatIskSummary,
  formatSignedIsk,
  MoneyAmount,
  splitExactDecimal,
} from "../components/money";

describe("ISK formatting", () => {
  it("formats compact worksheet values without a repeated currency suffix", () => {
    expect(formatIskCompact("987.12")).toBe("987.12");
    expect(formatIskCompact("35120.0000")).toBe("35.1K");
    expect(formatIskCompact("31102080.0000")).toBe("31.1M");
    expect(formatIskCompact("3420000000.0000")).toBe("3.4B");
  });

  it.each([
    ["0.0000", "0 ISK"],
    ["0.0049", "0 ISK"],
    ["0.0050", "0.01 ISK"],
    ["1.0000", "1 ISK"],
    ["1.1000", "1.1 ISK"],
    ["1.1050", "1.11 ISK"],
    ["245080.0000", "245,080 ISK"],
    ["245080.5000", "245,080.5 ISK"],
    ["245080.2500", "245,080.25 ISK"],
    ["245080.1250", "245,080.13 ISK"],
    ["-4735.0000", "-4,735 ISK"],
    ["000500000.0000", "500,000 ISK"],
    ["999999999999999999.995", "1,000,000,000,000,000,000 ISK"],
  ])("formats %s without floating-point conversion", (value, expected) => {
    expect(formatIskSummary(value)).toBe(expected);
  });

  it("formats signed comparisons without adding a plus to zero", () => {
    expect(formatSignedIsk("4735.0000")).toBe("+4,735 ISK");
    expect(formatSignedIsk("-12465.0000")).toBe("-12,465 ISK");
    expect(formatSignedIsk("0.0000")).toBe("0 ISK");
  });

  it("rejects invalid decimals", () => {
    expect(() => formatIskSummary("12 ISK")).toThrow("Invalid decimal value");
    expect(() => formatIskSummary("1e6")).toThrow("Invalid decimal value");
  });

  it("preserves exact scale and structured detail parts", () => {
    expect(splitExactDecimal("+000245080.1250", { mode: "detail" })).toMatchObject({
      integerPart: "245,080",
      fractionalPart: "1250",
      exactText: "245,080.1250 ISK",
      displayText: "245,080.1250 ISK",
      rounded: false,
    });
  });
});

describe("MoneyAmount", () => {
  it("renders summary values and exposes exact rounded values", () => {
    render(<MoneyAmount className="custom-money" data-testid="money" value="245080.1250" />);
    const amount = screen.getByTestId("money");
    expect(amount).toHaveTextContent("245,080.13 ISK");
    expect(amount).toHaveAttribute("title", "Exact: 245,080.1250 ISK");
    expect(amount).toHaveClass("custom-money");
  });

  it("does not add a tooltip when trimming zeros is exact", () => {
    render(<MoneyAmount data-testid="money" value="245080.0000" />);
    expect(screen.getByTestId("money")).not.toHaveAttribute("title");
  });

  it("renders detail fractions separately with one accessible value", () => {
    const { container } = render(<MoneyAmount mode="detail" value="245080.1250" />);
    expect(screen.getByLabelText("245,080.1250 ISK")).toBeInTheDocument();
    expect(container.querySelector(".iw-money-fraction")).toHaveTextContent(".1250");
  });

  it("renders exact values, signed values, and hidden currency", () => {
    const { rerender } = render(<MoneyAmount mode="exact" value="1.2300" />);
    expect(screen.getByLabelText("1.2300 ISK")).toBeInTheDocument();
    rerender(<MoneyAmount showCurrency={false} signDisplay="always" value="4735.0000" />);
    expect(screen.getByText("+4,735")).toBeInTheDocument();
  });
});
