import { WalletCards } from "lucide-react";
import { lazy, Suspense } from "react";
import { NavLink, Navigate, Route, Routes } from "react-router";

import { AnalyticsSkeleton } from "./analytics/analytics-skeleton";
import { TransactionsPage } from "./transactions-page";

// Analytics pulls in the charting library, so it loads on demand and stays out
// of the main bundle.
const AnalyticsPage = lazy(() =>
  import("./analytics/analytics-page").then((module) => ({ default: module.AnalyticsPage })),
);

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex h-10 items-center border-b-2 px-3 text-xs font-semibold transition ${
    isActive ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
  }`;

// Finance is two views of the same wallet data: the Transactions table and
// Analytics. Overview / Journal / Assets are not part of the alpha, so
// bookmarks to them land on Transactions.
export function FinanceWorkspace() {
  return (
    <section className="finance-workspace -mx-3 -my-3 flex h-[calc(var(--iw-viewport-h)-2.75rem-var(--iw-footer-h,0px))] min-h-0 flex-col overflow-hidden sm:-mx-4 sm:-my-4">
      <header className="flex min-h-10 items-center gap-2 border-b border-border bg-panel px-3">
        <WalletCards className="h-3.5 w-3.5 text-primary" aria-hidden="true" />
        <h1 className="text-sm font-semibold">Finance</h1>
        <span className="text-xs text-muted">Real EVE wallet activity</span>
        <nav aria-label="Finance views" className="ml-4 flex items-stretch self-stretch">
          <NavLink className={tabClass} to="/finance/transactions">Transactions</NavLink>
          <NavLink className={tabClass} to="/finance/analytics">Analytics</NavLink>
        </nav>
      </header>
      <Routes>
        <Route index element={<Navigate replace to="/finance/transactions" />} />
        <Route path="transactions" element={<TransactionsPage />} />
        <Route
          path="analytics"
          element={
            <Suspense fallback={<div className="flex-1 overflow-y-auto px-6 py-3"><AnalyticsSkeleton /></div>}>
              <AnalyticsPage />
            </Suspense>
          }
        />
        <Route path="*" element={<Navigate replace to="/finance/transactions" />} />
      </Routes>
    </section>
  );
}
