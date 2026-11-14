import { NavLink } from "react-router";

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex h-9 items-center border-b-2 px-3 text-xs font-semibold transition ${
    isActive ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
  }`;

// Planetary is three views: the characters' synced colonies, the Planner for
// what to build next, and the plans saved from it. `end` keeps Colonies from
// matching the others.
export function PlanetarySectionTabs() {
  return (
    <nav aria-label="Planetary views" className="mb-3 flex items-stretch border-b border-border">
      <NavLink className={tabClass} end to="/planetary">Colonies</NavLink>
      <NavLink className={tabClass} to="/planetary/planner">Planner</NavLink>
      <NavLink className={tabClass} to="/planetary/plans">Plans</NavLink>
    </nav>
  );
}
