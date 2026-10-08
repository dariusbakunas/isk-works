import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, expect, test, vi } from "vitest";

import type { BuildWorksheetProjection } from "../../../../../api/industry";
import type { BuildWorksheetEditorModel } from "../../use-build-worksheet-editor";
import { BuildWorksheetTable } from "../build-worksheet-table";
import { BuildWorksheetView } from "../build-worksheet-view";

const worksheetHook = vi.hoisted(() => ({ useBuildWorksheet: vi.fn() }));
vi.mock("../use-build-worksheet", () => worksheetHook);
const planHook = vi.hoisted(() => ({ useBuildExecutionPlan: vi.fn() }));
vi.mock("../../stages/use-build-execution-plan", () => planHook);
vi.mock("../../stages/use-plan-inspector", () => ({
  usePlanInspector: () => ({ command: {}, replan: vi.fn(), sourcing: {}, rootEditing: null }),
}));
vi.mock("../../components/production-setup-dialog", () => ({ ProductionSetupDialog: () => null }));
vi.mock("../../stages/stages-inspector", () => ({
  StagesInspector: ({ selection }: { selection: unknown }) => selection ? <aside aria-label="Inspector">{JSON.stringify(selection)}</aside> : null,
}));
beforeEach(() => {
  planHook.useBuildExecutionPlan.mockReturnValue({ plan: null, loading: false, refreshError: null, hardError: null, refetch: vi.fn() });
});
const plan = { occurrences: [{ id: "build:rcf", nodeId: "node-rcf", buildId: "rcf-build", isRoot: false }], nodes: [], acquisitions: [{ typeId: 2, consumers: [] }] };

const worksheet: BuildWorksheetProjection = {
  scope: { rootBuildId: "root", focusedProducerId: null, includeDownstream: false, label: "Root" },
  groups: [{ key: "components", label: "Construction Components", rowCount: 2, complete: false, rows: [
    { id: "rcf", typeId: 1, typeName: "Reinforced Carbon Fiber", categoryId: null, categoryName: null, groupId: null, groupName: null, sourcing: "manufacturing", requiredQuantity: 161, coveredQuantity: 0, shortageQuantity: 161, coveragePercentage: "0.00", evidenceState: "complete", pricing: { state: "complete", classification: "production", policy: null, sourceNote: "market-depth-v1; requested 107; observed timestamp" }, unitCost: "40000", totalValue: "5000000", producerBuildId: "rcf-build", retainedSurplusQuantity: 9, retainedSurplusBasis: "360000", warnings: [] },
    { id: "buy", typeId: 2, typeName: "Morphite", categoryId: null, categoryName: null, groupId: null, groupName: null, sourcing: "buy", requiredQuantity: 10, coveredQuantity: 10, shortageQuantity: 0, coveragePercentage: "100.00", evidenceState: "unpriced", pricing: { state: "unpriced", classification: "marketPolicy", policy: "highestBuy", sourceNote: "market-depth-v1; 3 orders" }, unitCost: null, totalValue: null, producerBuildId: null, retainedSurplusQuantity: null, retainedSurplusBasis: null, warnings: [] },
  ] }],
  output: { typeId: 3, typeName: "Muninn", quantity: 1, unitValue: "100000000", totalValue: "100000000", evidenceState: "complete" },
  warnings: [], economicsAreAdditive: false, generatedAt: "2026-09-26T00:00:00Z",
};

test("renders the final read-only columns, produced and buy values, and no additive grand total", () => {
  render(<BuildWorksheetTable worksheet={worksheet} />);
  expect(screen.getByText("Total Value ⓘ")).toHaveAttribute("title", expect.stringContaining("not additive"));
  expect(screen.queryByText("Available")).not.toBeInTheDocument();
  expect(screen.getByText("Reinforced Carbon Fiber")).toBeInTheDocument();
  expect(screen.getByText("5M")).toBeInTheDocument();
  expect(screen.getAllByText("Unpriced").length).toBeGreaterThan(0);
  expect(screen.getAllByText("0").length).toBeGreaterThan(0);
  expect(screen.queryByText(/Grand Total/i)).not.toBeInTheDocument();
});

test("only inspectable and output rows advertise and perform actions", async () => {
  const onSelectRow = vi.fn();
  const onOpenOutput = vi.fn();
  render(<BuildWorksheetTable onOpenOutput={onOpenOutput} onSelectRow={onSelectRow} worksheet={worksheet} />);

  const produced = screen.getByRole("row", { name: /Reinforced Carbon Fiber/ });
  const buy = screen.getByRole("row", { name: /Morphite/ });
  const output = screen.getByRole("row", { name: /Muninn/ });
  expect(produced).toHaveAttribute("tabindex", "0");
  expect(produced).toHaveClass("cursor-pointer");
  expect(output).toHaveAttribute("tabindex", "0");
  expect(buy).not.toHaveAttribute("tabindex");
  expect(buy).toHaveClass("cursor-default");

  await userEvent.click(produced);
  await userEvent.click(output);
  await userEvent.click(buy);
  expect(onSelectRow).toHaveBeenCalledOnce();
  expect(onSelectRow).toHaveBeenCalledWith(expect.objectContaining({ id: "rcf" }));
  expect(onOpenOutput).toHaveBeenCalledOnce();
});

function Location() {
  const location = useLocation();
  return <output>{location.pathname}{location.search}</output>;
}

const editor = {
  initialBuild: { id: "root" }, previewKey: "{}", linkedBuildsByTypeId: {},
} as unknown as BuildWorksheetEditorModel;

test("selecting a produced row opens the Plan inspector lazily and the toggle clears it", async () => {
  worksheetHook.useBuildWorksheet.mockReturnValue({ worksheet, loading: false, refreshError: "", hardError: "" });
  planHook.useBuildExecutionPlan.mockReturnValue({ plan, loading: false, refreshError: null, hardError: null, refetch: vi.fn() });
  render(
    <MemoryRouter initialEntries={["/builds/root?view=worksheet"]}>
      <Routes><Route path="*" element={<><BuildWorksheetView active editor={editor} /><Location /></>} /></Routes>
    </MemoryRouter>,
  );
  expect(screen.getByText("Materials missing")).toBeInTheDocument();
  expect(planHook.useBuildExecutionPlan).toHaveBeenLastCalledWith(expect.objectContaining({ active: false }));
  expect(screen.queryByLabelText("Inspector")).not.toBeInTheDocument();

  const produced = screen.getByRole("row", { name: /Reinforced Carbon Fiber/ });
  await userEvent.click(produced);
  expect(planHook.useBuildExecutionPlan).toHaveBeenLastCalledWith(expect.objectContaining({ active: true }));
  expect(screen.getByLabelText("Inspector")).toHaveTextContent('"nodeId":"node-rcf"');
  expect(produced).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("status")).toHaveTextContent(/^\/builds\/root\?view=worksheet$/);

  await userEvent.click(screen.getByRole("switch", { name: "Include downstream" }));
  expect(screen.queryByLabelText("Inspector")).not.toBeInTheDocument();
});

test("downstream is off by default and the toggle drives the request, URL, and note", async () => {
  worksheetHook.useBuildWorksheet.mockReturnValue({ worksheet, loading: false, refreshError: "", hardError: "" });
  render(
    <MemoryRouter initialEntries={["/builds/root?view=worksheet"]}>
      <Routes><Route path="*" element={<><BuildWorksheetView active editor={editor} /><Location /></>} /></Routes>
    </MemoryRouter>,
  );
  const toggle = screen.getByRole("switch", { name: "Include downstream" });
  expect(toggle).toHaveAttribute("aria-checked", "false");
  expect(worksheetHook.useBuildWorksheet).toHaveBeenLastCalledWith(expect.objectContaining({ includeDownstream: false }));
  expect(screen.queryByText("Nested values are not additive.")).not.toBeInTheDocument();

  await userEvent.click(toggle);
  expect(toggle).toHaveAttribute("aria-checked", "true");
  expect(worksheetHook.useBuildWorksheet).toHaveBeenLastCalledWith(expect.objectContaining({ includeDownstream: true }));
  expect(screen.getByText("Nested values are not additive.")).toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("?view=worksheet&downstream=1");

  await userEvent.click(toggle);
  expect(screen.getByRole("status")).toHaveTextContent(/^\/builds\/root\?view=worksheet$/);
});

test("focused output returns to that producer's existing Plan view", async () => {
  worksheetHook.useBuildWorksheet.mockReturnValue({ worksheet: { ...worksheet, scope: { ...worksheet.scope, focusedProducerId: "rcf-build" } }, loading: false, refreshError: "", hardError: "" });
  render(
    <MemoryRouter initialEntries={["/builds/root/producers/rcf-build?view=worksheet"]}>
      <Routes><Route path="*" element={<><BuildWorksheetView active editor={editor} focusedProducerId="rcf-build" /><Location /></>} /></Routes>
    </MemoryRouter>,
  );
  await userEvent.click(screen.getByRole("row", { name: /Muninn/ }));
  expect(screen.getByRole("status")).toHaveTextContent(/^\/builds\/root\/producers\/rcf-build\?view=plan$/);
});

test("root output returns to the root Build's explicit Plan view", async () => {
  worksheetHook.useBuildWorksheet.mockReturnValue({ worksheet, loading: false, refreshError: "", hardError: "" });
  render(
    <MemoryRouter initialEntries={["/builds/root"]}>
      <Routes><Route path="*" element={<><BuildWorksheetView active editor={editor} /><Location /></>} /></Routes>
    </MemoryRouter>,
  );
  await userEvent.click(screen.getByRole("row", { name: /Muninn/ }));
  expect(screen.getByRole("status")).toHaveTextContent(/^\/builds\/root\?view=plan$/);
});

test("restores the historical compact coverage, shortage, and pricing presentation", () => {
  render(<BuildWorksheetTable worksheet={worksheet} />);

  const produced = screen.getByRole("row", { name: /Reinforced Carbon Fiber/ });
  expect(produced).toHaveTextContent("Production");
  expect(produced).not.toHaveTextContent("market-depth-v1");
  expect(produced).toHaveTextContent("161");
  expect(produced.querySelector("[data-row-status-dot='blocking']")).toBeInTheDocument();
  expect(produced.querySelector("[data-shortage='true']")).toHaveClass("text-destructive", "whitespace-nowrap");
  expect(within(produced).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "0");
  expect(within(produced).getByText("0.00%")).toHaveClass("whitespace-nowrap");

  const buy = screen.getByRole("row", { name: /Morphite/ });
  expect(buy).toHaveTextContent("Highest buy");
  expect(buy).not.toHaveTextContent("market-depth-v1");
  expect(within(buy).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  expect(buy.querySelector("[data-shortage='false']")).toHaveTextContent("—");

  expect(screen.getByRole("button", { name: "Collapse Construction Components" })).toHaveClass("h-7");
  expect(screen.getByText("Reinforced Carbon Fiber")).toHaveClass("truncate");
  expect(screen.getByRole("columnheader", { name: "Pricing" })).not.toHaveClass("hidden");
  expect(screen.getByRole("columnheader", { name: "Unit Cost" })).not.toHaveClass("hidden");
  expect(screen.getByRole("table", { name: "Production Worksheet" }).parentElement).toHaveClass("overflow-x-auto", "max-w-full");
});

test("renders every pricing classification as compact presentation copy", () => {
  const classifications = [
    ["production", null, "Production", "complete"],
    ["default", null, "Default", "complete"],
    ["manual", null, "Manual", "complete"],
    ["marketPolicy", "highestBuy", "Highest buy", "complete"],
    ["marketPolicy", "lowestSell", "Lowest sell", "complete"],
    ["marketPolicy", "acquireQuantityFromSellOrders", "Buy from sells", "complete"],
    ["marketPolicy", "liquidateQuantityIntoBuyOrders", "Sell into buys", "complete"],
    ["mixed", null, "Mixed", "complete"],
    ["unresolved", null, "Unresolved", "incomplete"],
    ["unresolved", null, "Unpriced", "unpriced"],
  ] as const;
  const rows = classifications.map(([classification, policy, label, state], index) => ({
    ...worksheet.groups[0].rows[0],
    id: `pricing-${index}`,
    typeId: 10_000 + index,
    typeName: label,
    pricing: { classification, policy, state, sourceNote: `diagnostic ${index}` },
  }));
  render(<BuildWorksheetTable worksheet={{ ...worksheet, groups: [{ ...worksheet.groups[0], rowCount: rows.length, rows }] }} />);

  for (const [, , label] of classifications) {
    const row = screen.getByRole("row", { name: new RegExp(label) });
    expect(row).toHaveTextContent(label);
    expect(row).not.toHaveTextContent("diagnostic");
  }
});

test("a buy row fully covered by inventory says Inventory, a partly covered one still says Buy", () => {
  const [group] = worksheet.groups;
  const morphite = group.rows[1];
  const partly = { ...morphite, id: "partly", typeId: 4, typeName: "Zydrine", coveredQuantity: 4, shortageQuantity: 6, coveragePercentage: "40.00" };
  render(<BuildWorksheetTable worksheet={{ ...worksheet, groups: [{ ...group, rowCount: 3, rows: [...group.rows, partly] }] }} />);

  const covered = screen.getByRole("row", { name: /Morphite/ });
  expect(within(covered).getByText("Inventory")).toBeInTheDocument();
  expect(within(covered).queryByText("Buy")).not.toBeInTheDocument();

  const partial = screen.getByRole("row", { name: /Zydrine/ });
  expect(within(partial).getByText("Buy")).toBeInTheDocument();
  expect(within(partial).queryByText("Inventory")).not.toBeInTheDocument();
});
