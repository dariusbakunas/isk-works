use axum::http::{HeaderMap, HeaderValue};

use crate::auth::SESSION_COOKIE_NAME;
use iskworks_core::SESSION_TTL_DAYS;

/// Builds the `Set-Cookie` header for a freshly issued session. `secure`
/// should come from `AuthService::cookie_secure()` — plain HTTP local dev
/// can't set a `Secure` cookie and have the browser send it back.
pub(crate) fn session_cookie_header(raw_token: &str, secure: bool) -> HeaderValue {
    let max_age = SESSION_TTL_DAYS * 24 * 60 * 60;
    let secure_attr = if secure { "; Secure" } else { "" };
    let value = format!(
        "{SESSION_COOKIE_NAME}={raw_token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure_attr}"
    );
    HeaderValue::from_str(&value).expect("cookie value contains only ASCII produced by us")
}

/// Builds the `Set-Cookie` header that clears the session cookie on logout.
pub(crate) fn clear_session_cookie_header(secure: bool) -> HeaderValue {
    let secure_attr = if secure { "; Secure" } else { "" };
    let value =
        format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure_attr}");
    HeaderValue::from_str(&value).expect("cookie value contains only ASCII produced by us")
}

/// Binds an in-flight EVE OAuth flow to the browser that started it. Both
/// begin routes (login and character-link) set it to the flow's `state`, and
/// the callback refuses a `state` this browser doesn't hold. Without it, an
/// attacker could start a flow and get a victim's browser to finish it: a
/// login callback would sign the victim into the attacker's account, and a
/// character-link would put the victim's character tokens in the attacker's
/// workspace.
pub(crate) const OAUTH_STATE_COOKIE_NAME: &str = "iskworks_oauth_state";
const OAUTH_CALLBACK_PATH: &str = "/api/eve/oauth/callback";
/// Matches the pending-authorization rows' 10-minute lifetime.
const OAUTH_STATE_MAX_AGE_SECONDS: u32 = 10 * 60;

/// `SameSite=Lax`, not `Strict`: the callback arrives as a top-level
/// redirect from login.eveonline.com, and `Strict` would drop the cookie.
pub(crate) fn oauth_state_cookie_header(state: &str, secure: bool) -> HeaderValue {
    let secure_attr = if secure { "; Secure" } else { "" };
    let value = format!(
        "{OAUTH_STATE_COOKIE_NAME}={state}; Path={OAUTH_CALLBACK_PATH}; HttpOnly; SameSite=Lax; Max-Age={OAUTH_STATE_MAX_AGE_SECONDS}{secure_attr}"
    );
    HeaderValue::from_str(&value).expect("cookie value contains only ASCII produced by us")
}

pub(crate) fn clear_oauth_state_cookie_header(secure: bool) -> HeaderValue {
    let secure_attr = if secure { "; Secure" } else { "" };
    let value = format!(
        "{OAUTH_STATE_COOKIE_NAME}=; Path={OAUTH_CALLBACK_PATH}; HttpOnly; SameSite=Lax; Max-Age=0{secure_attr}"
    );
    HeaderValue::from_str(&value).expect("cookie value contains only ASCII produced by us")
}

/// Whether this browser holds the binding cookie for exactly `state`.
pub(crate) fn oauth_state_matches_cookie(headers: &HeaderMap, state: &str) -> bool {
    !state.is_empty()
        && extract_cookie(headers, OAUTH_STATE_COOKIE_NAME)
            .is_some_and(|cookie| constant_time_eq(cookie.as_bytes(), state.as_bytes()))
}

/// Extracts the raw session token from an incoming `Cookie` header, if present.
pub(crate) fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    extract_cookie(headers, SESSION_COOKIE_NAME)
}

fn extract_cookie(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == cookie_name).then(|| value.to_string())
    })
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_cookie_is_http_only_and_carries_the_raw_token() {
        let header = session_cookie_header("raw-token-value", false);
        let value = header.to_str().unwrap();
        assert!(value.starts_with("iskworks_session=raw-token-value;"));
        assert!(value.contains("HttpOnly"));
        assert!(value.contains("SameSite=Lax"));
        assert!(value.contains("Path=/"));
        assert!(!value.contains("Secure"));
    }

    #[test]
    fn session_cookie_carries_secure_only_when_requested() {
        let header = session_cookie_header("raw-token-value", true);
        assert!(header.to_str().unwrap().contains("; Secure"));
    }

    #[test]
    fn clear_session_cookie_expires_immediately() {
        let header = clear_session_cookie_header(false);
        let value = header.to_str().unwrap();
        assert!(value.starts_with("iskworks_session=;"));
        assert!(value.contains("Max-Age=0"));
    }

    #[test]
    fn extract_session_token_finds_it_among_other_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("other=1; iskworks_session=abc123; another=2"),
        );
        assert_eq!(extract_session_token(&headers), Some("abc123".to_string()));
    }

    #[test]
    fn oauth_state_cookie_is_short_lived_and_scoped_to_the_callback() {
        let header = oauth_state_cookie_header("state-value", false);
        let value = header.to_str().unwrap();
        assert!(value.starts_with("iskworks_oauth_state=state-value;"));
        assert!(value.contains("Path=/api/eve/oauth/callback"));
        assert!(value.contains("HttpOnly"));
        // Lax (not Strict): the callback arrives as a top-level redirect
        // from login.eveonline.com, which Strict would strip the cookie from.
        assert!(value.contains("SameSite=Lax"));
        assert!(value.contains("Max-Age=600"));
        assert!(!value.contains("Secure"));
        assert!(oauth_state_cookie_header("state-value", true)
            .to_str()
            .unwrap()
            .contains("; Secure"));
    }

    #[test]
    fn clear_oauth_state_cookie_expires_immediately_on_the_same_path() {
        let header = clear_oauth_state_cookie_header(false);
        let value = header.to_str().unwrap();
        assert!(value.starts_with("iskworks_oauth_state=;"));
        assert!(value.contains("Path=/api/eve/oauth/callback"));
        assert!(value.contains("Max-Age=0"));
    }

    #[test]
    fn oauth_state_matches_only_the_cookie_for_that_exact_state() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("iskworks_session=abc; iskworks_oauth_state=s1"),
        );
        assert!(oauth_state_matches_cookie(&headers, "s1"));
        assert!(!oauth_state_matches_cookie(&headers, "s2"));
        assert!(!oauth_state_matches_cookie(&headers, ""));
        assert!(!oauth_state_matches_cookie(&HeaderMap::new(), "s1"));
    }

    #[test]
    fn extract_session_token_is_none_when_absent() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("other=1"),
        );
        assert_eq!(extract_session_token(&headers), None);

        assert_eq!(extract_session_token(&HeaderMap::new()), None);
    }
}
