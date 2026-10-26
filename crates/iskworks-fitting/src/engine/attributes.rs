//! The dogma attributes the stats mapping reads, resolved by name once at
//! load. The derived ones (negative ids) come from EVEShip.fit's
//! sde-patched and have no stable id, so names are the contract; a renamed
//! attribute fails the load instead of quietly reading zero.

use esf_data::Sde;

use super::FittingLoadError;

macro_rules! attribute_table {
    ($(#[$meta:meta])* $table:ident { $($field:ident => $name:literal,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub(super) struct $table {
            $(pub(super) $field: i32,)*
        }

        impl $table {
            pub(super) fn resolve(sde: &Sde<'_>) -> Result<Self, FittingLoadError> {
                Ok(Self {
                    $($field: sde
                        .attribute_name_to_id($name)
                        .ok_or(FittingLoadError::MissingAttribute($name))?,)*
                })
            }
        }
    };
}

attribute_table! {
    /// Attribute ids by role. Unless noted they are read off the ship.
    StatAttributes {
    cpu_output => "cpuOutput",
    cpu_load => "cpuLoad",
    power_output => "powerOutput",
    power_load => "powerLoad",
    calibration_output => "upgradeCapacity",
    calibration_load => "upgradeLoad",
    turret_hardpoints => "turretSlotsLeft",
    launcher_hardpoints => "launcherSlotsLeft",
    high_slots => "hiSlots",
    med_slots => "medSlots",
    low_slots => "lowSlots",
    rig_slots => "upgradeSlotsLeft",
    drone_bandwidth => "droneBandwidth",
    drone_bandwidth_load => "droneBandwidthLoad",
    drone_bay => "droneCapacity",
    drone_bay_load => "droneCapacityLoad",
    capacitor_capacity => "capacitorCapacity",
    // milliseconds
    capacitor_recharge => "rechargeRate",
    capacitor_peak_delta => "capacitorPeakDelta",
    // seconds; negative when stable
    capacitor_depletes_in => "capacitorDepletesIn",
    // percent; zero when not stable
    capacitor_stable_percentage => "capacitorStablePercentage",
    dps_without_reload => "damagePerSecondWithoutReload",
    dps_with_reload => "damagePerSecondWithReload",
    volley => "damageVolley",
    drone_dps => "droneDamagePerSecond",
    ehp => "ehp",
    shield_ehp => "shieldEhp",
    armor_ehp => "armorEhp",
    hull_ehp => "hullEhp",
    shield_hp => "shieldCapacity",
    armor_hp => "armorHP",
    hull_hp => "hp",
    shield_em => "shieldEmDamageResonance",
    shield_thermal => "shieldThermalDamageResonance",
    shield_kinetic => "shieldKineticDamageResonance",
    shield_explosive => "shieldExplosiveDamageResonance",
    armor_em => "armorEmDamageResonance",
    armor_thermal => "armorThermalDamageResonance",
    armor_kinetic => "armorKineticDamageResonance",
    armor_explosive => "armorExplosiveDamageResonance",
    hull_em => "emDamageResonance",
    hull_thermal => "thermalDamageResonance",
    hull_kinetic => "kineticDamageResonance",
    hull_explosive => "explosiveDamageResonance",
    shield_passive_regen => "passiveShieldEffectiveRechargeRate",
    shield_boost => "shieldEffectiveBoostRate",
    armor_repair => "armorEffectiveRepairRate",
    hull_repair => "hullEffectiveRepairRate",
    max_targets => "maxTargets",
    lock_range => "maxTargetRange",
    scan_resolution => "scanResolution",
    sensor_strength => "scanStrength",
    signature_radius => "signatureRadius",
    max_velocity => "maxVelocity",
    mass => "mass",
    inertia => "agility",
    // seconds
    align_time => "alignTime",
    warp_speed_multiplier => "warpSpeedMultiplier",
    base_warp_speed => "baseWarpSpeed",
    drones_active => "droneActive",
    // on the character
    max_active_drones => "maxActiveDrones",
    // on the character
    drone_control_range => "droneControlDistance",
    }
}

attribute_table! {
    /// Attribute ids read off each fitted item.
    ItemAttributes {
    cpu => "cpu",
    powergrid => "power",
    calibration => "upgradeCost",
    // GJ per cycle
    capacitor_need => "capacitorNeed",
    // cycle time in ms, for most modules
    duration => "duration",
    // cycle time in ms, for turrets and launchers
    rate_of_fire => "speed",
    optimal => "maxRange",
    falloff => "falloff",
    dps => "damagePerSecondWithoutReload",
    }
}
