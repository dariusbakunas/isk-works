import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vitest";

import {
  OperationalTable,
  OperationalTableGroup,
  OperationalTableRow,
  type OperationalColumn,
} from "..";

const columns: OperationalColumn[] = [
  { key: "item", label: "Item", width: "minmax(180px,1fr)", sticky: true },
  { key: "required", label: "Required", width: "96px", align: "right" },
];

function fixture(onSelectRow = vi.fn(), selectedRowKey: string | null = null) {
  return {
    onSelectRow,
    view: (
      <OperationalTable
        ariaLabel="Production worksheet"
        columns={columns}
        onSelectRow={onSelectRow}
        selectedRowKey={selectedRowKey}
      >
        <OperationalTableGroup groupKey="minerals" itemCount={2} label="Minerals">
          <OperationalTableRow rowKey="material:34" cells={{ item: "Tritanium", required: "28,800" }} />
          <OperationalTableRow rowKey="material:35" cells={{ item: "Pyerite", required: "5,400" }} />
        </OperationalTableGroup>
      </OperationalTable>
    ),
  };
}

describe("OperationalTable", () => {
  test("renders semantic rows, selects a row, and collapses a group", async () => {
    const user = userEvent.setup();
    const { onSelectRow, view } = fixture();
    render(view);

    expect(screen.getByRole("table", { name: "Production worksheet" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Required" })).toBeInTheDocument();
    await user.click(screen.getByRole("row", { name: /Tritanium/ }));
    expect(onSelectRow).toHaveBeenCalledWith("material:34");

    await user.click(screen.getByRole("button", { name: "Collapse Minerals" }));
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Expand Minerals" })).toHaveAttribute("aria-expanded", "false");
  });

  test("moves row focus with arrow keys and selects with Enter", async () => {
    const user = userEvent.setup();
    const { onSelectRow, view } = fixture();
    render(view);

    const tritanium = screen.getByRole("row", { name: /Tritanium/ });
    const pyerite = screen.getByRole("row", { name: /Pyerite/ });
    tritanium.focus();
    await user.keyboard("{ArrowDown}");
    expect(pyerite).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(onSelectRow).toHaveBeenCalledWith("material:35");
  });

  test("uses the same selected background for every cell", () => {
    render(fixture(vi.fn(), "material:34").view);

    const cells = screen.getByRole("row", { name: /Tritanium/ }).querySelectorAll("td");
    expect(cells).toHaveLength(2);
    cells.forEach((cell) => expect(cell).toHaveClass("bg-primary/10"));
  });

  test("supports a collapsed uncontrolled default and group summary", async () => {
    const user = userEvent.setup();
    render(
      <OperationalTable
        ariaLabel="Inventory"
        columns={columns}
        onSelectRow={vi.fn()}
        selectedRowKey={null}
      >
        <OperationalTableGroup
          defaultExpanded={false}
          groupKey="minerals"
          itemCount={1}
          label="Minerals"
          summary="0.01 m³ · 4.20 ISK"
        >
          <OperationalTableRow rowKey="34" cells={{ item: "Tritanium", required: "1" }} />
        </OperationalTableGroup>
      </OperationalTable>,
    );

    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
    expect(screen.getByText("0.01 m³ · 4.20 ISK")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand Minerals" }));
    expect(screen.getByRole("row", { name: /Tritanium/ })).toBeInTheDocument();
  });

  test("reports controlled expansion changes without changing itself", async () => {
    const user = userEvent.setup();
    const onExpandedChange = vi.fn();
    render(
      <OperationalTable
        ariaLabel="Inventory"
        columns={columns}
        onSelectRow={vi.fn()}
        selectedRowKey={null}
      >
        <OperationalTableGroup
          expanded={false}
          groupKey="minerals"
          itemCount={1}
          label="Minerals"
          onExpandedChange={onExpandedChange}
        >
          <OperationalTableRow rowKey="34" cells={{ item: "Tritanium", required: "1" }} />
        </OperationalTableGroup>
      </OperationalTable>,
    );

    await user.click(screen.getByRole("button", { name: "Expand Minerals" }));
    expect(onExpandedChange).toHaveBeenCalledWith(true);
    expect(screen.queryByRole("row", { name: /Tritanium/ })).not.toBeInTheDocument();
  });

  test("a column with sort renders a clickable header with an aria-sort state", async () => {
    const user = userEvent.setup();
    const onToggle = vi.fn();
    const sortableColumns: OperationalColumn[] = [
      columns[0],
      { ...columns[1], sort: { direction: "desc", onToggle } },
    ];
    render(
      <OperationalTable ariaLabel="Sortable" columns={sortableColumns} onSelectRow={vi.fn()} selectedRowKey={null}>
        <tbody>
          <OperationalTableRow rowKey="34" cells={{ item: "Tritanium", required: "1" }} />
        </tbody>
      </OperationalTable>,
    );

    const header = screen.getByRole("columnheader", { name: "Required" });
    expect(header).toHaveAttribute("aria-sort", "descending");
    await user.click(screen.getByRole("button", { name: "Required" }));
    expect(onToggle).toHaveBeenCalledOnce();

    expect(screen.getByRole("columnheader", { name: "Item" })).not.toHaveAttribute("aria-sort");
  });

  test("group trailing controls sit outside the toggle and rows can carry an accent", async () => {
    const user = userEvent.setup();
    const onMenu = vi.fn();
    render(
      <OperationalTable ariaLabel="Colonies" columns={columns} onSelectRow={vi.fn()} selectedRowKey={null}>
        <OperationalTableGroup
          groupKey="valka"
          itemCount={1}
          label="Valka"
          labelCase="normal"
          leading={<span data-testid="portrait" />}
          trailing={<button onClick={onMenu} type="button">Group menu</button>}
        >
          <OperationalTableRow accent="blocking" align="top" rowKey="p1" cells={{ item: "EUU-4N II", required: "1" }} />
        </OperationalTableGroup>
      </OperationalTable>,
    );

    const toggle = screen.getByRole("button", { name: "Collapse Valka" });
    expect(toggle).toContainElement(screen.getByTestId("portrait"));
    expect(toggle).not.toHaveClass("uppercase");
    await user.click(screen.getByRole("button", { name: "Group menu" }));
    expect(onMenu).toHaveBeenCalled();
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("row", { name: /EUU-4N II/ })).toHaveAttribute("data-accent", "blocking");
  });
});
