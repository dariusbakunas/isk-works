import { render, screen } from "@testing-library/react";
import { describe, expect, test } from "vitest";

import { IskCompact } from "../money";

describe("IskCompact", () => {
  test("shows the compact value with the exact value on hover", () => {
    render(<IskCompact value="1234567.89" />);
    const cell = screen.getByText("1.2M");
    expect(cell).toHaveAttribute("title", "1,234,567.89 ISK");
    expect(cell).toHaveClass("whitespace-nowrap", "font-mono", "tabular-nums");
  });

  test("keeps a caller's class and title", () => {
    render(<IskCompact className="text-lg" title="Per unit" value="12.5" />);
    const cell = screen.getByText("12.5");
    expect(cell).toHaveAttribute("title", "Per unit");
    expect(cell).toHaveClass("text-lg", "font-mono");
  });
});
