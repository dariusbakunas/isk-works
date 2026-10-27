import { useMemo, useState } from "react";
import { Route, Routes } from "react-router";

import { FitList } from "./fit-list";
import { FitView } from "./fit-view";
import { FittingsContext, type FittingsContextValue } from "./fit-shared";
import { useFittingsMeta, useRoster } from "./use-fittings";

function Footer({ value }: { value: FittingsContextValue }) {
  const sde = value.meta?.sde?.buildNumber;
  const engine = value.meta?.engine?.dataVersion;
  return (
    <p className="mt-4 font-mono text-[0.625rem] text-muted">
      {sde ? `SDE build ${sde}` : "No SDE import"}
      {" · "}
      {engine ? `Stats engine data ${engine}` : "Stats engine data unavailable"}
    </p>
  );
}

// Fittings: saved ship fits, who in the workspace can fly them, and their
// stats for a chosen pilot (All V by default) against a damage profile.
export function FittingsPage() {
  const meta = useFittingsMeta();
  const roster = useRoster();
  // Bumped after an import, a copy or a delete so the list refetches.
  const [revision, setRevision] = useState(0);
  const value = useMemo(
    () => ({ meta: meta.data, roster: roster.data ?? [] }),
    [meta.data, roster.data],
  );

  return (
    <FittingsContext.Provider value={value}>
      <div className="iw-page-container">
        <Routes>
          <Route element={<FitList onChanged={() => setRevision((current) => current + 1)} revision={revision} />} index />
          <Route element={<FitView onChanged={() => setRevision((current) => current + 1)} />} path=":fitId" />
        </Routes>
        {meta.data ? <Footer value={value} /> : null}
      </div>
    </FittingsContext.Provider>
  );
}
