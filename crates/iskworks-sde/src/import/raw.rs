use super::*;

pub(crate) fn parse_jsonl<T>(file: &'static str, contents: &str) -> Result<Vec<T>, SdeError>
where
    T: for<'de> Deserialize<'de>,
{
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|source| SdeError::Json {
                file,
                line: index + 1,
                source,
            })
        })
        .collect()
}

pub(crate) fn parse_optional_jsonl<T>(
    source: &SdeSource,
    file: &'static str,
) -> Result<Vec<T>, SdeError>
where
    T: for<'de> Deserialize<'de>,
{
    source
        .files
        .get(file)
        .map(|contents| parse_jsonl(file, contents))
        .transpose()
        .map(Option::unwrap_or_default)
}

#[derive(Debug, Deserialize)]
pub(crate) struct LocalizedString {
    pub(crate) en: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawGroup {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    pub(crate) name: LocalizedString,
    #[serde(rename = "categoryID")]
    pub(crate) category_id: Option<i64>,
    #[serde(default)]
    pub(crate) published: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawCategory {
    #[serde(rename = "_key")]
    pub(super) key: i64,
    pub(super) name: LocalizedString,
    #[serde(default)]
    pub(super) published: bool,
}
#[derive(Debug, Deserialize)]
pub(crate) struct RawMetaGroup {
    #[serde(rename = "_key")]
    pub(super) key: i64,
    pub(super) name: LocalizedString,
}
#[derive(Debug, Deserialize)]
pub(crate) struct RawMarketGroup {
    #[serde(rename = "_key")]
    pub(super) key: i64,
    pub(super) name: LocalizedString,
    #[serde(rename = "parentGroupID")]
    pub(super) parent_group_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawType {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    pub(crate) name: LocalizedString,
    #[serde(rename = "groupID")]
    pub(crate) group_id: Option<i64>,
    #[serde(rename = "marketGroupID")]
    pub(crate) market_group_id: Option<i64>,
    #[serde(rename = "metaGroupID")]
    pub(crate) meta_group_id: Option<i64>,
    #[serde(rename = "packagedVolume")]
    pub(crate) packaged_volume: Option<serde_json::Number>,
    pub(crate) volume: Option<serde_json::Number>,
    #[serde(default)]
    pub(crate) published: bool,
}

pub(crate) fn json_number_decimal(value: serde_json::Number) -> Option<Decimal> {
    Decimal::from_str(&value.to_string()).ok()
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawBlueprint {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(default)]
    pub(crate) activities: HashMap<String, RawActivity>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawSolarSystem {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    pub(crate) name: LocalizedString,
    #[serde(rename = "constellationID")]
    pub(crate) constellation_id: Option<i64>,
    #[serde(rename = "regionID")]
    pub(crate) region_id: Option<i64>,
    #[serde(rename = "securityStatus")]
    pub(crate) security_status: Option<serde_json::Number>,
    #[serde(rename = "wormholeClassID")]
    pub(crate) wormhole_class_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawConstellation {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    pub(crate) name: LocalizedString,
    #[serde(rename = "regionID")]
    pub(crate) region_id: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawRegion {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    pub(crate) name: LocalizedString,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawNpcStation {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(rename = "solarSystemID")]
    pub(crate) solar_system_id: i64,
    #[serde(rename = "ownerID")]
    pub(crate) owner_id: i64,
    #[serde(rename = "operationID")]
    pub(crate) operation_id: i64,
    #[serde(rename = "typeID")]
    pub(crate) type_id: i64,
    #[serde(rename = "orbitID")]
    pub(crate) orbit_id: Option<i64>,
    #[serde(rename = "celestialIndex")]
    pub(crate) celestial_index: Option<i64>,
    #[serde(rename = "orbitIndex")]
    pub(crate) orbit_index: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawNamedRecord {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    pub(crate) name: LocalizedString,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawStationOperation {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(rename = "operationName")]
    pub(crate) operation_name: LocalizedString,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawPlanetSchematic {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    pub(crate) name: LocalizedString,
    #[serde(rename = "cycleTime")]
    pub(crate) cycle_time: i64,
    #[serde(default)]
    pub(crate) types: Vec<RawPlanetSchematicType>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawPlanetSchematicType {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(rename = "isInput")]
    pub(crate) is_input: bool,
    pub(crate) quantity: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawPlanet {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(rename = "solarSystemID")]
    pub(crate) solar_system_id: i64,
    #[serde(rename = "celestialIndex")]
    pub(crate) celestial_index: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawMoon {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
}

pub(crate) fn roman_numeral(mut value: i64) -> String {
    if value <= 0 {
        return value.to_string();
    }
    let mut result = String::new();
    for (number, numeral) in [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        while value >= number {
            result.push_str(numeral);
            value -= number;
        }
    }
    result
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTypeDogma {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(default, rename = "dogmaAttributes")]
    pub(crate) dogma_attributes: Vec<RawDogmaValue>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawDogmaValue {
    #[serde(rename = "attributeID")]
    pub(crate) attribute_id: i64,
    #[serde(deserialize_with = "deserialize_decimal")]
    pub(crate) value: Decimal,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawIndustryTargetFilter {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(rename = "categoryIDs", default)]
    pub(crate) category_ids: Vec<i64>,
    #[serde(rename = "groupIDs", default)]
    pub(crate) group_ids: Vec<i64>,
}

/// `industryModifierSources.jsonl` -- keyed by a structure or rig type id.
/// Only the `manufacturing`/`reaction` activities and only each entry's
/// `filterID` matter here; bonus magnitudes still come from `typeDogma`
/// (2593/2594 etc.), and research/copy/invention activities are ignored.
#[derive(Debug, Deserialize)]
pub(crate) struct RawIndustryModifierSource {
    #[serde(rename = "_key")]
    pub(crate) key: i64,
    #[serde(default)]
    pub(crate) manufacturing: RawIndustryActivityModifiers,
    #[serde(default)]
    pub(crate) reaction: RawIndustryActivityModifiers,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RawIndustryActivityModifiers {
    #[serde(default)]
    pub(crate) material: Vec<RawIndustryModifierEntry>,
    #[serde(default)]
    pub(crate) time: Vec<RawIndustryModifierEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawIndustryModifierEntry {
    #[serde(rename = "filterID")]
    pub(crate) filter_id: Option<i64>,
}

/// A rig type's resolved `filterID`s for the two activities ISK Works
/// models, extracted from `RawIndustryModifierSource` and handed to
/// `normalize_facility_dogma`. Each activity/dimension carries *every*
/// referenced filter (EVE emits one `industryModifierSources` entry per
/// covered filter for a multi-scope rig); an empty vec means "unrestricted"
/// (no `filterID`, or the SDE lacks the dataset). Sorted and de-duplicated.
#[derive(Debug, Default, Clone)]
pub(crate) struct RigFilterIds {
    pub(crate) manufacturing_material: Vec<i64>,
    pub(crate) manufacturing_time: Vec<i64>,
    pub(crate) reaction_material: Vec<i64>,
    pub(crate) reaction_time: Vec<i64>,
}

pub(crate) fn deserialize_decimal<'de, D>(deserializer: D) -> Result<Decimal, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let text = match value {
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(value) => value,
        _ => {
            return Err(D::Error::custom(
                "dogma value must be a number or numeric string",
            ))
        }
    };
    Decimal::from_scientific(&text)
        .or_else(|_| text.parse())
        .map_err(D::Error::custom)
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawActivity {
    pub(crate) time: Option<i64>,
    #[serde(default)]
    pub(crate) materials: Vec<RawTypeQuantity>,
    #[serde(default)]
    pub(crate) products: Vec<RawTypeQuantity>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawTypeQuantity {
    #[serde(rename = "typeID")]
    pub(crate) type_id: i64,
    pub(crate) quantity: i64,
}
