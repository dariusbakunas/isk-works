import { NavLink } from "react-router";

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex h-9 items-center border-b-2 px-3 text-xs font-semibold transition ${
    isActive ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
  }`;

// Facilities is two views: the profiles themselves, and which profile new
// Builds default to per rig target. `end` keeps Facilities from matching
// /facilities/defaults.
export function FacilitiesSectionTabs() {
  return (
    <nav aria-label="Facilities views" className="mt-4 flex items-stretch border-b border-border">
      <NavLink className={tabClass} end to="/facilities">Facilities</NavLink>
      <NavLink className={tabClass} to="/facilities/defaults">Defaults</NavLink>
    </nav>
  );
}
