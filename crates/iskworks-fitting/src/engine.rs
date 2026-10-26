//! Loading `sde.dat` and calculating fits with it.

mod attributes;

use std::fmt;
use std::path::Path;

use esf_data::{eve, InfoSde, Sde};
use iskworks_core::fitting::FittingEngineInfo;

use crate::DataLock;
use attributes::StatAttributes;

/// The SDE category of skills.
const SKILL_CATEGORY_ID: i32 = 16;

/// Why the engine data could not be loaded. The API keeps running without
/// fittings when this happens.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FittingLoadError {
    Read(String),
    Invalid(String),
    /// The file is not the version `fitting-data.lock` pins.
    StaleData {
        expected: String,
        found: String,
    },
    /// The data has no attribute of this name; the stats mapping needs it.
    MissingAttribute(&'static str),
}

impl fmt::Display for FittingLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(
                f,
                "cannot read the fitting engine data ({error}); run `make fitting-data`"
            ),
            Self::Invalid(error) => write!(f, "the fitting engine data is not valid: {error}"),
            Self::StaleData { expected, found } => write!(
                f,
                "the fitting engine data is {found} but this build expects {expected}; \
                 run `make fitting-data`"
            ),
            Self::MissingAttribute(name) => {
                write!(f, "the fitting engine data has no attribute {name}")
            }
        }
    }
}

/// [`iskworks_core::fitting::FittingEngine`] on `esf-dogma-engine`.
// Read by `calculate`, which lands in the next commit.
#[allow(dead_code)]
pub struct EsfFittingEngine {
    info: InfoSde<'static>,
    sde: &'static Sde<'static>,
    attributes: StatAttributes,
    /// Every published skill, for an All V pilot.
    skills: Vec<i32>,
    engine_info: FittingEngineInfo,
}

impl EsfFittingEngine {
    /// Load the engine data. The bytes (and the index over them) are leaked
    /// on purpose: they are loaded once per process and live as long as it,
    /// and the engine borrows them for `'static`.
    pub fn load(path: &Path) -> Result<Self, FittingLoadError> {
        let bytes = std::fs::read(path)
            .map_err(|error| FittingLoadError::Read(format!("{}: {error}", path.display())))?;
        let major = eve::root_as_sde(&bytes)
            .map_err(|error| FittingLoadError::Invalid(error.to_string()))?
            .major_version();
        let lock = DataLock::pinned();
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let sde = Sde::new(bytes).map_err(|error| FittingLoadError::Invalid(error.to_string()))?;
        check_version(&lock, major, sde.build_number())?;

        let attributes = StatAttributes::resolve(&sde)?;
        let skills = sde
            .types()
            .filter(|ty| ty.published() && ty.category_id() == SKILL_CATEGORY_ID)
            .map(|ty| ty.id())
            .collect();
        let engine_info = FittingEngineInfo {
            data_version: lock.version,
            major,
            build_number: sde.build_number(),
        };
        let sde: &'static Sde<'static> = Box::leak(Box::new(sde));
        Ok(Self {
            info: InfoSde::new(sde),
            sde,
            attributes,
            skills,
            engine_info,
        })
    }

    #[must_use]
    pub fn engine_info(&self) -> FittingEngineInfo {
        self.engine_info.clone()
    }

    /// How many skills an All V pilot has.
    #[must_use]
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}

fn check_version(lock: &DataLock, major: i32, build_number: i32) -> Result<(), FittingLoadError> {
    if (major, build_number) == (lock.major, lock.build_number) {
        return Ok(());
    }
    Err(FittingLoadError::StaleData {
        expected: format!("{}.{}", lock.major, lock.build_number),
        found: format!("{major}.{build_number}"),
    })
}

#[cfg(test)]
mod tests;
