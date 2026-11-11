import type { ReactNode } from "react";

import { formatDuration } from "../../../../lib/formatting";

import { BlueprintBody } from "./bodies/blueprint-body";
import { CostBody } from "./bodies/cost-body";
import { CoverageBody } from "./bodies/coverage-body";
import { FacilityBody } from "./bodies/facility-body";
import { PricingBody } from "./bodies/pricing-body";
import { ProvenanceBody } from "./bodies/provenance-body";
import { QuantitiesBody } from "./bodies/quantities-body";
import { RelatedBuildsBody } from "./bodies/related-builds-body";
import { RootPricingBody } from "./bodies/root-pricing-body";
import { SourcingBody } from "./bodies/sourcing-body";
import { IdentityHeader, WarningBadges } from "./inspector-header";
import { INSPECTOR_SECTION_ORDER, type InspectorActions, type InspectorModel } from "./inspector-model";
import { InspectorRow, InspectorSection } from "./inspector-section";

/**
 * The single presentation component for a selected planning object. Both the
 * Worksheet item inspector and the Graph node inspector build an
 * `InspectorModel` + `InspectorActions` from their own selection and render
 * THIS -- there is no separate Worksheet or Graph inspector body.
 *
 * A section renders only when its model slice is present, so a raw material
 * and a linked Build expose different sections from the same component.
 * Collapse state is keyed by semantic section id (`InspectorCollapseProvider`
 * above), so it survives selection changes and Worksheet <-> Graph switches.
 */
export function UnifiedItemInspector({
  model,
  actions,
  readOnly = false,
}: {
  model: InspectorModel;
  actions: InspectorActions;
  /** Show every value, change nothing (an Epic is selected): editable
   * bodies render disabled, and the settings footer and "Open linked
   * build" are hidden. Close and section toggles still work. */
  readOnly?: boolean;
}) {
  // Disabling a fieldset disables every form control inside it, so the
  // current choices stay visible but can't be changed.
  const editable = (body: ReactNode) =>
    readOnly ? (
      <fieldset className="m-0 min-w-0 border-0 p-0" disabled>
        {body}
      </fieldset>
    ) : (
      body
    );
  const sections: Partial<Record<(typeof INSPECTOR_SECTION_ORDER)[number], ReactNode>> = {
    quantities: model.quantities ? (
      <InspectorSection defaultExpanded id="quantities" label="Quantities" summary={model.quantities.summary}>
        <QuantitiesBody slice={model.quantities} />
      </InspectorSection>
    ) : null,
    coverage: model.coverage ? (
      <InspectorSection defaultExpanded id="coverage" label="Coverage" summary={model.coverage.summary}>
        <CoverageBody slice={model.coverage} />
      </InspectorSection>
    ) : null,
    sourcing: model.sourcing ? (
      <InspectorSection defaultExpanded id="sourcing" label="Sourcing" summary={model.sourcing.summary}>
        {editable(<SourcingBody name={model.identity.name} slice={model.sourcing} actions={actions.sourcing} />)}
      </InspectorSection>
    ) : null,
    blueprint: model.blueprint ? (
      <InspectorSection defaultExpanded id="blueprint" label="Blueprint" summary={model.blueprint.summary}>
        {editable(<BlueprintBody slice={model.blueprint} actions={actions.blueprint} />)}
      </InspectorSection>
    ) : null,
    recipe: model.recipe ? (
      <InspectorSection defaultExpanded id="recipe" label="Recipe" summary={model.recipe.summary}>
        <InspectorRow label="Reaction formula" value={model.recipe.name ?? "—"} />
      </InspectorSection>
    ) : null,
    facility: model.facility ? (
      <InspectorSection defaultExpanded id="facility" label="Facility" summary={model.facility.summary}>
        {editable(<FacilityBody slice={model.facility} actions={actions.facility} />)}
      </InspectorSection>
    ) : null,
    cost: model.cost ? (
      <InspectorSection defaultExpanded id="cost" label="Cost" summary={model.cost.summary}>
        <CostBody slice={model.cost} />
      </InspectorSection>
    ) : null,
    pricing: model.pricing ? (
      <InspectorSection defaultExpanded id="pricing" label="Pricing" summary={model.pricing.summary}>
        {model.pricing.kind === "root" ? (
          editable(<RootPricingBody slice={model.pricing} actions={actions.pricing} />)
        ) : (
          editable(<PricingBody slice={model.pricing} onChange={actions.pricing?.onChange} />)
        )}
      </InspectorSection>
    ) : null,
    duration:
      model.durationSeconds != null ? (
        <InspectorSection defaultExpanded id="duration" label="Duration">
          <InspectorRow label="Planned duration" value={formatDuration(model.durationSeconds)} />
        </InspectorSection>
      ) : null,
    inputs: model.inputs ? (
      <InspectorSection defaultExpanded id="inputs" label={model.inputs.label}>
        <RelatedBuildsBody slice={model.inputs} />
      </InspectorSection>
    ) : null,
    usedBy: model.usedBy ? (
      <InspectorSection defaultExpanded id="usedBy" label={model.usedBy.label}>
        <RelatedBuildsBody slice={model.usedBy} />
      </InspectorSection>
    ) : null,
    value: model.value ? (
      <InspectorSection defaultExpanded id="value" label="Value">
        {model.value.metrics.map((metric) => (
          <InspectorRow key={metric.label} label={metric.label} tone={metric.tone} value={metric.value} />
        ))}
      </InspectorSection>
    ) : null,
    provenance: model.provenance ? (
      <InspectorSection defaultExpanded id="provenance" label="Provenance" summary={model.provenance.summary}>
        <ProvenanceBody slice={model.provenance} onCopyBuildId={actions.copyBuildId} />
      </InspectorSection>
    ) : null,
  };

  return (
    <div className="pb-2">
      <IdentityHeader model={model} onClose={actions.onClose} />
      {model.statusLine ? (
        <p
          className={`border-b border-border px-3 py-1.5 text-[11px] ${
            model.statusLine.tone === "blocking" ? "text-danger" : "text-muted"
          }`}
          role="status"
        >
          {model.statusLine.text}
        </p>
      ) : null}
      <WarningBadges warnings={model.warnings} />
      {INSPECTOR_SECTION_ORDER.map((id) => (sections[id] ? <div key={id}>{sections[id]}</div> : null))}
      {actions.openLinkedBuild && !readOnly ? (
        <div className="px-3 py-2">
          <button
            aria-label={`Open linked build for ${model.identity.name}`}
            className="iw-button-secondary w-full"
            onClick={actions.openLinkedBuild}
            type="button"
          >
            Open linked build
          </button>
        </div>
      ) : null}
      {actions.calculationEvidence ? (
        <details className="border-t border-border px-3 pt-3">
          <summary className="cursor-pointer text-xs font-semibold">Exact calculation evidence</summary>
          <div className="mt-3">{actions.calculationEvidence}</div>
        </details>
      ) : null}
      {readOnly ? null : actions.footer}
    </div>
  );
}
