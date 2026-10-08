//! Spreadsheet formula injection guard for CSV exports. Player-chosen names
//! end up in exports (structure names in Location, container names, ...),
//! and a cell like `=HYPERLINK(...)` would run as a formula when the file is
//! opened in Excel or Sheets. Such cells get a leading `'`, which spreadsheets
//! treat as "this is text". Plain numbers -- including negative ISK amounts
//! -- are left alone so they stay numeric.

use std::borrow::Cow;

pub(crate) fn safe_cell(value: &str) -> Cow<'_, str> {
    let starts_like_formula = value
        .chars()
        .next()
        .is_some_and(|first| matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r'));
    if starts_like_formula && value.parse::<f64>().is_err() {
        Cow::Owned(format!("'{value}"))
    } else {
        Cow::Borrowed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::safe_cell;

    #[test]
    fn formula_like_text_is_neutralized() {
        for value in [
            "=HYPERLINK(\"http://evil\",\"Jita\")",
            "+1 Station",
            "-Rogue Citadel",
            "@SUM(A1)",
            "\t=1+1",
            "\r=1+1",
        ] {
            assert_eq!(safe_cell(value), format!("'{value}"), "{value:?}");
        }
    }

    #[test]
    fn numbers_and_ordinary_text_are_untouched() {
        for value in [
            "-1500.25",
            "+42",
            "-0",
            "1e5",
            "Jita IV - Moon 4 - Caldari Navy Assembly Plant",
            "",
        ] {
            assert_eq!(safe_cell(value), value, "{value:?}");
        }
    }
}
