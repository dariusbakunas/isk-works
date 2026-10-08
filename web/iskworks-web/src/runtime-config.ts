/**
 * Runtime configuration written by the web container at startup (see
 * docker/40-iskworks-runtime-config.sh) into /config.js, which index.html
 * loads before the app bundle. Operator settings live here, not in the build,
 * so one published image serves every deployment. Local `vite dev` has no
 * generated /config.js and falls back to VITE_* variables instead.
 */
export type RuntimeConfig = {
  logRocketAppId?: string;
  logRocketEnabled?: boolean;
  supportUrl?: string;
  supportLabel?: string;
  donationCharacter?: string;
  sourceUrl?: string;
};

declare global {
  interface Window {
    __ISKWORKS_CONFIG__?: RuntimeConfig;
  }
}

export function runtimeConfig(): RuntimeConfig | undefined {
  return typeof window === "undefined" ? undefined : window.__ISKWORKS_CONFIG__;
}
