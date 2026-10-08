use super::*;

pub(super) fn classify_status(status: StatusCode, response: &reqwest::Response) -> EsiError {
    match status {
        StatusCode::UNAUTHORIZED => EsiError::AuthorizationRequired,
        StatusCode::FORBIDDEN => EsiError::AccessDenied,
        // 420: this IP spent ESI's error budget (see `error_limit`).
        status if status.as_u16() == ERROR_LIMITED_STATUS => EsiError::EsiErrorLimit {
            reset_seconds: response
                .headers()
                .get("x-esi-error-limit-reset")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse().ok()),
        },
        StatusCode::TOO_MANY_REQUESTS | StatusCode::IM_A_TEAPOT => EsiError::RateLimited {
            retry_after_seconds: rate_limit::retry_after(response.headers(), Utc::now())
                .map(|wait| wait.as_secs().max(1)),
        },
        status if status.is_server_error() => EsiError::TemporaryFailure,
        _ => EsiError::PermanentFailure,
    }
}

pub(super) fn request_failure_category(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_builder() {
        "builder"
    } else if error.is_redirect() {
        "redirect"
    } else if error.is_body() {
        "body"
    } else if error.is_decode() {
        "decode"
    } else if error.is_status() {
        "status"
    } else {
        "request"
    }
}

pub(super) fn classify_request_error(error: &reqwest::Error) -> EsiError {
    if error.is_builder() || error.is_redirect() {
        EsiError::PermanentFailure
    } else {
        EsiError::TemporaryFailure
    }
}

pub(super) fn request_failure_cause(error: &reqwest::Error) -> &'static str {
    let mut current: &(dyn std::error::Error + 'static) = error;
    loop {
        let message = current.to_string().to_ascii_lowercase();
        if message.contains("connection closed") {
            return "connection_closed";
        }
        if message.contains("connection reset") || message.contains("reset by peer") {
            return "connection_reset";
        }
        if message.contains("incomplete message") {
            return "incomplete_message";
        }
        if message.contains("broken pipe") {
            return "broken_pipe";
        }
        if message.contains("http2") || message.contains("http/2") {
            return "http2";
        }
        if message.contains("dns") || message.contains("name resolution") {
            return "dns";
        }
        if message.contains("tls")
            || message.contains("certificate")
            || message.contains("handshake")
        {
            return "tls";
        }
        let Some(source) = current.source() else {
            return "unknown";
        };
        current = source;
    }
}

pub(super) fn http_failure_category(status: StatusCode) -> &'static str {
    if status.is_server_error() {
        "http_server"
    } else if status.is_client_error() {
        "http_client"
    } else {
        "http"
    }
}

pub(super) fn header_u64(
    response: &reqwest::Response,
    name: impl header::AsHeaderName,
) -> Option<u64> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}
