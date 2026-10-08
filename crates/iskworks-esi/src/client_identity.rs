//! How ISK Works identifies itself to ESI and EVE SSO. CCP asks every
//! application to send a `User-Agent` naming the app, its version, and a way
//! to contact the operator, so a misbehaving install can be reached instead
//! of blocked. Every self-hosted install is its own operator, so the contact
//! comes from that install's environment (`ISKWORKS_ESI_CONTACT`).

use std::sync::OnceLock;

/// ESI's response-shape contract. ESI answers requests without this header
/// using its oldest date, which is what every parser in `transport` was
/// written against -- pinning it keeps responses unchanged. Moving it
/// forward is a deliberate, tested upgrade, never a side effect.
pub const COMPATIBILITY_DATE: &str = "2020-01-01";

/// The project's canonical source, used when `ISKWORKS_SOURCE_URL` is unset.
pub const UPSTREAM_SOURCE_URL: &str = "https://github.com/dariusbakunas/isk-works";

const PRODUCT: &str = "ISKWorks";

/// Formats the `User-Agent`, e.g.
/// `ISKWorks/1.2.3 (ops@example.com; +https://github.com/you/fork)`.
///
/// `app_version` is the raw `APP_VERSION` (`"v1.2.3 (a1b2c3d)"` in released
/// images); only its first word, minus a leading `v`, becomes the product
/// version. `contact` is dropped (and reported) when it can't be carried in
/// a header comment.
#[must_use]
pub fn user_agent(
    app_version: Option<&str>,
    contact: Option<&str>,
    source_url: Option<&str>,
) -> String {
    let version = app_version
        .and_then(|raw| raw.split_whitespace().next())
        .map(|word| word.strip_prefix('v').unwrap_or(word))
        .filter(|word| !word.is_empty() && word.chars().all(is_token_char))
        .unwrap_or("dev");
    let source_url = source_url
        .map(str::trim)
        .filter(|url| !url.is_empty() && url.chars().all(is_comment_char))
        .unwrap_or(UPSTREAM_SOURCE_URL);
    match contact
        .map(str::trim)
        .filter(|contact| is_usable_contact(contact))
    {
        Some(contact) => format!("{PRODUCT}/{version} ({contact}; +{source_url})"),
        None => format!("{PRODUCT}/{version} (+{source_url})"),
    }
}

/// Whether `ISKWORKS_ESI_CONTACT` holds a contact that will be sent.
#[must_use]
pub fn contact_configured() -> bool {
    env_value("ISKWORKS_ESI_CONTACT").is_some_and(|contact| is_usable_contact(&contact))
}

/// The process's `User-Agent`, read from the environment once.
pub(crate) fn process_user_agent() -> &'static str {
    static USER_AGENT: OnceLock<String> = OnceLock::new();
    USER_AGENT.get_or_init(|| {
        user_agent(
            env_value("APP_VERSION").as_deref(),
            env_value("ISKWORKS_ESI_CONTACT").as_deref(),
            env_value("ISKWORKS_SOURCE_URL").as_deref(),
        )
    })
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn is_usable_contact(contact: &str) -> bool {
    !contact.is_empty() && contact.chars().all(|c| is_comment_char(c) && c != ';')
}

/// RFC 9110 `tchar`: what a product version may contain.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c)
}

/// Printable ASCII that can't close or nest the `( ... )` comment.
fn is_comment_char(c: char) -> bool {
    (' '..='~').contains(&c) && c != '(' && c != ')' && c != '\\'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_released_image_names_its_version_contact_and_source() {
        assert_eq!(
            user_agent(
                Some("v1.2.3 (a1b2c3d)"),
                Some("ops@example.com"),
                Some("https://github.com/you/fork"),
            ),
            "ISKWorks/1.2.3 (ops@example.com; +https://github.com/you/fork)"
        );
    }

    #[test]
    fn unset_values_fall_back_to_dev_and_the_upstream_source() {
        assert_eq!(
            user_agent(None, None, None),
            "ISKWorks/dev (+https://github.com/dariusbakunas/isk-works)"
        );
        assert_eq!(
            user_agent(Some("dev"), Some("  "), Some("")),
            "ISKWorks/dev (+https://github.com/dariusbakunas/isk-works)"
        );
    }

    #[test]
    fn values_that_would_break_the_header_are_dropped() {
        assert_eq!(
            user_agent(
                Some("v1/2"),
                Some("me (admin); evil\r\nX-Injected: 1"),
                Some("https://example.com/)"),
            ),
            "ISKWorks/dev (+https://github.com/dariusbakunas/isk-works)"
        );
    }

    #[test]
    fn the_contact_may_be_a_discord_handle_or_character_name() {
        assert_eq!(
            user_agent(Some("v2.0.0"), Some("Discord: someone"), None),
            "ISKWorks/2.0.0 (Discord: someone; +https://github.com/dariusbakunas/isk-works)"
        );
    }
}
