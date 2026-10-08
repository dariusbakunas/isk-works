//! Refuses state-changing requests that a browser sent from another origin.
//!
//! The session cookie is `SameSite=Lax`, which keeps it off cross-*site*
//! requests but not cross-*origin* ones from the same site: a page on
//! `stage.isk-works.com` is same-site with `isk-works.com`, so the browser
//! attaches the prod cookie to a form POST from stage. CORS doesn't help
//! either: it hides the response from the caller, but a "simple" request
//! (a form POST, or a fetch with no body or a `text/plain` one) is still
//! sent and still acts. Routes with no JSON body (logout, sync, refresh,
//! disconnect) are exposed that way. So the API checks where the request
//! came from itself.
//!
//! Only enforced when EVE SSO is configured: without an `AuthService` there
//! is no session cookie for a forged request to ride on.

use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::error::ApiError;
use crate::state::AppState;

pub(crate) async fn reject_cross_origin_writes(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if state.auth_service.is_some()
        && is_cross_origin_write(request.method(), request.headers(), &state.web_app_origin)
    {
        tracing::warn!(
            method = %request.method(),
            path = %request.uri().path(),
            origin = ?request.headers().get(header::ORIGIN),
            sec_fetch_site = ?request.headers().get("sec-fetch-site"),
            "refused a cross-origin state-changing request"
        );
        return ApiError::Forbidden.into_response();
    }
    next.run(request).await
}

/// Whether a state-changing request came from a browser page on an origin
/// other than `web_app_origin`. Browsers send `Origin` on every non-GET
/// request, so that decides it when present. Without it, `Sec-Fetch-Site`
/// (also browser-set and unforgeable from page script) is the fallback. A
/// request with neither is not from a browser (curl, scripts), which cannot
/// carry a victim's cookie anyway, so it passes.
pub(crate) fn is_cross_origin_write(
    method: &Method,
    headers: &HeaderMap,
    web_app_origin: &str,
) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return false;
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let allowed = web_app_origin.trim_end_matches('/');
        return !origin
            .to_str()
            .is_ok_and(|origin| origin.eq_ignore_ascii_case(allowed));
    }
    match headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        Some(site) => !matches!(site, "same-origin" | "none"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    const APP: &str = "https://isk-works.com";

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_static(value));
        }
        map
    }

    #[test]
    fn a_write_from_the_app_origin_passes() {
        let h = headers(&[("origin", "https://isk-works.com")]);
        assert!(!is_cross_origin_write(&Method::POST, &h, APP));
        assert!(!is_cross_origin_write(&Method::DELETE, &h, APP));
    }

    #[test]
    fn a_trailing_slash_on_the_configured_url_and_case_do_not_matter() {
        let h = headers(&[("origin", "https://ISK-WORKS.com")]);
        assert!(!is_cross_origin_write(
            &Method::POST,
            &h,
            "https://isk-works.com/"
        ));
    }

    #[test]
    fn a_write_from_a_same_site_subdomain_is_refused() {
        let h = headers(&[("origin", "https://stage.isk-works.com")]);
        assert!(is_cross_origin_write(&Method::POST, &h, APP));
    }

    #[test]
    fn a_write_from_another_site_or_an_opaque_origin_is_refused() {
        for origin in ["https://evil.example", "null", "http://isk-works.com"] {
            let mut h = HeaderMap::new();
            h.insert("origin", HeaderValue::from_str(origin).unwrap());
            assert!(is_cross_origin_write(&Method::PUT, &h, APP), "{origin}");
        }
    }

    #[test]
    fn reads_are_never_refused() {
        let h = headers(&[
            ("origin", "https://evil.example"),
            ("sec-fetch-site", "cross-site"),
        ]);
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!is_cross_origin_write(&method, &h, APP), "{method}");
        }
    }

    #[test]
    fn without_origin_sec_fetch_site_decides() {
        for (site, refused) in [
            ("same-origin", false),
            ("none", false),
            ("same-site", true),
            ("cross-site", true),
        ] {
            let mut h = HeaderMap::new();
            h.insert("sec-fetch-site", HeaderValue::from_static(site));
            assert_eq!(
                is_cross_origin_write(&Method::POST, &h, APP),
                refused,
                "{site}"
            );
        }
    }

    #[test]
    fn a_request_with_neither_header_is_not_from_a_browser_and_passes() {
        assert!(!is_cross_origin_write(
            &Method::POST,
            &HeaderMap::new(),
            APP
        ));
    }
}

#[cfg(test)]
mod router_tests {
    use crate::{build_router, AppState, AuthService};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};

    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::test_support::EmptyWorkspaceRepository;

    fn state(with_auth: bool) -> AppState {
        let state = AppState::new(Arc::new(EmptyWorkspaceRepository))
            .with_web_app_origin("https://isk-works.com".to_string());
        if with_auth {
            state.with_auth(Some(AuthService::new_unconnected_for_tests()))
        } else {
            state
        }
    }

    async fn post_workspace(state: AppState, origin: &str) -> StatusCode {
        build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/workspace")
                    .header("origin", origin)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn a_write_from_a_same_site_origin_is_refused_before_the_session_gate() {
        assert_eq!(
            post_workspace(state(true), "https://stage.isk-works.com").await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn a_write_from_the_app_origin_reaches_the_session_gate() {
        // No session cookie, so the session gate answers 401: the origin
        // check let it through.
        assert_eq!(
            post_workspace(state(true), "https://isk-works.com").await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn the_anonymous_logout_route_is_covered_too() {
        let status = build_router(state(true))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/logout")
                    .header("origin", "https://stage.isk-works.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn without_auth_configured_nothing_is_refused() {
        // Legacy/dev mode has no session cookie to protect, and the Vite dev
        // server may be opened on an origin other than WEB_APP_URL.
        assert_ne!(
            post_workspace(state(false), "https://stage.isk-works.com").await,
            StatusCode::FORBIDDEN
        );
    }
}
