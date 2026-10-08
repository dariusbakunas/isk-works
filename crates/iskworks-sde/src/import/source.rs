use super::*;

pub(crate) const REQUIRED_FILES: [&str; 3] = ["types.jsonl", "groups.jsonl", "blueprints.jsonl"];
pub(crate) const SOLAR_SYSTEMS_FILE: &str = "mapSolarSystems.jsonl";
pub(crate) const CONSTELLATIONS_FILE: &str = "mapConstellations.jsonl";
pub(crate) const REGIONS_FILE: &str = "mapRegions.jsonl";
pub(crate) const NPC_STATIONS_FILE: &str = "npcStations.jsonl";
pub(crate) const NPC_CORPORATIONS_FILE: &str = "npcCorporations.jsonl";
pub(crate) const STATION_OPERATIONS_FILE: &str = "stationOperations.jsonl";
pub(crate) const MOONS_FILE: &str = "mapMoons.jsonl";
pub(crate) const TYPE_DOGMA_FILE: &str = "typeDogma.jsonl";
pub(crate) const METADATA_FILE: &str = "_sde.jsonl";
pub(crate) const CATEGORIES_FILE: &str = "categories.jsonl";
pub(crate) const META_GROUPS_FILE: &str = "metaGroups.jsonl";
pub(crate) const MARKET_GROUPS_FILE: &str = "marketGroups.jsonl";
pub(crate) const INDUSTRY_TARGET_FILTERS_FILE: &str = "industryTargetFilters.jsonl";
pub(crate) const INDUSTRY_MODIFIER_SOURCES_FILE: &str = "industryModifierSources.jsonl";
pub(crate) const PLANET_SCHEMATICS_FILE: &str = "planetSchematics.jsonl";
pub(crate) const MAP_PLANETS_FILE: &str = "mapPlanets.jsonl";

#[derive(Debug, Error)]
pub enum SdeError {
    #[error("failed to access SDE source {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read SDE zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("required SDE file {0} is missing from archive")]
    MissingFile(&'static str),
    #[error("failed to parse {file} line {line}: {source}")]
    Json {
        file: &'static str,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("manufacturing blueprint {blueprint_type_id} references unknown type {type_id}")]
    UnknownType {
        blueprint_type_id: i64,
        type_id: i64,
    },
    #[error("{kind} quantity for blueprint {blueprint_type_id} type {type_id} must be positive")]
    InvalidQuantity {
        kind: &'static str,
        blueprint_type_id: i64,
        type_id: i64,
    },
    #[error("duplicate {kind} ID {id} in SDE source")]
    Duplicate { kind: &'static str, id: i64 },
    #[error("{0} contains a cycle")]
    CyclicReference(&'static str),
    #[error("{relation} references unknown ID {referenced_id} from ID {source_id}")]
    UnknownReference {
        relation: &'static str,
        source_id: i64,
        referenced_id: i64,
    },
    #[error("SDE storage failed: {0}")]
    Storage(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SdeSource {
    pub source_label: String,
    pub source_checksum: String,
    pub source_version: String,
    pub(super) files: HashMap<&'static str, String>,
}

impl SdeSource {
    pub fn open_zip(path: impl AsRef<Path>) -> Result<Self, SdeError> {
        let path = path.as_ref();
        let mut file = File::open(path).map_err(|source| SdeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 1024 * 1024];
        loop {
            let read = file.read(&mut buffer).map_err(|source| SdeError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        let checksum = format!("{:x}", hasher.finalize());
        let label = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("local-sde.zip")
            .to_string();
        let file = File::open(path).map_err(|source| SdeError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_zip_reader(file, label, checksum)
    }

    pub fn from_zip_reader<R>(
        reader: R,
        source_label: impl Into<String>,
        source_checksum: impl Into<String>,
    ) -> Result<Self, SdeError>
    where
        R: Read + Seek,
    {
        let mut archive = ZipArchive::new(reader)?;
        let mut files = HashMap::new();
        for name in REQUIRED_FILES {
            files.insert(name, read_archive_file(&mut archive, name)?);
        }
        for name in [
            SOLAR_SYSTEMS_FILE,
            CONSTELLATIONS_FILE,
            REGIONS_FILE,
            NPC_STATIONS_FILE,
            NPC_CORPORATIONS_FILE,
            STATION_OPERATIONS_FILE,
            MOONS_FILE,
            TYPE_DOGMA_FILE,
            CATEGORIES_FILE,
            META_GROUPS_FILE,
            MARKET_GROUPS_FILE,
            INDUSTRY_TARGET_FILTERS_FILE,
            INDUSTRY_MODIFIER_SOURCES_FILE,
            PLANET_SCHEMATICS_FILE,
            MAP_PLANETS_FILE,
        ] {
            match read_archive_file(&mut archive, name) {
                Ok(contents) => {
                    files.insert(name, contents);
                }
                Err(SdeError::MissingFile(_)) => {}
                Err(error) => return Err(error),
            }
        }

        let metadata = match archive.by_name(METADATA_FILE) {
            Ok(mut file) => {
                let mut contents = String::new();
                file.read_to_string(&mut contents)
                    .map_err(|source| SdeError::Io {
                        path: PathBuf::from(METADATA_FILE),
                        source,
                    })?;
                Some(contents)
            }
            Err(zip::result::ZipError::FileNotFound) => None,
            Err(error) => return Err(error.into()),
        };

        let source_checksum = source_checksum.into();
        let source_version = metadata
            .as_deref()
            .and_then(parse_source_version)
            .unwrap_or_else(|| {
                format!(
                    "sha256:{}",
                    &source_checksum[..16.min(source_checksum.len())]
                )
            });

        Ok(Self {
            source_label: source_label.into(),
            source_checksum,
            source_version,
            files,
        })
    }

    #[must_use]
    pub fn has_solar_systems(&self) -> bool {
        self.files.contains_key(SOLAR_SYSTEMS_FILE)
    }

    #[must_use]
    pub fn has_facility_dogma(&self) -> bool {
        self.files.contains_key(TYPE_DOGMA_FILE)
    }

    #[must_use]
    pub fn has_npc_stations(&self) -> bool {
        self.files.contains_key(NPC_STATIONS_FILE)
            && self.files.contains_key(NPC_CORPORATIONS_FILE)
            && self.files.contains_key(STATION_OPERATIONS_FILE)
    }

    /// Unlike the optional-file checks above, reaction formulas live inside
    /// the required `blueprints.jsonl` file (as a `"reaction"` activity on a
    /// blueprint entry) rather than a separate file, so presence is detected
    /// by content rather than by file existence.
    #[must_use]
    pub fn has_reaction_formulas(&self) -> bool {
        self.files
            .get("blueprints.jsonl")
            .is_some_and(|contents| contents.contains("\"reaction\":"))
    }

    #[must_use]
    pub fn has_packaged_volumes(&self) -> bool {
        self.files.get("types.jsonl").is_some_and(|contents| {
            contents.contains("\"packagedVolume\":") || contents.contains("\"volume\":")
        })
    }

    #[must_use]
    pub fn has_classification_metadata(&self) -> bool {
        [CATEGORIES_FILE, META_GROUPS_FILE, MARKET_GROUPS_FILE]
            .iter()
            .all(|name| self.files.contains_key(name))
    }

    /// Planetary Interaction data: `planetSchematics` (PI factory recipes)
    /// and `mapPlanets` (planet -> solar system / celestial index, for names).
    #[must_use]
    pub fn has_planetary_data(&self) -> bool {
        [PLANET_SCHEMATICS_FILE, MAP_PLANETS_FILE]
            .iter()
            .all(|name| self.files.contains_key(name))
    }

    pub(super) fn file(&self, name: &'static str) -> Result<&str, SdeError> {
        self.files
            .get(name)
            .map(String::as_str)
            .ok_or(SdeError::MissingFile(name))
    }
}

pub(crate) fn read_archive_file<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &'static str,
) -> Result<String, SdeError> {
    let mut file = archive.by_name(name).map_err(|error| match error {
        zip::result::ZipError::FileNotFound => SdeError::MissingFile(name),
        other => SdeError::Zip(other),
    })?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|source| SdeError::Io {
            path: PathBuf::from(name),
            source,
        })?;
    Ok(contents)
}

pub(crate) fn parse_source_version(contents: &str) -> Option<String> {
    let value: serde_json::Value =
        serde_json::from_str(contents.lines().find(|line| !line.trim().is_empty())?).ok()?;
    ["buildNumber", "build_number", "build", "version"]
        .iter()
        .find_map(|key| value.get(key))
        .and_then(|value| match value {
            serde_json::Value::String(value) => Some(value.clone()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
}
