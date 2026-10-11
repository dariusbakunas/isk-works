//! Request rate and latency per API route (issue #14), so a slow page can be
//! traced to the endpoint behind it.

use axum::extract::{MatchedPath, Request};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

const REQUESTS: &str = "iskworks_http_requests_total";
const DURATION: &str = "iskworks_http_request_duration_seconds";
/// The route label for requests no route matched (the 404 fallback).
const UNMATCHED: &str = "unmatched";

/// Records each request under its route template (`/api/builds/:build_id`)
/// and never its raw path, which would carry IDs and grow without bound.
/// Layered outermost, so the time includes session resolution. It stops at
/// the response head; streamed bodies (CSV exports) aren't covered.
pub(crate) async fn record_request(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| UNMATCHED.to_owned());
    let method = method_label(request.method());
    let started = std::time::Instant::now();

    let response = next.run(request).await;

    let status_class = match response.status().as_u16() {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        _ => "5xx",
    };
    metrics::histogram!(DURATION, "route" => route.clone(), "method" => method)
        .record(started.elapsed().as_secs_f64());
    metrics::counter!(
        REQUESTS,
        "route" => route,
        "method" => method,
        "status_class" => status_class
    )
    .increment(1);
    response
}

/// Standard methods only: hyper accepts any token as a method, and the label
/// must stay a closed set however a client calls us.
fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_methods_share_one_label() {
        assert_eq!(method_label(&Method::PATCH), "PATCH");
        assert_eq!(method_label(&Method::from_bytes(b"BREW").unwrap()), "other");
        assert_eq!(method_label(&Method::TRACE), "other");
    }
}
