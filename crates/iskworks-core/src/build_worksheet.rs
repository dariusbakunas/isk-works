//! Read-only saved-Build Worksheet projection.
//!
//! This module translates the already-computed whole-root planning,
//! allocation, and cost evidence. It never expands recipes, allocates
//! inventory, derives runs, looks up prices, or calculates production cost.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::build_materials::{
    AggregateMaterialLine, MaterialBoundaryResolution, NodeMaterialAllocation,
    VerificationBoundaryInput, VerificationOperationInput,
};
use crate::{
    BuildCostProjection, BuildId, CostWarning, MarketPricingPolicy, Money, PricingSelectionKind,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWorksheetProjection {
    pub scope: BuildWorksheetScope,
    pub groups: Vec<BuildWorksheetGroup>,
    pub output: BuildWorksheetOutput,
    pub warnings: Vec<CostWarning>,
    pub economics_are_additive: bool,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWorksheetScope {
    pub root_build_id: BuildId,
    pub focused_producer_id: Option<BuildId>,
    /// `false` limits rows to what the selected operation's own recipe needs
    /// directly; `true` also includes every downstream dependency.
    pub include_downstream: bool,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWorksheetGroup {
    pub key: String,
    pub label: String,
    pub row_count: usize,
    pub complete: bool,
    pub rows: Vec<BuildWorksheetRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWorksheetRow {
    pub id: String,
    pub type_id: i64,
    pub type_name: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub sourcing: WorksheetSourcing,
    pub required_quantity: Option<u64>,
    pub covered_quantity: Option<u64>,
    pub shortage_quantity: Option<u64>,
    pub coverage_percentage: Option<String>,
    pub evidence_state: WorksheetEvidenceState,
    pub pricing: WorksheetPricingEvidence,
    pub unit_cost: Option<Money>,
    pub total_value: Option<Money>,
    pub producer_build_id: Option<BuildId>,
    pub retained_surplus_quantity: Option<u64>,
    pub retained_surplus_basis: Option<Money>,
    pub warnings: Vec<CostWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWorksheetOutput {
    pub type_id: i64,
    pub type_name: String,
    pub quantity: Option<u64>,
    pub unit_value: Option<Money>,
    pub total_value: Option<Money>,
    pub evidence_state: WorksheetEvidenceState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorksheetSourcing {
    Buy,
    Manufacturing,
    Reaction,
    Unresolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorksheetEvidenceState {
    Complete,
    Unpriced,
    Incomplete,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksheetPricingEvidence {
    pub state: WorksheetEvidenceState,
    pub classification: WorksheetPricingClassification,
    pub policy: Option<MarketPricingPolicy>,
    pub source_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorksheetPricingClassification {
    Production,
    Default,
    Manual,
    MarketPolicy,
    Mixed,
    Unresolved,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorksheetTypeMetadata {
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
}

impl WorksheetTypeMetadata {
    #[must_use]
    pub fn new(category_name: impl Into<String>, group_name: impl Into<String>) -> Self {
        Self {
            category_id: None,
            category_name: Some(category_name.into()),
            group_id: None,
            group_name: Some(group_name.into()),
        }
    }
}

pub struct BuildWorksheetProjectionInput<'a> {
    pub operations: &'a [VerificationOperationInput],
    pub boundaries: &'a [VerificationBoundaryInput],
    pub allocations: &'a [NodeMaterialAllocation],
    pub aggregate_rows: &'a [AggregateMaterialLine],
    pub cost: &'a BuildCostProjection,
    pub focused_producer_id: Option<BuildId>,
    pub include_downstream: bool,
    pub metadata: &'a BTreeMap<i64, WorksheetTypeMetadata>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildWorksheetProjectionError {
    #[error("the authoritative plan has no root operation")]
    MissingRoot,
    #[error("the focused producer does not belong to this root plan")]
    UnknownFocus,
    #[error("worksheet quantity aggregation overflowed")]
    QuantityOverflow,
    #[error("worksheet money aggregation overflowed")]
    MoneyOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RowKey {
    type_id: i64,
    sourcing: WorksheetSourcing,
    producer_build_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone)]
struct RowAccumulator {
    type_name: String,
    required: u64,
    covered: u64,
    shortage: u64,
    cost_complete: bool,
    total_value: Money,
    unit_cost: Option<Money>,
    unit_cost_consistent: bool,
    retained_surplus_quantity: u64,
    retained_surplus_basis: Money,
    surplus_basis_complete: bool,
    pricing_note: Option<String>,
    pricing_classification: Option<WorksheetPricingClassification>,
    pricing_policy: Option<MarketPricingPolicy>,
    pricing_consistent: bool,
    warnings: Vec<CostWarning>,
}

pub fn project_build_worksheet(
    input: BuildWorksheetProjectionInput<'_>,
) -> Result<BuildWorksheetProjection, BuildWorksheetProjectionError> {
    let _ = (input.boundaries, input.aggregate_rows);
    let root = input
        .operations
        .iter()
        .find(|operation| operation.op_index == 0)
        .ok_or(BuildWorksheetProjectionError::MissingRoot)?;
    let focused_operation = input
        .focused_producer_id
        .map(|focused_id| {
            input
                .operations
                .iter()
                .find(|operation| operation.build_id == focused_id)
                .ok_or(BuildWorksheetProjectionError::UnknownFocus)
        })
        .transpose()?;
    let selected_operation = focused_operation.unwrap_or(root);
    let included_builds = focused_operation
        .filter(|_| input.include_downstream)
        .map(|focused| focused_subtree(input.operations, focused));

    let mut accumulated = BTreeMap::<RowKey, RowAccumulator>::new();
    for allocation in input.allocations {
        if !input.include_downstream && allocation.graph_node_id != selected_operation.graph_node_id
        {
            continue;
        }
        if included_builds
            .as_ref()
            .is_some_and(|builds| !builds.contains(&allocation.build_id.0))
        {
            continue;
        }
        let sourcing = sourcing(allocation.resolution);
        let key = RowKey {
            type_id: allocation.type_id,
            sourcing,
            producer_build_id: allocation.producer_build_id.map(|build_id| build_id.0),
        };
        let row = accumulated.entry(key).or_insert_with(|| RowAccumulator {
            type_name: allocation.type_name.clone(),
            required: 0,
            covered: 0,
            shortage: 0,
            cost_complete: true,
            total_value: Money::zero(),
            unit_cost: None,
            unit_cost_consistent: true,
            retained_surplus_quantity: 0,
            retained_surplus_basis: Money::zero(),
            surplus_basis_complete: true,
            pricing_note: None,
            pricing_classification: None,
            pricing_policy: None,
            pricing_consistent: true,
            warnings: Vec::new(),
        });
        row.required = row
            .required
            .checked_add(allocation.required_quantity)
            .ok_or(BuildWorksheetProjectionError::QuantityOverflow)?;
        row.covered = row
            .covered
            .checked_add(allocation.allocated_quantity)
            .ok_or(BuildWorksheetProjectionError::QuantityOverflow)?;
        row.shortage = row
            .shortage
            .checked_add(allocation.shortage_quantity)
            .ok_or(BuildWorksheetProjectionError::QuantityOverflow)?;

        let boundary = input
            .operations
            .iter()
            .find(|operation| operation.graph_node_id == allocation.graph_node_id)
            .and_then(|operation| {
                input.cost.boundaries.iter().find(|boundary| {
                    boundary.op_index == operation.op_index
                        && boundary.type_id == allocation.type_id
                        && boundary.resolution == allocation.resolution
                })
            });
        let Some(boundary) = boundary else {
            row.cost_complete = false;
            continue;
        };
        row.warnings.extend(boundary.warnings.iter().cloned());
        row.pricing_note = (!boundary.fresh_price_note.is_empty())
            .then(|| boundary.fresh_price_note.clone())
            .or(row.pricing_note.take());
        let (classification, policy) = pricing_classification(
            allocation.resolution,
            boundary.fresh_price_selection,
            boundary.fresh_pricing_policy,
        );
        match row.pricing_classification {
            None => {
                row.pricing_classification = Some(classification);
                row.pricing_policy = policy;
            }
            Some(existing) if existing != classification || row.pricing_policy != policy => {
                row.pricing_consistent = false;
                row.pricing_policy = None;
            }
            _ => {}
        }
        if !boundary.complete {
            row.cost_complete = false;
        }
        if let Some(value) = boundary.requirement_cost {
            row.total_value = row
                .total_value
                .checked_add(value)
                .map_err(|_| BuildWorksheetProjectionError::MoneyOverflow)?;
        } else {
            row.cost_complete = false;
        }
        let boundary_unit_cost = match allocation.resolution {
            // A Buy boundary's authoritative display unit cost is blended
            // inventory basis + fresh acquisition cost over the whole
            // requirement, matching BuildCostProjection::apply_to_revision.
            // Never show the fresh market quote as though every unit were
            // bought at that price.
            MaterialBoundaryResolution::Buy => boundary
                .requirement_cost
                .and_then(|total| total.checked_div_quantity(boundary.required_quantity).ok()),
            MaterialBoundaryResolution::Build | MaterialBoundaryResolution::Reaction => {
                boundary.child_unit_production_cost
            }
            MaterialBoundaryResolution::Unresolved => None,
        };
        match (row.unit_cost, boundary_unit_cost) {
            (None, Some(value)) => row.unit_cost = Some(value),
            (Some(existing), Some(value)) if existing != value => row.unit_cost_consistent = false,
            (_, None) => row.unit_cost_consistent = false,
            _ => {}
        }
        row.retained_surplus_quantity = row
            .retained_surplus_quantity
            .checked_add(boundary.child_surplus_quantity)
            .ok_or(BuildWorksheetProjectionError::QuantityOverflow)?;
        if boundary.child_surplus_quantity > 0 {
            if let Some(basis) = boundary.child_surplus_retained_basis {
                row.retained_surplus_basis = row
                    .retained_surplus_basis
                    .checked_add(basis)
                    .map_err(|_| BuildWorksheetProjectionError::MoneyOverflow)?;
            } else {
                row.surplus_basis_complete = false;
            }
        }
    }

    let mut grouped = BTreeMap::<(String, String), Vec<BuildWorksheetRow>>::new();
    for (key, values) in accumulated {
        let metadata = input
            .metadata
            .get(&key.type_id)
            .cloned()
            .unwrap_or_default();
        let group_label = metadata
            .group_name
            .clone()
            .unwrap_or_else(|| "Other".into());
        let group_key = metadata.group_id.map_or_else(
            || format!("group:{group_label}"),
            |group_id| format!("group:{group_id}"),
        );
        let row_id = format!(
            "{}:{}:{}",
            key.type_id,
            sourcing_key(key.sourcing),
            key.producer_build_id
                .map_or_else(|| "none".into(), |build_id| build_id.to_string())
        );
        let quantity_complete = values.required == values.covered.saturating_add(values.shortage);
        let evidence_state = if quantity_complete && values.cost_complete {
            WorksheetEvidenceState::Complete
        } else {
            WorksheetEvidenceState::Incomplete
        };
        grouped
            .entry((group_label, group_key))
            .or_default()
            .push(BuildWorksheetRow {
                id: row_id,
                type_id: key.type_id,
                type_name: values.type_name,
                category_id: metadata.category_id,
                category_name: metadata.category_name,
                group_id: metadata.group_id,
                group_name: metadata.group_name,
                sourcing: key.sourcing,
                required_quantity: Some(values.required),
                covered_quantity: Some(values.covered),
                shortage_quantity: Some(values.shortage),
                coverage_percentage: coverage_percentage(values.covered, values.required),
                evidence_state,
                pricing: WorksheetPricingEvidence {
                    state: if values.cost_complete {
                        WorksheetEvidenceState::Complete
                    } else {
                        WorksheetEvidenceState::Incomplete
                    },
                    classification: if values.pricing_consistent {
                        values
                            .pricing_classification
                            .unwrap_or(WorksheetPricingClassification::Unresolved)
                    } else {
                        WorksheetPricingClassification::Mixed
                    },
                    policy: values
                        .pricing_consistent
                        .then_some(values.pricing_policy)
                        .flatten(),
                    source_note: values.pricing_note,
                },
                unit_cost: if key.sourcing == WorksheetSourcing::Buy
                    && values.cost_complete
                    && values.required > 0
                {
                    Some(
                        values
                            .total_value
                            .checked_div_quantity(values.required)
                            .map_err(|_| BuildWorksheetProjectionError::MoneyOverflow)?,
                    )
                } else {
                    values
                        .unit_cost_consistent
                        .then_some(values.unit_cost)
                        .flatten()
                },
                total_value: values.cost_complete.then_some(values.total_value),
                producer_build_id: key.producer_build_id.map(BuildId),
                retained_surplus_quantity: (values.retained_surplus_quantity > 0)
                    .then_some(values.retained_surplus_quantity),
                retained_surplus_basis: (values.retained_surplus_quantity > 0
                    && values.surplus_basis_complete)
                    .then_some(values.retained_surplus_basis),
                warnings: values.warnings,
            });
    }

    let groups = grouped
        .into_iter()
        .map(|((label, key), mut rows)| {
            rows.sort_by(|left, right| {
                left.type_name
                    .cmp(&right.type_name)
                    .then(left.id.cmp(&right.id))
            });
            BuildWorksheetGroup {
                key,
                label,
                row_count: rows.len(),
                complete: rows
                    .iter()
                    .all(|row| row.evidence_state == WorksheetEvidenceState::Complete),
                rows,
            }
        })
        .collect();

    let output_cost = input
        .cost
        .operations
        .iter()
        .find(|operation| operation.op_index == selected_operation.op_index);
    Ok(BuildWorksheetProjection {
        scope: BuildWorksheetScope {
            root_build_id: root.build_id,
            focused_producer_id: focused_operation.map(|operation| operation.build_id),
            include_downstream: input.include_downstream,
            label: selected_operation.product_name.clone(),
        },
        groups,
        output: BuildWorksheetOutput {
            type_id: selected_operation.product_type_id,
            type_name: selected_operation.product_name.clone(),
            quantity: output_cost.map(|operation| operation.produced_quantity),
            unit_value: output_cost.and_then(|operation| operation.unit_production_cost),
            total_value: output_cost.and_then(|operation| operation.total_production_cost),
            evidence_state: output_cost.map_or(WorksheetEvidenceState::Incomplete, |operation| {
                if operation.complete {
                    WorksheetEvidenceState::Complete
                } else {
                    WorksheetEvidenceState::Incomplete
                }
            }),
        },
        warnings: input.cost.warnings.clone(),
        economics_are_additive: false,
        generated_at: input.generated_at,
    })
}

fn pricing_classification(
    resolution: MaterialBoundaryResolution,
    selection: PricingSelectionKind,
    policy: Option<MarketPricingPolicy>,
) -> (WorksheetPricingClassification, Option<MarketPricingPolicy>) {
    match resolution {
        MaterialBoundaryResolution::Build | MaterialBoundaryResolution::Reaction => {
            (WorksheetPricingClassification::Production, None)
        }
        MaterialBoundaryResolution::Unresolved => {
            (WorksheetPricingClassification::Unresolved, None)
        }
        MaterialBoundaryResolution::Buy => match selection {
            PricingSelectionKind::Default => (WorksheetPricingClassification::Default, None),
            PricingSelectionKind::Manual => (WorksheetPricingClassification::Manual, None),
            PricingSelectionKind::MarketPolicy => {
                (WorksheetPricingClassification::MarketPolicy, policy)
            }
        },
    }
}

fn focused_subtree(
    operations: &[VerificationOperationInput],
    focused: &VerificationOperationInput,
) -> BTreeSet<uuid::Uuid> {
    let mut op_indices = BTreeSet::from([focused.op_index]);
    let mut builds = BTreeSet::from([focused.build_id.0]);
    loop {
        let mut changed = false;
        for operation in operations {
            // A producer with several incoming consumers is a shared operation,
            // not an attributable private descendant of whichever consumer is
            // listed first. It enters scope only when it is itself focused.
            if operation.incoming.len() > 1 {
                continue;
            }
            if operation
                .parent_op_index
                .is_some_and(|parent| op_indices.contains(&parent))
                && op_indices.insert(operation.op_index)
            {
                builds.insert(operation.build_id.0);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    builds
}

fn sourcing(resolution: MaterialBoundaryResolution) -> WorksheetSourcing {
    match resolution {
        MaterialBoundaryResolution::Buy => WorksheetSourcing::Buy,
        MaterialBoundaryResolution::Build => WorksheetSourcing::Manufacturing,
        MaterialBoundaryResolution::Reaction => WorksheetSourcing::Reaction,
        MaterialBoundaryResolution::Unresolved => WorksheetSourcing::Unresolved,
    }
}

fn sourcing_key(sourcing: WorksheetSourcing) -> &'static str {
    match sourcing {
        WorksheetSourcing::Buy => "buy",
        WorksheetSourcing::Manufacturing => "manufacturing",
        WorksheetSourcing::Reaction => "reaction",
        WorksheetSourcing::Unresolved => "unresolved",
    }
}

fn coverage_percentage(covered: u64, required: u64) -> Option<String> {
    if required == 0 {
        return None;
    }
    let percentage =
        (Decimal::from(covered) * Decimal::from(100_u64) / Decimal::from(required)).round_dp(2);
    Some(format!("{percentage:.2}"))
}

#[cfg(test)]
mod tests;
