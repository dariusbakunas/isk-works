import { describe, expect, test } from "vitest";

import { countdown, formatHours, formatQuantity, importTone, isStale, storageTone } from "../planetary-format";

const NOW = Date.parse("2026-10-02T14:32:00Z");

describe("planetary formatting", () => {
  test("formats hours like the prototype", () => {
    expect(formatHours(2.2)).toBe("2.2h");
    expect(formatHours(100.4)).toBe("4d 4h");
    expect(formatHours(99.99)).toBe("4d 4h");
    expect(formatHours(47.6)).toBe("2d 0h");
    expect(formatHours(24 * 9)).toBe("9d");
    expect(formatHours(0)).toBe("0.0h");
    expect(formatHours(-3)).toBe("0.0h");
  });

  test("countdown is amber under a day and red once expired", () => {
    expect(countdown("2026-10-02T16:44:00Z", NOW)).toMatchObject({ label: "2.2h", tone: "warn", expired: false });
    expect(countdown("2026-10-06T18:32:00Z", NOW)).toMatchObject({ label: "4d 4h", tone: "ok" });
    expect(countdown("2026-10-02T11:32:00Z", NOW)).toMatchObject({ label: "Expired", tone: "danger", expired: true });
  });

  test("storage and import thresholds", () => {
    expect(storageTone(69.9)).toBe("ok");
    expect(storageTone(70)).toBe("warn");
    expect(storageTone(90)).toBe("danger");
    expect(importTone(0)).toBe("danger");
    expect(importTone(22.1)).toBe("warn");
    expect(importTone(46)).toBe("ok");
  });

  test("quantities and staleness", () => {
    expect(formatQuantity(1_200)).toBe("1.2K");
    expect(formatQuantity(40)).toBe("40");
    expect(isStale("2026-10-02T13:00:00Z", NOW)).toBe(true);
    expect(isStale("2026-10-02T14:00:00Z", NOW)).toBe(false);
    expect(isStale(null, NOW)).toBe(false);
  });
});
