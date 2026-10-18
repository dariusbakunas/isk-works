import { NavLink } from "react-router";

const tabClass = ({ isActive }: { isActive: boolean }) =>
  `inline-flex h-9 items-center border-b-2 px-3 text-xs font-semibold transition ${
    isActive ? "border-primary text-foreground" : "border-transparent text-muted hover:text-foreground"
  }`;

// Builds is two views: the plans you intend to run, and the blueprints you
// own to run them with. `end` keeps Plans from matching /builds/blueprints.
export function BuildsSectionTabs({ className = "" }: { className?: string }) {
  return (
    <nav aria-label="Builds views" className={`flex items-stretch ${className}`}>
      <NavLink className={tabClass} end to="/builds">Plans</NavLink>
      <NavLink className={tabClass} to="/builds/blueprints">Blueprints</NavLink>
    </nav>
  );
}
