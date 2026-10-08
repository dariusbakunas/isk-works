//! Constructing and round-tripping facility profiles: identity derivation and
//! matching, command parsing/validation into a persisted profile, and the
//! inverse export back to a command. CSV/profile-format semantics are
//! byte-for-byte compatible with the pre-split code.

use std::collections::BTreeSet;
use std::str::FromStr;

use chrono::Utc;
use rust_decimal::Decimal;

use crate::{Money, WorkspaceId};

use super::rig_applicability::RigApplicability;
use super::types::{
    CreateFacilityProfileCommand, FacilityError, FacilityIdentity, FacilityIdentityBasis,
    FacilityKind, FacilityProfileId, FacilityRig, FacilityRigInput, IndustryFacilityProfile,
};

pub fn facility_identity(
    command: &CreateFacilityProfileCommand,
) -> Result<(FacilityIdentity, FacilityIdentityBasis), FacilityError> {
    match command.kind {
        FacilityKind::NpcStation | FacilityKind::UpwellStructure => command
            .structure_id
            .map(|location_id| {
                (
                    FacilityIdentity::EveLocation {
                        role: command.role,
                        location_id,
                    },
                    FacilityIdentityBasis::EveLocation,
                )
            })
            .ok_or_else(|| {
                FacilityError::Validation(
                    "EVE-backed facilities require a structure or station ID.".into(),
                )
            }),
        FacilityKind::Manual => {
            let solar_system_id = command.solar_system_id.ok_or_else(|| {
                FacilityError::Validation("Manual facilities require a solar system.".into())
            })?;
            let normalized_name = command
                .name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if normalized_name.is_empty() {
                return Err(FacilityError::Validation(
                    "Manual facilities require a name.".into(),
                ));
            }
            Ok((
                FacilityIdentity::Manual {
                    role: command.role,
                    solar_system_id,
                    normalized_name,
                },
                FacilityIdentityBasis::ManualName,
            ))
        }
    }
}

pub fn find_active_facility_match<'a>(
    command: &CreateFacilityProfileCommand,
    profiles: &'a [IndustryFacilityProfile],
) -> Result<Option<(&'a IndustryFacilityProfile, FacilityIdentityBasis)>, FacilityError> {
    let (wanted, basis) = facility_identity(command)?;
    Ok(profiles.iter().find_map(|profile| {
        if profile.archived_at.is_some() {
            return None;
        }
        let existing = facility_identity(&export_command(profile)).ok()?.0;
        (existing == wanted).then_some((profile, basis))
    }))
}

pub fn parse_profile(
    workspace_id: WorkspaceId,
    command: CreateFacilityProfileCommand,
) -> Result<IndustryFacilityProfile, FacilityError> {
    let name = command.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(FacilityError::Validation(
            "Facility name must contain 1 to 120 characters.".to_string(),
        ));
    }
    if command.kind == FacilityKind::UpwellStructure
        && command.structure_id.map_or(true, |value| value <= 0)
    {
        return Err(FacilityError::Validation(
            "An Upwell Structure requires a positive EVE structure ID.".to_string(),
        ));
    }
    let mut slots = BTreeSet::new();
    let rigs = command
        .rigs
        .into_iter()
        .map(|rig| {
            if !(1..=3).contains(&rig.slot_number)
                || rig.type_id <= 0
                || rig.type_name.trim().is_empty()
                || !slots.insert(rig.slot_number)
            {
                return Err(FacilityError::Validation(
                    "Each rig requires a unique slot 1-3 and a valid EVE type.".to_string(),
                ));
            }
            Ok(FacilityRig {
                slot_number: rig.slot_number,
                type_id: rig.type_id,
                type_name: rig.type_name.trim().to_string(),
                material_reduction_percent: parse_percent(&rig.material_reduction_percent, false)?,
                time_reduction_percent: parse_percent(&rig.time_reduction_percent, false)?,
                // Resolved from the SDE by the facility route after parsing,
                // not carried on the wire command -- see `RigApplicability`.
                applicability: RigApplicability::default(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let now = Utc::now();
    Ok(IndustryFacilityProfile {
        id: FacilityProfileId::new(),
        workspace_id,
        name: name.to_string(),
        kind: command.kind,
        role: command.role,
        structure_id: command.structure_id,
        structure_type_id: command.structure_type_id,
        structure_type_name: command.structure_type_name.trim().to_string(),
        solar_system_id: command.solar_system_id,
        solar_system_name: command.solar_system_name.trim().to_string(),
        security_class: command.security_class,
        material_reduction_percent: parse_percent(&command.material_reduction_percent, false)?,
        time_reduction_percent: parse_percent(&command.time_reduction_percent, false)?,
        job_cost_reduction_percent: parse_percent(&command.job_cost_reduction_percent, false)?,
        facility_tax_percent: parse_percent(&command.facility_tax_percent, true)?,
        scc_surcharge_percent: parse_percent(&command.scc_surcharge_percent, true)?,
        alliance_surcharge_percent: parse_percent(&command.alliance_surcharge_percent, true)?,
        fixed_supplemental_cost: if command.fixed_supplemental_cost.trim().is_empty() {
            Money::zero()
        } else {
            Money::parse(&command.fixed_supplemental_cost)
                .map_err(|error| FacilityError::Validation(error.to_string()))?
        },
        manual_system_cost_index: command
            .manual_system_cost_index
            .as_deref()
            .map(parse_index)
            .transpose()?,
        notes: command.notes.trim().to_string(),
        rigs,
        archived_at: None,
        revision: 1,
        created_at: now,
        updated_at: now,
    })
}

/// The inverse of `parse_profile`: reduces a persisted profile back to the
/// same command shape used to create it, for export/import. Money.0 is a
/// `Decimal` already scaled to the required precision, so `.to_string()`
/// round-trips cleanly through `parse_percent`/`Money::parse` on re-import.
#[must_use]
pub fn export_command(profile: &IndustryFacilityProfile) -> CreateFacilityProfileCommand {
    CreateFacilityProfileCommand {
        name: profile.name.clone(),
        kind: profile.kind,
        role: profile.role,
        structure_id: profile.structure_id,
        structure_type_id: profile.structure_type_id,
        structure_type_name: profile.structure_type_name.clone(),
        solar_system_id: profile.solar_system_id,
        solar_system_name: profile.solar_system_name.clone(),
        security_class: profile.security_class,
        material_reduction_percent: profile.material_reduction_percent.to_string(),
        time_reduction_percent: profile.time_reduction_percent.to_string(),
        job_cost_reduction_percent: profile.job_cost_reduction_percent.to_string(),
        facility_tax_percent: profile.facility_tax_percent.to_string(),
        scc_surcharge_percent: profile.scc_surcharge_percent.to_string(),
        alliance_surcharge_percent: profile.alliance_surcharge_percent.to_string(),
        fixed_supplemental_cost: profile.fixed_supplemental_cost.0.to_string(),
        manual_system_cost_index: profile
            .manual_system_cost_index
            .map(|index| index.to_string()),
        notes: profile.notes.clone(),
        rigs: profile
            .rigs
            .iter()
            .map(|rig| FacilityRigInput {
                slot_number: rig.slot_number,
                type_id: rig.type_id,
                type_name: rig.type_name.clone(),
                material_reduction_percent: rig.material_reduction_percent.to_string(),
                time_reduction_percent: rig.time_reduction_percent.to_string(),
            })
            .collect(),
    }
}

fn parse_percent(value: &str, allow_hundred: bool) -> Result<Decimal, FacilityError> {
    let value = if value.trim().is_empty() {
        Decimal::ZERO
    } else {
        Decimal::from_str(value.trim())
            .map_err(|_| FacilityError::Validation("Expected a decimal percentage.".into()))?
    };
    let maximum = if allow_hundred {
        Decimal::ONE_HUNDRED
    } else {
        Decimal::from_str("99.999999").expect("valid constant")
    };
    if value.is_sign_negative() || value > maximum || value.normalize().scale() > 6 {
        return Err(FacilityError::Validation(
            "Percentage is outside the supported range or precision.".into(),
        ));
    }
    Ok(value)
}

fn parse_index(value: &str) -> Result<Decimal, FacilityError> {
    let value = Decimal::from_str(value.trim())
        .map_err(|_| FacilityError::Validation("Cost index must be a decimal.".into()))?;
    if value.is_sign_negative() || value > Decimal::ONE || value.normalize().scale() > 10 {
        return Err(FacilityError::Validation(
            "Cost index must be between 0 and 1.".into(),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facility::tests_common::fixture_profile;
    use crate::facility::{FacilityRole, SecurityClass};
    use uuid::Uuid;

    #[test]
    fn eve_facility_identity_uses_role_and_location_id() {
        let mut existing = fixture_profile("0", "0");
        existing.kind = FacilityKind::UpwellStructure;
        existing.structure_id = Some(1_050_474_463_169);
        existing.name = "Original structure name".into();

        let mut renamed = export_command(&existing);
        renamed.name = "Updated structure name".into();
        let profiles = [existing.clone()];
        let matched = find_active_facility_match(&renamed, &profiles).unwrap();
        assert_eq!(matched.map(|(profile, _)| profile.id), Some(existing.id));

        renamed.role = FacilityRole::Reaction;
        assert!(find_active_facility_match(&renamed, &[existing])
            .unwrap()
            .is_none());
    }

    #[test]
    fn manual_facility_identity_normalizes_name_and_requires_system() {
        let mut existing = fixture_profile("0", "0");
        existing.kind = FacilityKind::Manual;
        existing.name = "  GEZ   Manufacturing ".into();
        existing.solar_system_id = Some(30_002_099);

        let mut equivalent = export_command(&existing);
        equivalent.name = "gez manufacturing".into();
        let profiles = [existing.clone()];
        let matched = find_active_facility_match(&equivalent, &profiles).unwrap();
        assert_eq!(
            matched.map(|(_, basis)| basis),
            Some(FacilityIdentityBasis::ManualName)
        );

        equivalent.solar_system_id = None;
        assert!(matches!(
            facility_identity(&equivalent),
            Err(FacilityError::Validation(_))
        ));
    }

    #[test]
    fn archived_profiles_are_excluded_from_matches() {
        let mut existing = fixture_profile("0", "0");
        existing.kind = FacilityKind::NpcStation;
        existing.structure_id = Some(60_003_760);
        existing.archived_at = Some(Utc::now());
        let command = export_command(&existing);

        assert!(find_active_facility_match(&command, &[existing])
            .unwrap()
            .is_none());
    }

    #[test]
    fn export_command_round_trips_through_parse_profile() {
        let workspace_id = WorkspaceId(Uuid::nil());
        let original = parse_profile(
            workspace_id,
            CreateFacilityProfileCommand {
                name: "Raitaru".into(),
                kind: FacilityKind::UpwellStructure,
                role: FacilityRole::Manufacturing,
                structure_id: Some(1_234_567_890),
                structure_type_id: Some(35_825),
                structure_type_name: "Raitaru".into(),
                solar_system_id: Some(30_000_142),
                solar_system_name: "Jita".into(),
                security_class: SecurityClass::HighSec,
                material_reduction_percent: "1".into(),
                time_reduction_percent: "15".into(),
                job_cost_reduction_percent: "3".into(),
                facility_tax_percent: "1.5".into(),
                scc_surcharge_percent: "0.5".into(),
                alliance_surcharge_percent: "0".into(),
                fixed_supplemental_cost: "1000".into(),
                manual_system_cost_index: Some("0.048".into()),
                notes: "Home structure".into(),
                rigs: vec![FacilityRigInput {
                    slot_number: 1,
                    type_id: 43_920,
                    type_name: "Standup M-Set Basic Material Efficiency I".into(),
                    material_reduction_percent: "2".into(),
                    time_reduction_percent: "0".into(),
                }],
            },
        )
        .unwrap();

        let reimported = parse_profile(workspace_id, export_command(&original)).unwrap();

        assert_eq!(reimported.name, original.name);
        assert_eq!(reimported.kind, original.kind);
        assert_eq!(reimported.role, original.role);
        assert_eq!(reimported.structure_id, original.structure_id);
        assert_eq!(reimported.structure_type_name, original.structure_type_name);
        assert_eq!(reimported.solar_system_name, original.solar_system_name);
        assert_eq!(reimported.security_class, original.security_class);
        assert_eq!(
            reimported.material_reduction_percent,
            original.material_reduction_percent
        );
        assert_eq!(
            reimported.time_reduction_percent,
            original.time_reduction_percent
        );
        assert_eq!(
            reimported.job_cost_reduction_percent,
            original.job_cost_reduction_percent
        );
        assert_eq!(
            reimported.facility_tax_percent,
            original.facility_tax_percent
        );
        assert_eq!(
            reimported.scc_surcharge_percent,
            original.scc_surcharge_percent
        );
        assert_eq!(
            reimported.fixed_supplemental_cost,
            original.fixed_supplemental_cost
        );
        assert_eq!(
            reimported.manual_system_cost_index,
            original.manual_system_cost_index
        );
        assert_eq!(reimported.notes, original.notes);
        assert_eq!(reimported.rigs, original.rigs);
        // id/revision/created_at/updated_at/archived_at are intentionally
        // fresh on every import — a new profile, not a copy of the old row.
        assert_ne!(reimported.id, original.id);
        assert_eq!(reimported.revision, 1);
    }

    #[test]
    fn export_command_carries_empty_rigs_and_present_manual_cost_index() {
        let profile = fixture_profile("0", "0");

        let command = export_command(&profile);

        assert!(command.rigs.is_empty());
        assert_eq!(command.manual_system_cost_index, Some("0.05".to_string()));
    }
}
