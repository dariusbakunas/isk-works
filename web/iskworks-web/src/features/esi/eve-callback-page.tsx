import { Navigate, useSearchParams } from "react-router";

import { ButtonLink, EmptyState, PageHeader, Panel } from "../../components/primitives";

export function EveCallbackPage() {
  const [params] = useSearchParams();
  const connected = params.get("status") === "connected";

  // On success there's nothing for the user to do here — the character is
  // already linked server-side — so skip the interstitial and drop them
  // straight on the Characters page.
  if (connected) {
    return <Navigate to="/characters" replace />;
  }

  return (
    <>
      <PageHeader eyebrow="EVE authorization" title="Connection not completed">
        EVE authorization was denied or could not be completed.
      </PageHeader>
      <Panel>
        <EmptyState title="No connection was changed" action={<ButtonLink to="/characters">Try Again</ButtonLink>}>
          Return to Characters to begin a new single-use authorization.
        </EmptyState>
      </Panel>
    </>
  );
}
