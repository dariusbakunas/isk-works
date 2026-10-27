//! EFT, the client's copy/paste fit format (pyfa writes it too), through
//! `esf-format`.

use esf_data::InfoNameSde;
use esf_dogma_engine::{Fit, Ship, Slot, State};
use iskworks_core::fitting::{
    EftError, EftLine, FitInput, FitItem, FitMutation, FittingError, ModuleState, ParsedFit, Pilot,
    SlotKind,
};

use super::calculate::{known_id, EngineItems};
use super::EsfFittingEngine;

/// The state suffixes a module line may end in. EVE writes `/OFFLINE`;
/// `esf-format` reads them in lower case only.
const STATE_SUFFIXES: [&str; 4] = ["/offline", "/online", "/active", "/overload"];

/// Modules pyfa imports online rather than active (its
/// `activeStateLimit`), by dogma effect: ones run for a moment or a
/// situation, like a cyno, a jump drive or a scanner, whose active effect
/// (a cyno pins the ship in place) shouldn't shape a fit's standing stats.
const ONLINE_ON_IMPORT: [&str; 21] = [
    "moduleBonusAssaultDamageControl",
    "moduleBonusIndustrialInvulnerability",
    "microJumpDrive",
    "microJumpPortalDrive",
    "emergencyHullEnergizer",
    "cynosuralGeneration",
    "jumpPortalGeneration",
    "jumpPortalGenerationBO",
    "cloneJumpAccepting",
    "cloakingWarpSafe",
    "cloakingPrototype",
    "cloaking",
    "massEntanglerEffect5",
    "electronicAttributeModifyOnline",
    "targetPassively",
    "cargoScan",
    "shipScan",
    "surveyScan",
    "targetSpectrumBreakerBonus",
    "interdictionNullifierBonus",
    "industrialItemCompression",
];

impl EsfFittingEngine {
    pub(super) fn read_eft(&self, text: &str) -> Result<ParsedFit, EftError> {
        let unknown = unknown_lines(text, |name| self.sde.type_name_to_id(name).is_some());
        if !unknown.is_empty() {
            return Err(EftError::UnknownLines(unknown));
        }
        let names = InfoNameSde::new(self.sde, None)
            .map_err(|error| EftError::Invalid(error.to_string()))?;
        let mut fit = esf_format::eft::load_eft(&names, &for_loader(text))
            .map_err(|error| EftError::Invalid(error.to_string()))?;

        // EVE's own import rules (cloaks online, group limits, how many
        // drones launch), with every skill so no drone stays in the bay for
        // want of one.
        fit.character.skills = self.skill_levels(&Pilot::AllV);
        esf_format::post_load(&self.info, &mut fit);
        for item in &mut fit.items {
            if ONLINE_ON_IMPORT
                .iter()
                .any(|effect| self.has_effect(item.type_id, effect))
            {
                item.state = item.state.min(State::Online);
            }
        }
        // EFT reads every module as active; a passive module or rig can't
        // go past online, so ask for what it can reach.
        let calculation = esf_dogma_engine::calculate(&self.info, &fit, &Default::default());
        for (item, result) in fit.items.iter_mut().zip(&calculation.items) {
            item.state = item.state.min(result.max_state);
        }

        Ok(ParsedFit {
            name: fit
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty() && *name != ".")
                .map(str::to_string),
            fit: FitInput {
                ship_type_id: i64::from(fit.ship.type_id),
                items: fit.items.iter().map(fit_item).collect(),
            },
        })
    }

    pub(super) fn save_eft(&self, name: &str, fit: &FitInput) -> Result<String, FittingError> {
        let ship_type_id = known_id(fit.ship_type_id)
            .filter(|id| self.is_known(*id))
            .ok_or(FittingError::UnknownShip(fit.ship_type_id))?;
        let EngineItems { items, .. } = self.engine_items(fit);
        let fit = Fit {
            name: Some(name.to_string()),
            ship: Ship {
                type_id: ship_type_id,
                mode: None,
            },
            items,
            character: Default::default(),
            environment: Default::default(),
            incoming: Default::default(),
        };
        esf_format::eft::save_eft(&self.info, &fit)
            .map(|text| client_state_suffixes(&text))
            .map_err(|_| FittingError::Export)
    }
}

/// `esf-format` pins every state with a suffix; the client only knows
/// ` /OFFLINE`, and reads the rest as part of the module's name. Keep
/// offline as the client writes it and drop the others: the client gives an
/// imported module its state itself.
fn client_state_suffixes(text: &str) -> String {
    let mut out: String = text
        .lines()
        .map(|line| match state_suffix(line) {
            Some(position) => {
                let name = line[..position].trim_end();
                if line[position..].trim_end().eq_ignore_ascii_case("/offline") {
                    format!("{name} /OFFLINE")
                } else {
                    name.to_string()
                }
            }
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn fit_item(item: &esf_dogma_engine::FitItem) -> FitItem {
    let (slot_kind, slot_index) = match item.slot {
        Slot::High(index) => (SlotKind::High, Some(i16::from(index))),
        Slot::Medium(index) => (SlotKind::Med, Some(i16::from(index))),
        Slot::Low(index) => (SlotKind::Low, Some(i16::from(index))),
        Slot::Rig(index) => (SlotKind::Rig, Some(i16::from(index))),
        Slot::Subsystem(index) => (SlotKind::Subsystem, Some(i16::from(index))),
        Slot::Service(index) => (SlotKind::Service, Some(i16::from(index))),
        Slot::FighterTube(index) => (SlotKind::FighterTube, Some(i16::from(index))),
        Slot::Implant(index) => (SlotKind::Implant, Some(i16::from(index))),
        Slot::Booster(index) => (SlotKind::Booster, i16::try_from(index).ok()),
        Slot::FighterBay => (SlotKind::FighterBay, None),
        Slot::DroneBay => (SlotKind::DroneBay, None),
        Slot::Cargo => (SlotKind::Cargo, None),
    };
    FitItem {
        type_id: i64::from(item.type_id),
        slot_kind,
        slot_index,
        quantity: item.quantity,
        state: match item.state {
            State::Offline => ModuleState::Offline,
            State::Online => ModuleState::Online,
            State::Active => ModuleState::Active,
            State::Overload => ModuleState::Overload,
        },
        charge_type_id: item.charge.as_ref().map(|charge| i64::from(charge.type_id)),
        mutation: item.mutation.as_ref().map(|mutation| FitMutation {
            base_type_id: i64::from(mutation.base),
            attributes: mutation
                .attributes
                .iter()
                .map(|(id, value)| (i64::from(*id), *value))
                .collect(),
        }),
    }
}

/// The text as `esf-format` reads it: each line trimmed, since it splits
/// sections only on empty lines and the client writes them as one space,
/// and the state suffixes in lower case.
fn for_loader(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .map(|line| match state_suffix(line) {
            Some(position) => format!("{}{}", &line[..position], line[position..].to_lowercase()),
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where a ` /offline`-style suffix starts, in any case.
fn state_suffix(line: &str) -> Option<usize> {
    let trimmed = line.trim_end();
    let position = trimmed.rfind('/')?;
    let suffix = trimmed[position..].to_lowercase();
    STATE_SUFFIXES
        .contains(&suffix.as_str())
        .then_some(position)
}

/// Every line naming a type `known` doesn't know: the ship in the header,
/// a module, its charge, or a stack. `load_eft` stops at the first unknown
/// name; this reads them all, the way `load_eft` splits lines, so a paste
/// with three typos says so once. Malformed text is left for `load_eft` to
/// refuse.
pub(super) fn unknown_lines(text: &str, known: impl Fn(&str) -> bool) -> Vec<EftLine> {
    let mut unknown = Vec::new();
    let mut lines = text
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()));
    let Some((header_number, header)) = lines.by_ref().find(|(_, line)| !line.is_empty()) else {
        return unknown;
    };
    let Some(ship) = header
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|inner| inner.split(',').next())
    else {
        return unknown;
    };
    if !known(ship.trim()) {
        unknown.push(EftLine {
            line: header_number,
            text: header.to_string(),
        });
    }

    let mut in_mutation_section = false;
    let mut section_start = true;
    for (number, line) in lines {
        if line.is_empty() {
            section_start = true;
            continue;
        }
        if section_start {
            // `[1] Base Module` opens a section describing a mutation.
            in_mutation_section = is_mutation_header(line);
            section_start = false;
        }
        if in_mutation_section || line.to_lowercase().starts_with("[empty") {
            continue;
        }
        if !line_is_known(line, &known) {
            unknown.push(EftLine {
                line: number,
                text: line.to_string(),
            });
        }
    }
    unknown
}

fn is_mutation_header(line: &str) -> bool {
    line.strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .is_some_and(|(number, _)| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

fn line_is_known(line: &str, known: &impl Fn(&str) -> bool) -> bool {
    let line = strip_mutation_reference(line);
    if known(line) {
        return true;
    }
    // A stack: `Hobgoblin II x5`.
    if let Some((name, count)) = line.rsplit_once(" x") {
        if !count.is_empty() && count.chars().all(|c| c.is_ascii_digit()) {
            return known(name.trim());
        }
    }
    let line = match state_suffix(line) {
        Some(position) => line[..position].trim_end(),
        None => line,
    };
    match line.split_once(',') {
        Some((module, charge)) => known(module.trim()) && known(charge.trim()),
        None => known(line),
    }
}

/// `Module [1]` refers to mutation section 1; the name is what's before it.
fn strip_mutation_reference(line: &str) -> &str {
    if let Some(open) = line.rfind(" [") {
        let reference = &line[open + 2..];
        if let Some(number) = reference.strip_suffix(']') {
            if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
                return line[..open].trim_end();
            }
        }
    }
    line
}

#[cfg(test)]
mod tests;
