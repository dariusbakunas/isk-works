import { useState } from "react";

import { beginLogin } from "../../api/auth";
import { ApiError } from "../../api/workspace";
import mascot from "../../assets/mascot.png";
import { AppFooter } from "../../components/app-footer";
import { InlineAlert, PageHeader, Panel } from "../../components/primitives";
import { PRIVATE_ATTR } from "../../observability/private";
import { SessionReplayDisclosure } from "../../observability/session-replay-disclosure";

/**
 * The `?status=` value the OAuth callback bounces back with when a new
 * identity could not be onboarded. `denied` is a user-cancelled EVE consent;
 * the two invite states come from invite-only mode (see
 * `docs/security/invite-only-alpha.md`). `character_transferred` is a
 * character now owned by a different EVE account. Anything else is ignored.
 */
type CallbackStatus =
  | "denied"
  | "invite_required"
  | "invite_invalid"
  | "account_disabled"
  | "character_transferred"
  | null;

function readCallbackStatus(): CallbackStatus {
  if (typeof window === "undefined") return null;
  const status = new URLSearchParams(window.location.search).get("status");
  return status === "denied" ||
    status === "invite_required" ||
    status === "invite_invalid" ||
    status === "account_disabled" ||
    status === "character_transferred"
    ? status
    : null;
}

const INVITE_INVALID_MESSAGE = "That invite code isn’t valid. Check it and try again.";

export function LoginPage({ inviteRequired = false }: { inviteRequired?: boolean }) {
  const initialStatus = readCallbackStatus();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [inviteCode, setInviteCode] = useState("");
  const [inviteOpen, setInviteOpen] = useState(
    initialStatus === "invite_required" || initialStatus === "invite_invalid",
  );
  const [notice, setNotice] = useState<string>(() => {
    if (initialStatus === "denied") return "EVE sign-in was cancelled. You can try again.";
    if (initialStatus === "invite_required")
      return "ISK Works is invite-only right now. Enter an invite code to create an account.";
    if (initialStatus === "invite_invalid") return INVITE_INVALID_MESSAGE;
    if (initialStatus === "account_disabled")
      return "This account has been disabled. Contact the ISK Works administrator if you think that is a mistake.";
    if (initialStatus === "character_transferred")
      return "This character has been transferred to another EVE account since it was used to create its ISK Works account, so it can't sign in to that account. Contact the ISK Works administrator.";
    return "";
  });

  async function start(withInvite: boolean) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const started = await beginLogin(withInvite ? inviteCode : undefined);
      window.location.assign(started.authorizationUrl);
    } catch (caught) {
      if (caught instanceof ApiError && caught.body.code === "invite_invalid") {
        setError(INVITE_INVALID_MESSAGE);
        setInviteOpen(true);
      } else if (caught instanceof ApiError && caught.body.code === "invite_required") {
        setError("An invite code is required to create a new account.");
        setInviteOpen(true);
      } else if (caught instanceof ApiError) {
        setError(caught.body.message);
      } else {
        setError("ISK Works could not start EVE SSO sign-in. Try again.");
      }
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-[var(--iw-viewport-h)] place-items-center content-center gap-4 px-4 py-8">
      <section className="iw-panel w-full max-w-md p-6">
        <img className="mx-auto mb-4 h-40 w-40" src={mascot} alt="" aria-hidden="true" />
        <PageHeader eyebrow="ISK Works" title="Sign in">
          Your Builds, Finance, Plans, Boards, Assets, and connected Characters are private to your
          own account.
        </PageHeader>
        <Panel>
          {error ? <InlineAlert title="Sign-in failed">{error}</InlineAlert> : null}
          {!error && notice ? <p className="text-sm text-muted">{notice}</p> : null}

          <button
            className="iw-button-primary w-full justify-center"
            disabled={busy}
            onClick={() => start(false)}
            type="button"
          >
            {busy ? "Redirecting to EVE Online..." : "Continue with EVE Online"}
          </button>

          {inviteRequired ? (
            <div className="mt-4 border-t border-border pt-4">
              {inviteOpen ? (
                <form
                  className="space-y-2"
                  onSubmit={(event) => {
                    event.preventDefault();
                    if (!busy) void start(true);
                  }}
                >
                  <label className="block text-sm font-semibold" htmlFor="invite-code">
                    Invite code
                  </label>
                  <input
                    id="invite-code"
                    className="iw-input"
                    value={inviteCode}
                    onChange={(event) => setInviteCode(event.target.value)}
                    placeholder="ISK-XXXX-XXXX-XXXX-XXXX"
                    autoComplete="off"
                    autoCapitalize="characters"
                    spellCheck={false}
                    // Defence in depth: keep the code out of session replay
                    // even though inputs are masked by default.
                    {...PRIVATE_ATTR}
                  />
                  <button
                    className="iw-button-primary w-full justify-center"
                    disabled={busy || inviteCode.trim() === ""}
                    type="submit"
                  >
                    {busy ? "Redirecting to EVE Online..." : "Join alpha with invite"}
                  </button>
                  <p className="text-[0.6875rem] leading-snug text-muted">
                    Already have an ISK Works account? Just use “Continue with EVE Online” above — no
                    invite needed.
                  </p>
                </form>
              ) : (
                <button
                  className="iw-button-secondary w-full justify-center"
                  type="button"
                  onClick={() => setInviteOpen(true)}
                >
                  Have an invite? Join the alpha
                </button>
              )}
            </div>
          ) : null}
        </Panel>
        <SessionReplayDisclosure className="mt-4" />
      </section>
      <AppFooter className="max-w-md" />
    </main>
  );
}
