import type { ReactNode } from "react";
import { Link } from "react-router";

import { AppFooter } from "../../components/app-footer";
import { CCP_NON_AFFILIATION, CCP_PROPRIETARY_NOTICE, communityLinks } from "../../components/community-links";
import { PageHeader, Panel } from "../../components/primitives";
import { isLogRocketEnabled } from "../../observability/logrocket";
import { SESSION_REPLAY_DISCLOSURE } from "../../observability/session-replay-disclosure";

const ESI_SCOPE_PURPOSES: { scope: string; purpose: string }[] = [
  { scope: "Skills and skill queue", purpose: "job time and cost calculations, skill planning" },
  { scope: "Blueprints", purpose: "ME/TE and runs for build planning" },
  { scope: "Industry jobs", purpose: "tracking running manufacturing and reaction jobs" },
  { scope: "Assets", purpose: "inventory and asset views" },
  { scope: "Wallet", purpose: "importing transactions into Finance and Inventory cost tracking" },
  { scope: "Structure markets and structures", purpose: "market prices and facility names in player structures" },
  { scope: "Planets", purpose: "the Planetary Interaction page" },
  { scope: "Location", purpose: "showing where a character currently is" },
];

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Panel className="grid gap-2 text-sm leading-relaxed">
      <h2 className="text-base font-semibold">{title}</h2>
      {children}
    </Panel>
  );
}

/**
 * About & Legal: CCP notice, as-is disclaimer, plain-language privacy notice,
 * and community/donation pointers. Reachable without signing in.
 */
export function LegalPage() {
  const { support, donationCharacter, sourceUrl } = communityLinks();
  const supportLink = support ? (
    <a className="text-primary underline" href={support.url} rel="noopener noreferrer" target="_blank">
      {support.label}
    </a>
  ) : null;

  return (
    <div className="min-h-[var(--iw-viewport-h)] text-foreground">
      <main className="mx-auto grid w-full max-w-3xl gap-3 px-4 py-8">
        <Link className="justify-self-start text-xs text-muted hover:text-foreground hover:underline" to="/">
          ← Back to ISK Works
        </Link>
        <PageHeader eyebrow="ISK Works" title="About, privacy & legal">
          ISK Works is a free, fan-made industry and accounting tool for EVE Online.
        </PageHeader>

        <Section title="Community & support">
          {supportLink ? (
            <p>Questions, bug reports, feature ideas, and data requests go through {supportLink}.</p>
          ) : (
            <p>Questions, bug reports, feature ideas, and data requests go to the operator of this instance.</p>
          )}
          {donationCharacter ? (
            <p>
              ISK Works is free. If you want to support it, ISK donations to the in-game character{" "}
              <strong>{donationCharacter}</strong> are appreciated. Donations are entirely optional and unlock nothing.
            </p>
          ) : (
            <p>ISK Works is free.</p>
          )}
          <p>
            ISK Works is open source under the GNU Affero General Public License v3. The{" "}
            <a className="text-primary underline" href={sourceUrl} rel="noopener noreferrer" target="_blank">
              source code
            </a>{" "}
            is available to everyone.
          </p>
        </Section>

        <Section title="No warranty">
          <p>
            ISK Works is provided as is, without warranty of any kind. Costs, profits, prices, and
            build plans are estimates based on game data, market data, and your own inputs, and may be
            wrong or out of date. Check the numbers before committing ISK. The authors are not liable
            for any in-game or real-world losses from using ISK Works.
          </p>
          <p>The service may change, be interrupted, or be discontinued at any time.</p>
        </Section>

        <Section title="Privacy">
          <p>
            <strong>What we store.</strong> When you sign in with EVE Online we store your character ID
            and name, and an EVE SSO refresh token for each character you connect. Refresh tokens are
            encrypted at rest. We also store the data you create in ISK Works (builds, inventory,
            finance records, facilities, price overrides) and data synced from ESI for your connected
            characters.
          </p>
          <div>
            <p>
              <strong>Why we ask for ESI scopes.</strong> ISK Works only reads data. The scopes are used for:
            </p>
            <ul className="mt-1 list-disc pl-5">
              {ESI_SCOPE_PURPOSES.map(({ scope, purpose }) => (
                <li key={scope}>
                  {scope}: {purpose}
                </li>
              ))}
            </ul>
          </div>
          <p>
            <strong>Who can see it.</strong> Your workspace is private to your account. We do not sell or
            share your data and do not use it for advertising.
          </p>
          {isLogRocketEnabled() ? (
            <p>
              <strong>Diagnostics.</strong> {SESSION_REPLAY_DISCLOSURE} Replay is provided by LogRocket.
            </p>
          ) : null}
          <p>
            <strong>Cookies.</strong> ISK Works uses only the cookie needed to keep you signed in. There
            are no advertising or tracking cookies.
          </p>
          <p>
            <strong>Your control.</strong> Disconnecting a character on the Characters page revokes its
            token with EVE. You can also revoke ISK Works' access at any time from your EVE account's
            third-party application settings. To have your account and all of its data deleted,{" "}
            {supportLink ? <>ask via {supportLink}</> : "contact the operator of this instance"} and we will erase it.
          </p>
        </Section>

        <Section title="CCP notice">
          <p className="text-muted">{CCP_PROPRIETARY_NOTICE}</p>
          <p className="text-muted">{CCP_NON_AFFILIATION}</p>
        </Section>
      </main>
      <AppFooter />
    </div>
  );
}
