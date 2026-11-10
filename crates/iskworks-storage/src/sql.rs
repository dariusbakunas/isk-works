//! SQL text helpers shared across repositories.

/// Escapes `value` for use inside a `LIKE`/`ILIKE` pattern, so `%`, `_` and
/// `\` match themselves. Relies on Postgres' default `LIKE` escape (`\`).
pub(crate) fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// A `LIKE` pattern matching `value` anywhere, literally.
pub(crate) fn like_contains(value: &str) -> String {
    format!("%{}%", escape_like(value))
}

/// A `LIKE` pattern matching `value` at the start, literally.
pub(crate) fn like_prefix(value: &str) -> String {
    format!("{}%", escape_like(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_and_the_escape_character_match_themselves() {
        assert_eq!(escape_like(r"50%_off\"), r"50\%\_off\\");
        assert_eq!(like_contains("a_b"), r"%a\_b%");
        assert_eq!(like_prefix("100%"), r"100\%%");
        assert_eq!(like_contains("Rifter"), "%Rifter%");
    }
}
