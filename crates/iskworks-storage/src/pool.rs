//! How the API and worker size and instrument their Postgres pool. The
//! binaries look the settings up (storage does no env reads) and pass a
//! [`DbPoolConfig`] to [`PgWorkspaceRepository::connect_with`].
//!
//! [`PgWorkspaceRepository::connect_with`]: crate::PgWorkspaceRepository::connect_with

use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::ConnectOptions;

const MAX_CONNECTIONS_VAR: &str = "ISKWORKS_DB_MAX_CONNECTIONS";
const SLOW_STATEMENT_MS_VAR: &str = "ISKWORKS_DB_SLOW_STATEMENT_MS";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbPoolConfig {
    /// `ISKWORKS_DB_MAX_CONNECTIONS`, default 5.
    pub max_connections: u32,
    /// `ISKWORKS_DB_SLOW_STATEMENT_MS`, default 500. Statements slower than
    /// this log at WARN inside the caller's span; `0` turns it off.
    pub slow_statement: Option<Duration>,
}

impl Default for DbPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
            slow_statement: Some(Duration::from_millis(500)),
        }
    }
}

impl DbPoolConfig {
    /// Reads the settings through `lookup` (the binaries pass their env
    /// lookup). Unset or blank keeps the default.
    pub fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Result<Self, String> {
        let mut config = Self::default();
        if let Some(value) = setting(&mut lookup, MAX_CONNECTIONS_VAR) {
            config.max_connections = value
                .parse::<u32>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(|| format!("{MAX_CONNECTIONS_VAR} must be a positive integer"))?;
        }
        if let Some(value) = setting(&mut lookup, SLOW_STATEMENT_MS_VAR) {
            let millis = value.parse::<u64>().map_err(|_| {
                format!("{SLOW_STATEMENT_MS_VAR} must be a whole number of milliseconds")
            })?;
            config.slow_statement = (millis > 0).then(|| Duration::from_millis(millis));
        }
        Ok(config)
    }

    pub(crate) fn pool_options(&self) -> PgPoolOptions {
        PgPoolOptions::new().max_connections(self.max_connections)
    }

    pub(crate) fn connect_options(
        &self,
        database_url: &str,
    ) -> Result<PgConnectOptions, sqlx::Error> {
        let options: PgConnectOptions = database_url.parse()?;
        Ok(match self.slow_statement {
            Some(threshold) => options.log_slow_statements(log::LevelFilter::Warn, threshold),
            None => options.log_slow_statements(log::LevelFilter::Off, Duration::ZERO),
        })
    }
}

fn setting(lookup: &mut impl FnMut(&str) -> Option<String>, name: &str) -> Option<String> {
    lookup(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(vars: &[(&str, &str)]) -> Result<DbPoolConfig, String> {
        DbPoolConfig::from_lookup(|name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn unset_or_blank_settings_keep_the_defaults() {
        assert_eq!(config(&[]).unwrap(), DbPoolConfig::default());
        assert_eq!(
            config(&[(MAX_CONNECTIONS_VAR, " "), (SLOW_STATEMENT_MS_VAR, "")]).unwrap(),
            DbPoolConfig::default()
        );
        assert_eq!(DbPoolConfig::default().max_connections, 5);
        assert_eq!(
            DbPoolConfig::default().slow_statement,
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn settings_override_pool_size_and_slow_threshold() {
        let config =
            config(&[(MAX_CONNECTIONS_VAR, "12"), (SLOW_STATEMENT_MS_VAR, "250")]).unwrap();
        assert_eq!(config.max_connections, 12);
        assert_eq!(config.slow_statement, Some(Duration::from_millis(250)));
    }

    #[test]
    fn zero_slow_threshold_turns_slow_statement_logging_off() {
        assert_eq!(
            config(&[(SLOW_STATEMENT_MS_VAR, "0")])
                .unwrap()
                .slow_statement,
            None
        );
    }

    #[test]
    fn invalid_settings_name_the_variable() {
        for bad in ["0", "-1", "lots"] {
            let error = config(&[(MAX_CONNECTIONS_VAR, bad)]).unwrap_err();
            assert!(error.contains(MAX_CONNECTIONS_VAR), "{error}");
        }
        let error = config(&[(SLOW_STATEMENT_MS_VAR, "fast")]).unwrap_err();
        assert!(error.contains(SLOW_STATEMENT_MS_VAR), "{error}");
    }
}
