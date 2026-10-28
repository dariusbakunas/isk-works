import { describe, expect, test } from "vitest";

import { isApplePlatform, modKey } from "../shortcut-keys";

const nav = (platform: string, uaPlatform?: string) =>
  ({ platform, userAgentData: uaPlatform === undefined ? undefined : { platform: uaPlatform } }) as unknown as Navigator;

describe("modKey", () => {
  test("shows ⌘ on Macs and iPads", () => {
    expect(modKey(nav("MacIntel"))).toBe("⌘");
    expect(modKey(nav("iPad"))).toBe("⌘");
    expect(modKey(nav("", "macOS"))).toBe("⌘");
  });

  test("shows Ctrl on Windows, Linux and unknown platforms", () => {
    expect(modKey(nav("Win32"))).toBe("Ctrl ");
    expect(modKey(nav("Linux x86_64"))).toBe("Ctrl ");
    expect(modKey(nav("", "Windows"))).toBe("Ctrl ");
    expect(modKey(nav(""))).toBe("Ctrl ");
  });

  test("prefers User-Agent Client Hints over the legacy platform", () => {
    expect(isApplePlatform(nav("MacIntel", "Windows"))).toBe(false);
  });
});
