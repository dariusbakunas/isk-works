use super::*;

#[derive(Deserialize)]
pub(super) struct TokenResponse {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_in: i64,
}

/// RFC 6749 §5.2 token-endpoint error body. Only `error` is read; the
/// description is free text and never logged.
#[derive(Deserialize)]
pub(super) struct TokenErrorResponse {
    pub(super) error: String,
}

#[derive(Deserialize)]
pub(super) struct Claims {
    pub(super) sub: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) scp: ScopeClaim,
    pub(super) aud: AudienceClaim,
    #[serde(default)]
    pub(super) owner: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum AudienceClaim {
    One(String),
    Many(Vec<String>),
}

impl AudienceClaim {
    pub(super) fn contains(&self, expected: &str) -> bool {
        match self {
            Self::One(value) => value == expected,
            Self::Many(values) => values.iter().any(|value| value == expected),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ScopeClaim {
    One(String),
    Many(Vec<String>),
}

impl Default for ScopeClaim {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl ScopeClaim {
    pub(super) fn into_set(self) -> BTreeSet<String> {
        match self {
            Self::One(value) => value.split_whitespace().map(str::to_string).collect(),
            Self::Many(values) => values.into_iter().collect(),
        }
    }
}
