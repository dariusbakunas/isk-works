import type { ReactNode } from "react";

import { CharacterName } from "../observability/private";
import { OperationalTable, OperationalTableRow, type OperationalColumn } from "./operational-table";

export interface HoldingsTableRow {
  key: string;
  characterName: string;
  /** Shown before the name (e.g. a character portrait). */
  portrait?: ReactNode;
  location: string;
  /** Hover text for the location, e.g. its id and flag. */
  locationTitle?: string;
  quantity: number;
  /** Fades the values (not the action), e.g. a location left out of a total. */
  dimmed?: boolean;
  action?: ReactNode;
}

// The rows don't select: the only actions are the caller's own buttons.
const noSelection = () => {};

/** Where characters hold one type: a row per character and location, and
 * the total. An optional trailing column holds a per-row action. */
export function HoldingsTable({
  ariaLabel,
  rows,
  total,
  action,
}: {
  ariaLabel: string;
  rows: HoldingsTableRow[];
  total: number;
  action?: { label: string; width: string };
}) {
  const columns: OperationalColumn[] = [
    { key: "character", label: "Character", width: "minmax(10rem,1fr)" },
    { key: "location", label: "Location", width: "minmax(10rem,1.5fr)" },
    { key: "quantity", label: "Quantity", width: "6rem", align: "right", numeric: true },
    ...(action ? [{ key: "action", label: action.label, width: action.width, align: "right" as const }] : []),
  ];

  return (
    <OperationalTable ariaLabel={ariaLabel} columns={columns} onSelectRow={noSelection} selectedRowKey={null}>
      <tbody>
        {rows.map((row) => {
          const fade = row.dimmed ? "opacity-60" : "";
          return (
            <OperationalTableRow
              cells={{
                // `w-0 min-w-full`: fill the column but add nothing to the
                // table's own width, so a long name truncates instead of
                // widening its flexible column past the dialog.
                character: (
                  <span className={`flex w-0 min-w-full items-center gap-2 ${fade}`}>
                    {row.portrait}
                    <CharacterName className="truncate" name={row.characterName} />
                  </span>
                ),
                location: (
                  <span className={`block w-0 min-w-full truncate text-muted ${fade}`} title={row.locationTitle ?? row.location}>
                    {row.location}
                  </span>
                ),
                quantity: <span className={fade}>{row.quantity.toLocaleString("en-US")}</span>,
                action: row.action,
              }}
              interactive={false}
              key={row.key}
              rowKey={row.key}
            />
          );
        })}
      </tbody>
      <tfoot>
        <tr className="grid grid-cols-[var(--operational-columns)] font-semibold">
          <td className="col-span-2 flex min-h-8 items-center px-2 py-1">Total</td>
          <td className="flex min-h-8 items-center justify-end px-2 py-1 font-mono tabular-nums">{total.toLocaleString("en-US")}</td>
          {action ? <td /> : null}
        </tr>
      </tfoot>
    </OperationalTable>
  );
}
