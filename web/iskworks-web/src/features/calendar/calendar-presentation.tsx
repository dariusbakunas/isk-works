import {
  Atom,
  Copy,
  Factory,
  FlaskConical,
  Globe2,
  GraduationCap,
  PackageX,
  RefreshCcw,
  type LucideIcon,
} from "lucide-react";
import type { CalendarMilestone } from "../../api/calendar";

export interface CalendarMilestonePresentation {
  kindLabel: "Industry" | "Skill" | "Planetary";
  activityLabel: string;
  tone: "industry" | "skill" | "planetary";
  icon: LucideIcon;
  accessibleLabel: string;
}

const industryPresentation: Record<string, { label: string; icon: LucideIcon }> = {
  manufacturing: { label: "Manufacturing", icon: Factory },
  timeEfficiencyResearch: { label: "Time efficiency research", icon: FlaskConical },
  materialEfficiencyResearch: { label: "Material efficiency research", icon: FlaskConical },
  copying: { label: "Blueprint copying", icon: Copy },
  reverseEngineering: { label: "Reverse engineering", icon: RefreshCcw },
  invention: { label: "Invention", icon: FlaskConical },
  reaction: { label: "Reaction", icon: Atom },
  other: { label: "Industry", icon: Factory },
};

export function presentCalendarMilestone(milestone: CalendarMilestone): CalendarMilestonePresentation {
  if (milestone.kind === "planetary") {
    const activityLabel = milestone.event === "extractorExpiry" ? "Extractor expiry" : "Factory input runs out (estimate)";
    return {
      kindLabel: "Planetary",
      activityLabel,
      tone: "planetary",
      icon: milestone.event === "extractorExpiry" ? Globe2 : PackageX,
      accessibleLabel: `${milestone.title}, ${activityLabel} for ${milestone.characterName}`,
    };
  }
  if (milestone.kind === "skill") {
    return {
      kindLabel: "Skill",
      activityLabel: "Skill completion",
      tone: "skill",
      icon: GraduationCap,
      accessibleLabel: `${milestone.title}, skill completion for ${milestone.characterName}`,
    };
  }
  const activity = industryPresentation[milestone.activity] ?? industryPresentation.other;
  return {
    kindLabel: "Industry",
    activityLabel: activity.label,
    tone: "industry",
    icon: activity.icon,
    accessibleLabel: `${milestone.title}, ${activity.label} for ${milestone.characterName}`,
  };
}

export function CalendarMilestoneLabel({ milestone }: { milestone: CalendarMilestone }) {
  const presentation = presentCalendarMilestone(milestone);
  const Icon = presentation.icon;
  return (
    <span aria-label={presentation.accessibleLabel} className="flex min-w-0 items-center gap-1">
      <Icon aria-hidden="true" className="h-2.5 w-2.5 shrink-0" />
      <span className="truncate" title={milestone.title}>{milestone.title}</span>
    </span>
  );
}
