//! `FitInput` → `esf_dogma_engine::Fit` → `FitStats`.

use std::collections::BTreeMap;

use esf_data::Info;
use esf_dogma_engine::{
    Calculation, Character, Charge, Environment, Fit, ItemResult, Mutation, Options, ReactiveArmor,
    Ship, Slot, SourceRef as EngineSourceRef, State, Target, Violation,
};
use iskworks_core::fitting::{
    AttributeExplanation, CapacitorState, CapacitorStats, DamageProfile, DefenseStats, DroneStats,
    ExplainTarget, FitCalculation, FitInput, FitItem, FitStats, FitViolation, FittingError,
    Headline, HeadlineKind, ItemStats, LayerStats, ModifierSource, ModuleState, NavigationStats,
    OffenseStats, Pilot, Resists, ResourceStats, SlotKind, SourceRef, TargetingStats, Usage,
    ViolationTarget,
};

use super::attributes::ItemAttributes;
use super::EsfFittingEngine;

impl EsfFittingEngine {
    /// The fit as the engine takes it, with `included` (each engine item's
    /// index in the caller's fit) and the caller's unknown items.
    fn engine_fit(
        &self,
        fit: &FitInput,
        pilot: &Pilot,
        profile: &DamageProfile,
    ) -> Result<(Fit, Vec<usize>, Vec<usize>), FittingError> {
        let ship_type_id = known_id(fit.ship_type_id)
            .filter(|id| self.info.get_type(*id).is_some())
            .ok_or(FittingError::UnknownShip(fit.ship_type_id))?;

        let EngineItems {
            items,
            included,
            unknown_items,
        } = self.engine_items(fit);

        let [em, thermal, kinetic, explosive] = profile.normalized();
        let uniform = profile.normalized() == DamageProfile::uniform().normalized();
        let engine_fit = Fit {
            name: None,
            ship: Ship {
                type_id: ship_type_id,
                mode: None,
            },
            items,
            character: Character {
                skills: self.skill_levels(pilot),
                ..Default::default()
            },
            environment: Environment {
                damage_profile: esf_dogma_engine::DamageProfile {
                    em,
                    explosive,
                    kinetic,
                    thermal,
                },
                reactive_armor: if uniform {
                    ReactiveArmor::DoNotAdapt
                } else {
                    ReactiveArmor::DamageProfile
                },
                ..Default::default()
            },
            incoming: Default::default(),
        };
        Ok((engine_fit, included, unknown_items))
    }

    pub(super) fn run(
        &self,
        fit: &FitInput,
        pilot: &Pilot,
        profile: &DamageProfile,
    ) -> Result<FitCalculation, FittingError> {
        let (engine_fit, included, unknown_items) = self.engine_fit(fit, pilot, profile)?;
        let calculation = esf_dogma_engine::calculate(
            &self.info,
            &engine_fit,
            &Options {
                sources: false,
                validate: true,
            },
        );

        let included_items: Vec<&FitItem> =
            included.iter().map(|index| &fit.items[*index]).collect();
        let mut items = vec![None; fit.items.len()];
        for (result, index) in calculation.items.iter().zip(&included) {
            items[*index] = Some(self.item_stats(result));
        }
        Ok(FitCalculation {
            stats: self.stats(&calculation, &engine_fit, &included_items),
            items,
            violations: calculation
                .violations
                .iter()
                .flatten()
                .map(|violation| fit_violation(violation, &included))
                .collect(),
            unknown_items,
        })
    }

    pub(super) fn explain_attribute(
        &self,
        fit: &FitInput,
        pilot: &Pilot,
        profile: &DamageProfile,
        target: ExplainTarget,
        attribute: &str,
    ) -> Result<AttributeExplanation, FittingError> {
        let attribute_id = self
            .info
            .attribute_name_to_id(attribute)
            .ok_or(FittingError::UnknownAttribute)?;
        let (engine_fit, included, _) = self.engine_fit(fit, pilot, profile)?;
        let calculation = esf_dogma_engine::calculate(
            &self.info,
            &engine_fit,
            &Options {
                sources: true,
                validate: false,
            },
        );
        let result = match target {
            ExplainTarget::Ship => &calculation.ship,
            ExplainTarget::Item(index) => included
                .iter()
                .position(|included| *included == index)
                .and_then(|engine_index| calculation.items.get(engine_index))
                .ok_or(FittingError::UnknownItem(index))?,
        };
        let Some(attribute) = result.attributes.get(&attribute_id) else {
            // The target doesn't carry it: nothing modified it.
            return Ok(AttributeExplanation {
                base: 0.0,
                value: 0.0,
                sources: Vec::new(),
            });
        };
        let caller = |engine_index: usize| included[engine_index];
        Ok(AttributeExplanation {
            base: attribute.base,
            value: attribute.value,
            sources: attribute
                .sources
                .iter()
                .map(|source| ModifierSource {
                    from: match source.from {
                        EngineSourceRef::Ship => SourceRef::Ship,
                        EngineSourceRef::Mode => SourceRef::Mode,
                        EngineSourceRef::Character => SourceRef::Character,
                        EngineSourceRef::Item { index } => SourceRef::Item {
                            index: caller(index),
                        },
                        EngineSourceRef::Charge { index } => SourceRef::Charge {
                            index: caller(index),
                        },
                        EngineSourceRef::Skill { type_id } => SourceRef::Skill {
                            type_id: i64::from(type_id),
                        },
                        EngineSourceRef::Projected { index } => SourceRef::Projected { index },
                        EngineSourceRef::Buff { id } => SourceRef::Buff { id: i64::from(id) },
                    },
                    operator: serde_json::to_value(source.operator)
                        .ok()
                        .and_then(|name| name.as_str().map(str::to_string))
                        .unwrap_or_default(),
                    value: source.value,
                    penalty: source.penalty,
                    applied: source.applied,
                })
                .collect(),
        })
    }

    /// The fit's items as the engine takes them. Items the data does not
    /// know are left out (and reported); the engine's results index into
    /// what it was given, so `included` is the way back to the caller's
    /// indexes.
    pub(super) fn engine_items(&self, fit: &FitInput) -> EngineItems {
        let mut engine_items = EngineItems::default();
        for (index, item) in fit.items.iter().enumerate() {
            let Some(type_id) = known_id(item.type_id).filter(|id| self.is_known(*id)) else {
                engine_items.unknown_items.push(index);
                continue;
            };
            let charge = match item.charge_type_id {
                Some(charge) => match known_id(charge).filter(|id| self.is_known(*id)) {
                    Some(type_id) => Some(Charge { type_id }),
                    None => {
                        engine_items.unknown_items.push(index);
                        None
                    }
                },
                None => None,
            };
            let mutation = item.mutation.as_ref().and_then(|mutation| {
                Some(Mutation {
                    base: known_id(mutation.base_type_id)?,
                    attributes: mutation
                        .attributes
                        .iter()
                        .filter_map(|(id, value)| Some((known_id(*id)?, *value)))
                        .collect(),
                })
            });
            engine_items.included.push(index);
            engine_items.items.push(esf_dogma_engine::FitItem {
                type_id,
                slot: slot(item),
                quantity: item.quantity,
                state: state(item.state),
                charge,
                mutation,
                fighter_abilities: None,
                booster_side_effects: Default::default(),
                spool: None,
            });
        }
        engine_items
    }

    fn item_stats(&self, result: &ItemResult) -> ItemStats {
        let a = &self.item_attributes;
        let attribute = |id: i32| value(result, id);
        let cycle_ms = [attribute(a.duration), attribute(a.rate_of_fire)]
            .into_iter()
            .find(|cycle| *cycle > 0.0);
        let capacitor_per_second = match cycle_ms {
            Some(cycle_ms) if result.state >= State::Active => {
                attribute(a.capacitor_need) / (cycle_ms / 1000.0)
            }
            _ => 0.0,
        };
        ItemStats {
            reached_state: module_state(result.state),
            max_state: module_state(result.max_state),
            cpu: attribute(a.cpu),
            powergrid: attribute(a.powergrid),
            calibration: attribute(a.calibration),
            capacitor_per_second,
            optimal: attribute(a.optimal),
            falloff: attribute(a.falloff),
            dps: attribute(a.dps),
            headline: headline(a, result, cycle_ms),
        }
    }

    pub(super) fn is_known(&self, type_id: i32) -> bool {
        self.info.get_type(type_id).is_some()
    }

    pub(super) fn skill_levels(&self, pilot: &Pilot) -> BTreeMap<i32, u8> {
        match pilot {
            Pilot::AllV => self.skills.iter().map(|skill| (*skill, 5)).collect(),
            Pilot::AllZero => BTreeMap::new(),
            Pilot::Skills(skills) => skills
                .levels()
                .iter()
                .filter_map(|(skill, level)| {
                    // PilotSkills holds 0–5, so the level always fits.
                    Some((known_id(*skill)?, u8::try_from(*level).ok()?))
                })
                .collect(),
        }
    }

    /// Whether a type carries the dogma effect of this name (how the engine
    /// itself tells turrets and launchers apart).
    fn has_effect(&self, type_id: i32, name: &str) -> bool {
        self.info
            .get_dogma_effects(type_id)
            .into_iter()
            .flatten()
            .filter_map(|effect| self.info.get_dogma_effect(effect.effect_id()))
            .any(|effect| effect.name() == name)
    }

    fn stats(&self, calculation: &Calculation, fit: &Fit, items: &[&FitItem]) -> FitStats {
        let a = &self.attributes;
        let ship = |id: i32| value(&calculation.ship, id);
        let character = |id: i32| value(&calculation.character, id);
        let rack = |kind: SlotKind, total: i32| Usage {
            used: items.iter().filter(|item| item.slot_kind == kind).count() as f64,
            total: ship(total),
        };
        let hardpoints = |effect: &str, total: i32| Usage {
            used: fit
                .items
                .iter()
                .filter(|item| matches!(item.slot, Slot::High(_)))
                .filter(|item| self.has_effect(item.type_id, effect))
                .count() as f64,
            total: ship(total),
        };
        let usage = |used: i32, total: i32| Usage {
            used: ship(used),
            total: ship(total),
        };
        let layer = |hp: i32, ehp: i32, resonances: [i32; 4]| LayerStats {
            hp: ship(hp),
            ehp: ship(ehp),
            resists: Resists::from_resonances(resonances.map(ship)),
        };

        let depletes_in = ship(a.capacitor_depletes_in);
        // The ship's damage per second already includes its drones.
        let total_dps = ship(a.dps_without_reload);
        let drone_dps = ship(a.drone_dps);
        // Volley stays on each weapon (it comes from the loaded charge);
        // only weapons that reached active fire.
        let volley = calculation
            .items
            .iter()
            .zip(&fit.items)
            .filter(|(result, item)| {
                !matches!(item.slot, Slot::DroneBay | Slot::FighterBay | Slot::Cargo)
                    && result.state >= State::Active
            })
            .map(|(result, _)| value(result, a.volley))
            .sum();
        FitStats {
            resources: ResourceStats {
                cpu: usage(a.cpu_load, a.cpu_output),
                powergrid: usage(a.power_load, a.power_output),
                calibration: usage(a.calibration_load, a.calibration_output),
                turret_hardpoints: hardpoints("turretFitted", a.turret_hardpoints),
                launcher_hardpoints: hardpoints("launcherFitted", a.launcher_hardpoints),
                high_slots: rack(SlotKind::High, a.high_slots),
                med_slots: rack(SlotKind::Med, a.med_slots),
                low_slots: rack(SlotKind::Low, a.low_slots),
                rig_slots: rack(SlotKind::Rig, a.rig_slots),
                drone_bandwidth: usage(a.drone_bandwidth_load, a.drone_bandwidth),
                drone_bay: usage(a.drone_bay_load, a.drone_bay),
            },
            capacitor: CapacitorStats {
                capacity: ship(a.capacitor_capacity),
                recharge_seconds: ship(a.capacitor_recharge) / 1000.0,
                peak_delta: ship(a.capacitor_peak_delta),
                state: if depletes_in < 0.0 {
                    CapacitorState::Stable {
                        level: ship(a.capacitor_stable_percentage) / 100.0,
                    }
                } else {
                    CapacitorState::Depletes {
                        seconds: depletes_in,
                    }
                },
            },
            offense: OffenseStats {
                weapon_dps: total_dps - drone_dps,
                weapon_dps_with_reload: ship(a.dps_with_reload) - drone_dps,
                volley,
                drone_dps,
                total_dps,
            },
            defense: DefenseStats {
                ehp: ship(a.ehp),
                shield: layer(
                    a.shield_hp,
                    a.shield_ehp,
                    [
                        a.shield_em,
                        a.shield_thermal,
                        a.shield_kinetic,
                        a.shield_explosive,
                    ],
                ),
                armor: layer(
                    a.armor_hp,
                    a.armor_ehp,
                    [
                        a.armor_em,
                        a.armor_thermal,
                        a.armor_kinetic,
                        a.armor_explosive,
                    ],
                ),
                hull: layer(
                    a.hull_hp,
                    a.hull_ehp,
                    [a.hull_em, a.hull_thermal, a.hull_kinetic, a.hull_explosive],
                ),
                shield_passive_regen: ship(a.shield_passive_regen),
                shield_boost: ship(a.shield_boost),
                armor_repair: ship(a.armor_repair),
                hull_repair: ship(a.hull_repair),
            },
            targeting: TargetingStats {
                max_targets: ship(a.max_targets),
                lock_range: ship(a.lock_range),
                scan_resolution: ship(a.scan_resolution),
                sensor_strength: ship(a.sensor_strength),
                signature_radius: ship(a.signature_radius),
            },
            navigation: NavigationStats {
                max_velocity: ship(a.max_velocity),
                mass: ship(a.mass),
                inertia: ship(a.inertia),
                align_seconds: ship(a.align_time),
                warp_speed: ship(a.warp_speed_multiplier) * ship(a.base_warp_speed),
            },
            drones: DroneStats {
                active: ship(a.drones_active),
                max_active: character(a.max_active_drones),
                control_range: character(a.drone_control_range),
            },
        }
    }
}

#[derive(Default)]
pub(super) struct EngineItems {
    pub(super) items: Vec<esf_dogma_engine::FitItem>,
    /// For each engine item, its index in the caller's fit.
    pub(super) included: Vec<usize>,
    pub(super) unknown_items: Vec<usize>,
}

/// The first thing the item does that the table shows, in this order:
/// damage, repair, resistance, velocity, shield or armor HP, capacitor.
fn headline(a: &ItemAttributes, result: &ItemResult, cycle_ms: Option<f64>) -> Option<Headline> {
    let attribute = |id: i32| value(result, id);
    let per_second = |amount: f64| cycle_ms.map(|cycle_ms| amount / (cycle_ms / 1000.0));
    // The strongest of its four bonuses (stored negative): a single-type
    // hardener reads as its own bonus, an omni one as its even bonus.
    let resist_bonus = [
        a.em_resist_bonus,
        a.thermal_resist_bonus,
        a.kinetic_resist_bonus,
        a.explosive_resist_bonus,
    ]
    .map(|id| -attribute(id))
    .into_iter()
    .fold(0.0_f64, f64::max);
    let charge_capacitor = result
        .charge
        .as_ref()
        .map_or(0.0, |charge| value(charge, a.capacitor_bonus));
    let candidates = [
        (HeadlineKind::Dps, Some(attribute(a.dps))),
        (
            HeadlineKind::ShieldRepair,
            per_second(attribute(a.shield_repair)),
        ),
        (
            HeadlineKind::ArmorRepair,
            per_second(attribute(a.armor_repair)),
        ),
        (HeadlineKind::Resist, Some(resist_bonus)),
        (HeadlineKind::Velocity, Some(attribute(a.velocity_bonus))),
        (HeadlineKind::ShieldHp, Some(attribute(a.shield_hp_bonus))),
        (HeadlineKind::ArmorHp, Some(attribute(a.armor_hp_bonus))),
        (HeadlineKind::Capacitor, Some(charge_capacitor)),
    ];
    candidates.into_iter().find_map(|(kind, value)| {
        value
            .filter(|value| *value > 0.0)
            .map(|value| Headline { kind, value })
    })
}

/// An attribute's final value; absent attributes read as 0.
fn value(result: &ItemResult, attribute_id: i32) -> f64 {
    result
        .attributes
        .get(&attribute_id)
        .map_or(0.0, |attribute| attribute.value)
}

/// Engine type ids are `i32`; anything outside that range is unknown.
pub(super) fn known_id(type_id: i64) -> Option<i32> {
    i32::try_from(type_id).ok()
}

fn slot(item: &FitItem) -> Slot {
    // Structural checks (indexes present and in range) belong to the fit
    // itself; here an odd index just becomes slot 0, where the engine's own
    // `slot_taken` rule will flag a clash.
    let index = item
        .slot_index
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or(0);
    match item.slot_kind {
        SlotKind::High => Slot::High(index),
        SlotKind::Med => Slot::Medium(index),
        SlotKind::Low => Slot::Low(index),
        SlotKind::Rig => Slot::Rig(index),
        SlotKind::Subsystem => Slot::Subsystem(index),
        SlotKind::Service => Slot::Service(index),
        SlotKind::FighterTube => Slot::FighterTube(index),
        SlotKind::FighterBay => Slot::FighterBay,
        SlotKind::DroneBay => Slot::DroneBay,
        SlotKind::Cargo => Slot::Cargo,
        SlotKind::Implant => Slot::Implant(index),
        SlotKind::Booster => Slot::Booster(u16::from(index)),
    }
}

fn module_state(state: State) -> ModuleState {
    match state {
        State::Offline => ModuleState::Offline,
        State::Online => ModuleState::Online,
        State::Active => ModuleState::Active,
        State::Overload => ModuleState::Overload,
    }
}

fn state(state: ModuleState) -> State {
    match state {
        ModuleState::Offline => State::Offline,
        ModuleState::Online => State::Online,
        ModuleState::Active => State::Active,
        ModuleState::Overload => State::Overload,
    }
}

/// The engine's violation, with its item index mapped back to the caller's
/// and its rule split into a name and the values that failed it.
fn fit_violation(violation: &Violation, included: &[usize]) -> FitViolation {
    let target = match violation.target {
        Target::Ship => ViolationTarget::Ship,
        Target::Item { index } => ViolationTarget::Item(included[index]),
        Target::Charge { index } => ViolationTarget::Charge(included[index]),
    };
    let mut detail = serde_json::to_value(violation.rule).unwrap_or_default();
    let rule = detail
        .as_object_mut()
        .and_then(|fields| fields.remove("type"))
        .and_then(|name| name.as_str().map(str::to_string))
        .unwrap_or_default();
    FitViolation {
        target,
        rule,
        detail,
    }
}
