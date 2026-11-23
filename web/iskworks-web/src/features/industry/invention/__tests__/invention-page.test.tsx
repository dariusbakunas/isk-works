import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { beforeEach, describe, expect, test, vi } from "vitest";

import { listCharacters, type CharacterRosterEntry } from "../../../../api/characters";
import { listFacilities, type FacilityProfile } from "../../../../api/industry";
import { quoteInvention, searchInventableProducts, type InventionQuote } from "../../../../api/invention";
import { InventionPage } from "../invention-page";
import { ACCELERANT, PROCESS, quote } from "./invention-fixtures";

vi.mock("../../../../api/invention", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../../api/invention")>(),
  quoteInvention: vi.fn(),
  searchInventableProducts: vi.fn(),
}));
vi.mock("../../../../api/characters", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../../api/characters")>(),
  listCharacters: vi.fn(),
}));
vi.mock("../../../../api/industry", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../../api/industry")>(),
  listFacilities: vi.fn(),
}));

const CHARACTER = "0b8f8d6e-2f43-4c1a-9a55-5d0f3c6f9a11";
const RAITARU = "5d0f3c6f-9a11-4c1a-9a55-0b8f8d6e2f43";
const TATARA = "9a115d0f-3c6f-4c1a-9a55-2f430b8f8d6e";

function facility(id: string, name: string, role: FacilityProfile["role"]): FacilityProfile {
  return { id, name, role, solarSystemName: "Tama", archivedAt: null } as FacilityProfile;
}

let location = "";
function LocationProbe() {
  const current = useLocation();
  location = `${current.pathname}${current.search}`;
  return null;
}

function renderPage(path = "/invention?product=2185") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path="/invention" element={<><InventionPage /><LocationProbe /></>} />
      </Routes>
    </MemoryRouter>,
  );
}

function respond(fixture: InventionQuote | null, inventionDataAvailable = true) {
  vi.mocked(quoteInvention).mockResolvedValue({ inventionDataAvailable, quote: fixture });
}

/** The table row of an option, by its name. */
function row(name: string) {
  const table = screen.getByRole("table", { name: "Decryptor comparison" });
  return within(table).getByText(name, { selector: "span.truncate" }).closest("tr")!;
}

beforeEach(() => {
  window.localStorage.clear();
  vi.mocked(searchInventableProducts).mockReset().mockResolvedValue({ inventionDataAvailable: true, products: [] });
  vi.mocked(quoteInvention).mockReset();
  vi.mocked(listCharacters).mockReset().mockResolvedValue([
    { connectionId: CHARACTER, characterName: "Ana Belle" } as CharacterRosterEntry,
  ]);
  vi.mocked(listFacilities).mockReset().mockResolvedValue([
    facility(TATARA, "Tatara", "reaction"),
    facility(RAITARU, "Raitaru", "manufacturing"),
  ]);
  respond(quote());
});

describe("InventionPage", () => {
  test("asks for a product before anything is quoted", async () => {
    renderPage("/invention");

    expect(await screen.findByText("Pick a product to compare decryptors")).toBeInTheDocument();
    expect(quoteInvention).not.toHaveBeenCalled();
  });

  test("quotes with the first character and manufacturing facility by default", async () => {
    renderPage();

    await screen.findByRole("table", { name: "Decryptor comparison" });
    expect(quoteInvention).toHaveBeenCalledWith(
      {
        productTypeId: 2185,
        targetRuns: 100,
        characterId: CHARACTER,
        inventionFacilityId: RAITARU,
        manufacturingFacilityId: RAITARU,
      },
      expect.any(AbortSignal),
    );
    expect(screen.getByRole("combobox", { name: "Product" })).toHaveValue("Hammerhead II");
  });

  test("lists every option, cheapest unit cost first and unpriced last", async () => {
    renderPage();

    const table = await screen.findByRole("table", { name: "Decryptor comparison" });
    const names = within(table).getAllByRole("row").slice(1)
      .map((entry) => entry.querySelector("[data-column='decryptor'] .truncate")?.textContent);
    expect(names).toEqual(["No decryptor", "Process Decryptor", "Accelerant Decryptor"]);
    expect(within(row("No decryptor")).getByText("Best")).toBeInTheDocument();
    expect(within(row("Process Decryptor")).queryByText("Best")).not.toBeInTheDocument();
  });

  test("shows the chance truncated and money compact with the exact value on hover", async () => {
    renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });

    const process = row("Process Decryptor");
    expect(within(process).getByText("48.1%")).toBeInTheDocument();
    const unitCost = process.querySelector("[data-column='unitCost'] span")!;
    expect(unitCost).toHaveTextContent("774K");
    expect(unitCost).toHaveAttribute("title", expect.stringContaining("774,038.96"));
    expect(within(process).getByText("+72K (+10.3%)")).toBeInTheDocument();
    expect(within(process).getByText("21 · 27")).toBeInTheDocument();
  });

  test("tints the copy figures a decryptor changes", async () => {
    renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });

    const process = row("Process Decryptor");
    expect(within(process).getByText("ME 5")).toHaveAttribute("data-tone", "better");
    expect(within(process).getByText("10 runs")).toHaveAttribute("data-tone", "same");
    expect(within(row("No decryptor")).getByText("ME 2")).toHaveAttribute("data-tone", "same");
  });

  test("an option without a price says so and keeps its chance and attempts", async () => {
    renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });

    const accelerant = row("Accelerant Decryptor");
    expect(within(accelerant).getByText("No price")).toBeInTheDocument();
    expect(accelerant.querySelector("[data-column='unitCost']")).toHaveTextContent("—");
    expect(within(accelerant).getByText("52.5%")).toBeInTheDocument();
    expect(within(accelerant).getByText("20 · 25")).toBeInTheDocument();
  });

  test("summarises the base chance, the skills and the best option", async () => {
    renderPage();

    const summary = await screen.findByRole("region", { name: "Invention summary" });
    expect(within(summary).getByText("30.0%")).toBeInTheDocument();
    expect(within(summary).getByText("43.7%")).toBeInTheDocument();
    expect(within(summary).getByText("×1.458")).toBeInTheDocument();
    expect(within(summary).getByText("2 × Electronic Engineering · 2 × Graviton Physics")).toBeInTheDocument();
    expect(within(summary).getByText("No decryptor")).toBeInTheDocument();
  });

  test("warns when the character can't start the job", async () => {
    respond(quote({ missingSkills: ["Gallente Encryption Methods"] }));
    renderPage();

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Ana Belle can't start this job: Gallente Encryption Methods below the required level.",
    );
    expect(screen.getByRole("table", { name: "Decryptor comparison" })).toBeInTheDocument();
  });

  test("says when the fee can't be priced without an invention facility", async () => {
    respond(quote({ jobFee: { ...quote().jobFee, total: null, systemCostIndex: null, systemIndexCost: null } }));
    renderPage(`/invention?product=2185&inv=none`);

    expect(await screen.findByText(/Choose an invention facility to price the job fee/)).toBeInTheDocument();
    expect(vi.mocked(quoteInvention).mock.calls[0][0]).not.toHaveProperty("inventionFacilityId");
  });

  test("raising a skill re-quotes with the override and keeps it in the url", async () => {
    const user = userEvent.setup();
    respond(quote({
      skills: quote().skills.map((skill) => ({ ...skill, characterLevel: 4, level: 4 })),
    }));
    renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });

    await user.click(screen.getByRole("button", { name: "Raise Graviton Physics" }));

    await waitFor(() => expect(location).toContain("skills=11446%3A5"));
    await waitFor(() => expect(vi.mocked(quoteInvention).mock.calls.at(-1)![0]).toMatchObject({
      skillOverrides: { "11446": 5 },
    }));
  });

  test("an overridden skill is marked and can be reset", async () => {
    const user = userEvent.setup();
    respond(quote({
      skills: quote().skills.map((skill, index) => (index === 0 ? { ...skill, level: 3, overridden: true } : skill)),
    }));
    renderPage("/invention?product=2185&skills=23121:3");
    await screen.findByRole("table", { name: "Decryptor comparison" });

    expect(screen.getByRole("img", { name: "Overridden" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reset" }));

    await waitFor(() => expect(location).not.toContain("skills="));
  });

  test("changing the target runs re-quotes", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });

    const runs = screen.getByRole("spinbutton", { name: "Target runs" });
    await user.clear(runs);
    await user.type(runs, "250");

    await waitFor(() => expect(location).toContain("runs=250"));
    await waitFor(() => expect(vi.mocked(quoteInvention).mock.calls.at(-1)![0]).toMatchObject({ targetRuns: 250 }));
  });

  test("offers the output choice only when the source invents several", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });
    expect(screen.queryByRole("combobox", { name: "Output" })).not.toBeInTheDocument();

    respond(quote({
      productTypeId: 11371,
      productName: "Wolf",
      sourceBlueprintName: "Rifter Blueprint",
      outputs: [
        { productTypeId: 11400, productName: "Jaguar", blueprintTypeId: 11401, blueprintName: "Jaguar Blueprint" },
        { productTypeId: 11371, productName: "Wolf", blueprintTypeId: 11372, blueprintName: "Wolf Blueprint" },
      ],
    }));
    renderPage("/invention?product=11371");

    const output = await screen.findByRole("combobox", { name: "Output" });
    expect(output).toHaveValue("11371");
    expect(screen.getByText("Rifter Blueprint invents 2 blueprints")).toBeInTheDocument();
    await user.selectOptions(output, "Jaguar");
    await waitFor(() => expect(location).toContain("product=11400"));
  });

  test("searching lists products with their source and picking one quotes it", async () => {
    const user = userEvent.setup();
    vi.mocked(searchInventableProducts).mockImplementation(async (query) => ({
      inventionDataAvailable: true,
      products: query === "" ? [] : [{
        productTypeId: 2185,
        productName: "Hammerhead II",
        groupName: "Combat Drone",
        blueprintTypeId: 2186,
        sourceBlueprintTypeId: 2184,
        sourceBlueprintName: "Hammerhead I Blueprint",
        sourceOutputCount: 1,
      }],
    }));
    renderPage("/invention");

    await user.type(screen.getByRole("combobox", { name: "Product" }), "hammer");
    const result = await screen.findByRole("option", { name: /Hammerhead II/ });
    expect(within(result).getByText("from Hammerhead I Blueprint")).toBeInTheDocument();
    await user.click(within(result).getByRole("button"));

    await waitFor(() => expect(location).toBe("/invention?product=2185"));
    expect(await screen.findByRole("table", { name: "Decryptor comparison" })).toBeInTheDocument();
  });

  test("a search with no match says so", async () => {
    const user = userEvent.setup();
    renderPage("/invention");

    await user.type(screen.getByRole("combobox", { name: "Product" }), "tritanium");

    expect(await screen.findByText("No inventable products match")).toBeInTheDocument();
  });

  test("an import without invention data asks for a re-import", async () => {
    vi.mocked(searchInventableProducts).mockResolvedValue({ inventionDataAvailable: false, products: [] });
    renderPage("/invention");

    expect(await screen.findByText(/This SDE import has no invention data/)).toBeInTheDocument();
  });

  test("a failed quote shows the error and a way out", async () => {
    vi.mocked(quoteInvention).mockRejectedValue(new Error("Request failed."));
    renderPage();

    expect(await screen.findByRole("alert")).toHaveTextContent("Request failed.");
    expect(screen.getByRole("button", { name: "Start over" })).toBeInTheDocument();
  });

  test("selecting a row opens its details and remembers it in the url", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });

    await user.click(row("Process Decryptor"));

    await waitFor(() => expect(location).toContain(`option=${PROCESS}`));
    const details = await screen.findByRole("complementary", { name: "Process Decryptor" });
    expect(within(details).getByText("48.1% chance")).toBeInTheDocument();
    expect(within(details).getByText("30.0% base × 1.458 skills × 1.1 decryptor")).toBeInTheDocument();
    expect(within(details).getByText("ME 5 · TE 10 · 10 runs")).toBeInTheDocument();
    expect(within(details).getByText("10 copies needed")).toBeInTheDocument();
    expect(within(details).getByText("21 attempts")).toBeInTheDocument();
    expect(within(details).getByText("27 attempts")).toBeInTheDocument();
    expect(within(details).getByText("54 × Datacore - Graviton Physics")).toBeInTheDocument();
    expect(within(details).getByText("27 × Process Decryptor")).toBeInTheDocument();
  });

  test("the attempt ledger opens the job fee into its lines", async () => {
    renderPage(`/invention?product=2185&option=none`);

    const details = await screen.findByRole("complementary", { name: "No decryptor" });
    const ledger = within(details).getByRole("region", { name: "Attempt cost" });
    expect(within(ledger).getByText("Datacore - Electronic Engineering × 2")).toBeInTheDocument();
    expect(within(ledger).getByText("190K")).toBeInTheDocument();
    // Once for the invention fee and once for the copy fee.
    expect(within(ledger).getAllByText("Job cost base (2%)")).toHaveLength(2);
    expect(within(ledger).getAllByText("System cost index (4.1%)")).toHaveLength(2);
    expect(within(ledger).getByText("429K")).toBeInTheDocument();
  });

  test("a fee reduced by the structure and its rigs says by how much", async () => {
    respond(quote({
      jobFee: { ...quote().jobFee, indexCostReductionPercent: "27.444", systemIndexCost: "5884.1465" },
    }));
    renderPage(`/invention?product=2185&option=none`);

    const details = await screen.findByRole("complementary", { name: "No decryptor" });
    const ledger = within(details).getByRole("region", { name: "Attempt cost" });
    // Only the invention fee has a bonus; the copy fee's line stays out.
    expect(within(ledger).getAllByText("Structure and rig bonuses")).toHaveLength(1);
    expect(within(ledger).getByText("−27.4%")).toBeInTheDocument();
    expect(screen.queryByText(/bonuses for invention are not applied/)).not.toBeInTheDocument();
  });

  test("copies the shopping list for the confident attempts", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    renderPage(`/invention?product=2185&option=${PROCESS}`);

    await user.click(await screen.findByRole("button", { name: "Copy shopping list" }));

    expect(writeText).toHaveBeenCalledWith(
      "Datacore - Electronic Engineering 54\nDatacore - Graviton Physics 54\nProcess Decryptor 27",
    );
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
    vi.unstubAllGlobals();
  });

  test("an unpriced option's details name what is missing", async () => {
    renderPage(`/invention?product=2185&option=${ACCELERANT}`);

    const details = await screen.findByRole("complementary", { name: "Accelerant Decryptor" });
    expect(within(details).getByText("No decryptor price")).toBeInTheDocument();
  });

  test("the character chosen last becomes the default", async () => {
    const user = userEvent.setup();
    const other = "63484f19-af12-49ce-aa68-858ef8ddfad2";
    vi.mocked(listCharacters).mockResolvedValue([
      { connectionId: CHARACTER, characterName: "Ana Belle" } as CharacterRosterEntry,
      { connectionId: other, characterName: "Xuesen" } as CharacterRosterEntry,
    ]);
    const first = renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });
    await user.selectOptions(screen.getByRole("combobox", { name: "Character" }), "Xuesen");
    await waitFor(() => expect(location).toContain(`char=${other}`));
    first.unmount();

    renderPage();

    await waitFor(() => expect(vi.mocked(quoteInvention).mock.calls.at(-1)![0]).toMatchObject({ characterId: other }));
    expect(location).toBe("/invention?product=2185");
  });

  test("offers recently quoted products on the empty page", async () => {
    const first = renderPage();
    await screen.findByRole("table", { name: "Decryptor comparison" });
    first.unmount();

    renderPage("/invention");

    expect(await screen.findByRole("button", { name: "Hammerhead II" })).toBeInTheDocument();
  });
});
