//! Engineering / audit `.xlsx` export for a Build.
//!
//! This module turns the **authoritative** ISKWorks Build projections
//! (`BuildPlanRevision` from the build-plan preview + `BuildMaterialsSummary`
//! from the whole-tree inventory allocator) into a spreadsheet whose backend
//! values can be independently re-checked with plain Excel formulas.
//!
//! It never re-implements industry math: every quantity written into the
//! workbook comes unchanged from a projection the product already computes.
//! The Excel formulas only reconstruct simple *relationships* between those
//! exported per-boundary values (shortage arithmetic, discrete child-run
//! sizing, produced / surplus) and flag any mismatch.
//!
//! Read-only: assembling the model performs exactly one materials projection
//! (the canonical projection of the Build's plan, whose one `list_balances`
//! is the only inventory read) plus one bulk SDE `type_reference` read. Nothing is
//! persisted.

mod verification_workbook;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use iskworks_app::BuildMaterialsSummary;
use iskworks_core::build_cost::BuildCostProjection;
use iskworks_core::Build;
use iskworks_sde::TypeReference;
use rust_decimal::Decimal;
use uuid::Uuid;

pub(crate) use verification_workbook::build_verification_workbook;

/// The complete, self-contained input to [`build_verification_workbook`].
///
/// Assembled from `IndustryRepository::get_build` (identity),
/// `BuildMaterialsCoordinator::materials_with_planning_cost` (every
/// quantity, the per-node and per-boundary verification evidence, the
/// inventory-basis snapshot, the allocation-aware [`BuildCostProjection`],
/// and the raw adjusted-price map, all from one allocation-aware walk), and
/// `SdeReadRepository::type_reference` (the `Types` dictionary).
#[derive(Debug, Clone)]
pub(crate) struct VerificationExportModel {
    pub build_id: Uuid,
    pub build_name: String,
    /// Runs from the live editor overlay -- the root operation's run count.
    pub overlay_runs: u64,
    pub materials: BuildMaterialsSummary,
    /// The allocation-aware planning-cost projection over the same walk that
    /// produced `materials` -- comparison evidence only; every Excel cost
    /// formula must be reconstructible from `materials`'s primitives and
    /// `adjusted_prices`, never from this struct directly.
    pub cost: BuildCostProjection,
    /// The bulk-resolved adjusted-price map (`type_id` -> price) used for
    /// every operation's EIV -- exposed on the `Types` sheet as primitive,
    /// independently-checkable evidence.
    pub adjusted_prices: BTreeMap<i64, Decimal>,
    /// Bulk SDE metadata for every `type_id` the workbook references -- the
    /// `Types` sheet and the source for every INDEX/MATCH name lookup.
    pub type_reference: BTreeMap<i64, TypeReference>,
    pub generated_at: DateTime<Utc>,
}

impl VerificationExportModel {
    pub(crate) fn assemble(
        build: &Build,
        overlay_runs: u64,
        materials: BuildMaterialsSummary,
        cost: BuildCostProjection,
        adjusted_prices: BTreeMap<i64, Decimal>,
        type_reference: BTreeMap<i64, TypeReference>,
        generated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            build_id: build.id.0,
            build_name: build.name.clone(),
            overlay_runs,
            materials,
            cost,
            adjusted_prices,
            type_reference,
            generated_at,
        }
    }
}

/// Guard a user-controlled text cell against spreadsheet formula injection.
///
/// A value beginning with `=`, `+`, `-`, `@` (or a leading control character
/// Excel treats the same way) is prefixed with a single quote so Excel /
/// LibreOffice render it as literal text instead of evaluating it. Applied
/// only to free-text cells the exporter fills from user input (Build name,
/// notes, facility names); never to numeric cells or to formulas the
/// exporter itself authored.
pub(crate) fn sanitize_cell_text(value: &str) -> String {
    let needs_guard = value
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r' | '\n'));
    if needs_guard {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// Turn a Build name into a safe download filename stem: keep alphanumerics,
/// spaces, `-`, `_`, `.`; collapse every other run to a single `_`; trim; cap
/// length; fall back to `build` when nothing usable remains.
pub(crate) fn sanitize_filename_stem(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_sep = false;
    for ch in name.chars() {
        if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.') {
            out.push(ch);
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }
    let trimmed = out.trim_matches([' ', '_', '.', '-']).trim();
    let capped: String = trimmed.chars().take(80).collect();
    let capped = capped.trim_matches([' ', '_', '.', '-']).trim();
    if capped.is_empty() {
        "build".to_string()
    } else {
        capped.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_cell_text_guards_formula_leaders() {
        assert_eq!(sanitize_cell_text("=cmd|' /c calc'"), "'=cmd|' /c calc'");
        assert_eq!(sanitize_cell_text("+1+1"), "'+1+1");
        assert_eq!(sanitize_cell_text("-2"), "'-2");
        assert_eq!(sanitize_cell_text("@SUM"), "'@SUM");
        assert_eq!(sanitize_cell_text("\tTab"), "'\tTab");
    }

    #[test]
    fn sanitize_cell_text_leaves_ordinary_text_untouched() {
        assert_eq!(sanitize_cell_text("Sabre"), "Sabre");
        assert_eq!(sanitize_cell_text("Rifter Mk II"), "Rifter Mk II");
        assert_eq!(sanitize_cell_text(""), "");
    }

    #[test]
    fn sanitize_filename_stem_is_safe_and_stable() {
        assert_eq!(sanitize_filename_stem("Sabre"), "Sabre");
        assert_eq!(
            sanitize_filename_stem("Sabre / Interdictor"),
            "Sabre _ Interdictor"
        );
        assert_eq!(sanitize_filename_stem("../../etc/passwd"), "etc_passwd");
        assert_eq!(sanitize_filename_stem("   "), "build");
        assert_eq!(sanitize_filename_stem(""), "build");
        assert_eq!(sanitize_filename_stem("a\"b:c*d?e"), "a_b_c_d_e");
    }
}
