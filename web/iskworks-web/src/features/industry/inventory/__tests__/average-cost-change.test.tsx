import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { AverageCostChange } from "../inventory-posting-panel";

describe("AverageCostChange", () => {
  test("a dearer purchase shows the exact increase, percent, and prior average", () => {
    render(<AverageCostChange current="8751820.4800" resulting="18167880.3200" />);
    expect(screen.getByTestId("average-cost-change")).toHaveTextContent("Increases by+9,416,059.84 ISK (+107.6%)");
    expect(screen.getByTestId("average-cost-change")).toHaveClass("text-warning");
    expect(screen.getByText("from 8,751,820.48 ISK")).toBeInTheDocument();
  });

  test("a cheaper purchase shows a decrease", () => {
    render(<AverageCostChange current="1000.0000" resulting="750.5000" />);
    expect(screen.getByTestId("average-cost-change")).toHaveTextContent("Decreases by-249.5 ISK (-25%)");
    expect(screen.getByTestId("average-cost-change")).toHaveClass("text-positive");
  });

  test("an unchanged average, a first purchase, and emptied stock say so plainly", () => {
    const { rerender } = render(<AverageCostChange current="700.0000" resulting="700.0000" />);
    expect(screen.getByText("No change")).toBeInTheDocument();
    rerender(<AverageCostChange current={null} resulting="700.0000" />);
    expect(screen.getByText("No existing stock")).toBeInTheDocument();
    rerender(<AverageCostChange current="700.0000" resulting={null} />);
    expect(screen.getByText("No remaining stock")).toBeInTheDocument();
  });
});
