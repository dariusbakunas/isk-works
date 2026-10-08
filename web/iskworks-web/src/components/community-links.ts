import { type RuntimeConfig, runtimeConfig } from "../runtime-config";

// Community, support, and CCP notice strings shown in the footer, on the sign-in page,
// on the About & Legal page, and in Help.

export const LEGAL_PATH = "/legal";
export const HELP_PATH = "/help";

// Where the unmodified source lives. Operators running a modified build must point
// ISKWORKS_SOURCE_URL at their own source (AGPL-3.0, section 13).
export const UPSTREAM_SOURCE_URL = "https://github.com/dariusbakunas/isk-works";

const DEFAULT_SUPPORT_LABEL = "Community";

export type CommunityLinks = {
  /** The instance's support/community channel, or null when the operator set none. */
  support: { url: string; label: string } | null;
  /** In-game character that receives ISK donations, or null to hide the donation note. */
  donationCharacter: string | null;
  sourceUrl: string;
};

function text(value: string | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed === "" ? null : trimmed;
}

function webUrl(value: string | undefined): string | null {
  const candidate = text(value);
  return candidate && /^https?:\/\//i.test(candidate) ? candidate : null;
}

/**
 * Operator-configured community links. When the runtime config exists (every
 * container), it is the only source; the VITE_* build variables are a fallback
 * for local `vite dev` only, where no /config.js is generated.
 */
export function resolveCommunityLinks(
  runtime: RuntimeConfig | undefined,
  build: { supportUrl?: string; supportLabel?: string; donationCharacter?: string; sourceUrl?: string },
): CommunityLinks {
  const source = runtime ?? build;
  const supportUrl = webUrl(source.supportUrl);
  return {
    support: supportUrl ? { url: supportUrl, label: text(source.supportLabel) ?? DEFAULT_SUPPORT_LABEL } : null,
    donationCharacter: text(source.donationCharacter),
    sourceUrl: webUrl(source.sourceUrl) ?? UPSTREAM_SOURCE_URL,
  };
}

export function communityLinks(): CommunityLinks {
  return resolveCommunityLinks(runtimeConfig(), {
    supportUrl: import.meta.env.VITE_SUPPORT_URL,
    supportLabel: import.meta.env.VITE_SUPPORT_LABEL,
    donationCharacter: import.meta.env.VITE_DONATION_CHARACTER,
    sourceUrl: import.meta.env.VITE_SOURCE_URL,
  });
}

// The proprietary notice required by section 7.1 of CCP's Developer License
// Agreement (https://developers.eveonline.com/license-agreement), verbatim.
export const CCP_PROPRIETARY_NOTICE =
  "© 2014 CCP hf. All rights reserved. \"EVE\", \"EVE Online\", \"CCP\", and all related logos and " +
  "images are trademarks or registered trademarks of CCP hf.";

export const CCP_NON_AFFILIATION =
  "ISK Works is a third-party application. It is not affiliated with or endorsed by CCP hf. (Fenris Creations), and CCP is " +
  "not responsible for its content or functioning.";
