//! `fitting-data.lock`: the engine data version this build expects.

/// The repository's lock, compiled in so a stale `sde.dat` is caught at load.
const PINNED: &str = include_str!("../../../fitting-data.lock");

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DataLock {
    /// The npm package version, `<major>.<build>.<patch>`.
    pub version: String,
    pub major: i32,
    pub build_number: i32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LockError {
    MissingVersion,
    InvalidVersion(String),
}

impl DataLock {
    pub fn parse(text: &str) -> Result<Self, LockError> {
        let version = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("version="))
            .ok_or(LockError::MissingVersion)?;
        let invalid = || LockError::InvalidVersion(version.to_string());
        let parts: Vec<i32> = version
            .split('.')
            .map(|part| part.parse().map_err(|_| invalid()))
            .collect::<Result<_, _>>()?;
        let [major, build_number, _patch] = parts[..] else {
            return Err(invalid());
        };
        Ok(Self {
            version: version.to_string(),
            major,
            build_number,
        })
    }

    /// The lock this crate was built with.
    #[must_use]
    pub fn pinned() -> Self {
        Self::parse(PINNED).expect("fitting-data.lock is checked by its own test")
    }
}

#[cfg(test)]
mod tests;
