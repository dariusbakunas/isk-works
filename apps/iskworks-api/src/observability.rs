//! Request instrumentation that ties backend logs to a LogRocket session.
//!
//! The web client (`web/iskworks-web/src/observability/logrocket.ts`)
//! attaches an `X-LogRocket-URL` header carrying the current session-replay
//! URL to every same-origin `/api/*` request. This middleware lifts that
//! URL into a `tracing` span wrapping the request, so every log line the
//! request emits -- most usefully the redacted-internal-error diagnostic in
//! `error::log_internal_failure` -- carries the session URL alongside its
//! correlation id. Given a backend error you can open the exact replay;
//! given a replay you can grep the logs.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;

/// Header the web client sets to the value of `LogRocket.getSessionURL()`.
const LOGROCKET_URL_HEADER: &str = "x-logrocket-url";

/// Canonical prefix of every LogRocket session URL. Requiring it means a
/// forged header can only inject text that already looks like a LogRocket
/// link, never arbitrary log content.
const LOGROCKET_URL_PREFIX: &str = "https://app.logrocket.com/";

/// Longest header value echoed into a span. A real session URL is well
/// under this; the bound caps how much a spoofed header can bloat a line.
const MAX_LEN: usize = 300;

/// Wrap the request in a `logrocket` span carrying the client's session
/// URL, when the `X-LogRocket-URL` header is present and well-formed.
/// A missing or rejected header is a no-op -- the request proceeds
/// untouched and simply has no session URL in its logs.
pub async fn tag_logrocket_session(request: Request, next: Next) -> Response {
    let session_url = request
        .headers()
        .get(LOGROCKET_URL_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(sanitize_logrocket_url);

    match session_url {
        Some(url) => {
            let span = tracing::info_span!("logrocket", session_url = %url);
            next.run(request).instrument(span).await
        }
        None => next.run(request).await,
    }
}

/// Accept a value only if it is a plausibly-real LogRocket session URL: the
/// canonical host prefix, a bounded length, and a conservative URL
/// character set (no whitespace, quotes, or control bytes that could
/// derail a log line). Returns the trimmed, owned string on success.
fn sanitize_logrocket_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.len() > MAX_LEN {
        return None;
    }
    let rest = raw.strip_prefix(LOGROCKET_URL_PREFIX)?;
    if rest.is_empty() {
        return None;
    }
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b"-_./?=&%:@~+".contains(&b);
    if rest.bytes().all(allowed) {
        Some(raw.to_owned())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_logrocket_url;

    #[test]
    fn accepts_a_session_url() {
        let url = "https://app.logrocket.com/example-org/example-app/s/6-00000000-0000-0000-0000-000000000000/0?t=1700000000000";
        assert_eq!(sanitize_logrocket_url(url), Some(url.to_owned()));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let url = "https://app.logrocket.com/example-org/example-app/s/abc/0";
        assert_eq!(
            sanitize_logrocket_url(&format!("  {url}  ")),
            Some(url.to_owned())
        );
    }

    #[test]
    fn rejects_a_foreign_host() {
        assert_eq!(
            sanitize_logrocket_url("https://evil.example.com/example-org/x"),
            None
        );
    }

    #[test]
    fn rejects_the_bare_prefix() {
        assert_eq!(sanitize_logrocket_url("https://app.logrocket.com/"), None);
    }

    #[test]
    fn rejects_log_injection_characters() {
        assert_eq!(
            sanitize_logrocket_url(
                "https://app.logrocket.com/x y\nERROR forged line correlation_id=deadbeef"
            ),
            None
        );
    }

    #[test]
    fn rejects_an_over_long_value() {
        let long = format!("https://app.logrocket.com/{}", "a".repeat(400));
        assert_eq!(sanitize_logrocket_url(&long), None);
    }
}
