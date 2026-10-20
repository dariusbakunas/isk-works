import { lazy, Suspense } from "react";
import { Navigate, NavLink, Route, Routes } from "react-router";

import { PageHeader } from "../../components/primitives";
import { ActiveJobsTab } from "./active-jobs-tab";

// History pulls in the charting library, so it loads on demand.
const JobHistoryTab = lazy(() =>
  import("./job-history-tab").then((module) => ({ default: module.JobHistoryTab })),
);

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex h-9 items-center border-b-2 px-3 text-xs font-semibold transition ${
    isActive ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
  }`;

// Jobs: every industry job across the workspace's characters. Active is
// what is running now; History is what finished (kept beyond ESI's 90 days).
export function JobsPage() {
  return (
    <>
      <PageHeader eyebrow="Industry" title="Jobs">
        Industry jobs across your characters: what is running, what is ready to deliver, and what finished.
      </PageHeader>
      <nav aria-label="Jobs views" className="mb-3 flex items-stretch border-b border-border">
        <NavLink className={tabClass} end to="/jobs">Active</NavLink>
        <NavLink className={tabClass} to="/jobs/history">History</NavLink>
      </nav>
      <Routes>
        <Route element={<ActiveJobsTab />} index />
        <Route
          element={<Suspense fallback={<div className="iw-panel p-3 text-sm text-muted">Loading history…</div>}><JobHistoryTab /></Suspense>}
          path="history"
        />
        <Route element={<Navigate replace to="/jobs" />} path="*" />
      </Routes>
    </>
  );
}
