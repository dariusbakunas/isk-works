import { isLogRocketEnabled } from "./logrocket";

/**
 * Plain-language notice that the alpha runs privacy-filtered session replay.
 * Not a legal privacy policy and not a consent gate — see
 * docs/security/session-replay-privacy.md.
 *
 * Decision: **disclosure-only, explicit replay opt-in deferred.** LogRocket
 * initializes before app render, so a real opt-in would mean delaying init
 * and persisting a per-viewer preference, and a fake pre-consent toggle
 * that records anyway would be worse than honest disclosure. This line is
 * shown on the sign-in / invite surface so the disclosure reads as
 * intentional.
 */
export const SESSION_REPLAY_DISCLOSURE =
  "During the alpha, ISK Works uses privacy-filtered diagnostic session replay to " +
  "identify bugs and improve the product. Sensitive inputs, authentication data, " +
  "and selected financial and account data are excluded before anything leaves your browser.";

/**
 * Renders the disclosure line, but only in builds where replay is actually
 * enabled. Drop it onto the sign-in screen (and, later, the invite/
 * onboarding screen).
 */
export function SessionReplayDisclosure({ className = "" }: { className?: string }) {
  if (!isLogRocketEnabled()) return null;
  return (
    <p className={`text-[0.6875rem] leading-snug text-muted ${className}`}>
      {SESSION_REPLAY_DISCLOSURE}
    </p>
  );
}
