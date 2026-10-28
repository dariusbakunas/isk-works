// Shortcut handlers accept ⌘ or Ctrl; hints show the one this keyboard has.

type NavigatorWithUAData = Navigator & { userAgentData?: { platform?: string } };

export function isApplePlatform(nav: NavigatorWithUAData | undefined = globalThis.navigator): boolean {
  const platform = nav?.userAgentData?.platform || nav?.platform || "";
  return /mac|iphone|ipad|ipod/i.test(platform);
}

/** "⌘" on Apple platforms, "Ctrl " elsewhere, ready to prefix a key: `${modKey()}K`. */
export function modKey(nav?: NavigatorWithUAData): string {
  return isApplePlatform(nav) ? "⌘" : "Ctrl ";
}
