// Client-side CSV for exports built from data a page already has.

export type CsvValue = string | number | null;

/** A leading `= + - @` (or tab/CR) makes a spreadsheet evaluate the cell, so
 * it is prefixed with `'`; quotes, commas and newlines are quoted. Same rule
 * as the API's `safe_cell`. */
export function csvCell(value: CsvValue): string {
  if (value === null) return "";
  let text = String(value);
  if (typeof value === "string" && /^[=+\-@\t\r]/.test(text)) text = `'${text}`;
  return /[",\n\r]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

export function toCsv(rows: CsvValue[][]): string {
  return rows.map((row) => row.map(csvCell).join(",")).join("\n") + "\n";
}

/** Saves text through a throwaway link. */
export function downloadText(text: string, filename: string, type: string): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
