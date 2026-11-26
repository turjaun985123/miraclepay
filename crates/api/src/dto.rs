//! Shared API error type and small response helpers for MiraclePay.
//!
//! Original clean-room implementation for MiraclePay v0.0.1.
//!
//! Handlers across the API return [`ApiResult`]; any [`ApiError`] they
//! produce is rendered by its [`IntoResponse`] implementation as a
//! JSON body of the shape `{"error": "<message>"}` with the HTTP
//! status that matches the variant. Errors from the domain and
//! gateway layers convert into [`ApiError`] through the `From`
//! implementations in this module, so handlers can use `?` on those
//! results without hand-writing a mapping at every call site.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use miraclepay_domain::errors::DomainError;
use miraclepay_gateways::types::GatewayError;

/// Errors an API handler can return.
///
/// The `String` payloads are human-readable messages that are safe to
/// show to the caller — with one exception: [`ApiError::Internal`]
/// may hold internal detail, so its payload is only logged (see the
/// [`IntoResponse`] implementation) and never sent in the body.
#[derive(Debug)]
pub enum ApiError {
    /// The request was malformed or failed validation — HTTP 400.
    BadRequest(String),
    /// The caller is not authenticated — HTTP 401.
    Unauthorized,
    /// Login needs a TOTP code that was not supplied — HTTP 401
    /// with message `totp_required` (Wave 26).
    TotpRequired,
    /// The supplied TOTP code did not verify — HTTP 401 with
    /// message `totp_invalid` (Wave 26).
    TotpInvalid,
    /// The caller is authenticated but not allowed — HTTP 403.
    Forbidden(String),
    /// The requested resource does not exist — HTTP 404.
    NotFound(String),
    /// The request conflicts with current state — HTTP 409.
    Conflict(String),
    /// An unexpected internal failure — HTTP 500. The payload is
    /// logged server-side only; clients see `"internal error"`.
    Internal(String),
}

impl ApiError {
    /// The stable machine-readable error code for this variant
    /// (Wave 45). Codes never change within API v1; the human
    /// message in `error` may be reworded. The full catalogue is
    /// `docs/errors.md`.
    pub fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest(_) => "bad_request",
            ApiError::Unauthorized => "unauthorized",
            ApiError::TotpRequired => "totp_required",
            ApiError::TotpInvalid => "totp_invalid",
            ApiError::Forbidden(_) => "forbidden",
            ApiError::NotFound(_) => "not_found",
            ApiError::Conflict(_) => "conflict",
            ApiError::Internal(_) => "internal_error",
        }
    }

    /// The HTTP status this variant maps to.
    pub fn status(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized | ApiError::TotpRequired | ApiError::TotpInvalid => {
                StatusCode::UNAUTHORIZED
            }
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.code();
        let (status, message) = match self {
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            ApiError::TotpRequired => (StatusCode::UNAUTHORIZED, "totp_required".to_string()),
            ApiError::TotpInvalid => (StatusCode::UNAUTHORIZED, "totp_invalid".to_string()),
            ApiError::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, message),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, message),
            ApiError::Internal(detail) => {
                // Log the real detail server-side, but never leak
                // internals to the client.
                eprintln!("internal error: {detail}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };

        // Wave 45 envelope: `error` (human message) stays exactly
        // as before for compatibility; `code` (stable) and
        // `status` are additive.
        (
            status,
            Json(serde_json::json!({
                "error": message,
                "code": code,
                "status": status.as_u16(),
            })),
        )
            .into_response()
    }
}

impl From<DomainError> for ApiError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::NotFound(message) => ApiError::NotFound(message),
            DomainError::Validation(message) => ApiError::BadRequest(message),
            DomainError::Unauthorized => ApiError::Unauthorized,
            DomainError::Forbidden => ApiError::Forbidden("forbidden".to_string()),
            DomainError::Conflict(message) => ApiError::Conflict(message),
            DomainError::Internal(message) => ApiError::Internal(message),
            // InvalidAmount, CurrencyMismatch, and every remaining
            // domain failure are caller-facing problems with the
            // request, reported with the error's Display message.
            other => ApiError::BadRequest(other.to_string()),
        }
    }
}

impl From<GatewayError> for ApiError {
    fn from(error: GatewayError) -> Self {
        match error {
            // Provider and network failures are not the caller's
            // fault and their details must not leak: surface a
            // generic internal error (the IntoResponse impl logs the
            // generic message and returns "internal error").
            GatewayError::Provider(_) | GatewayError::Network(_) => {
                ApiError::Internal("gateway error".to_string())
            }
            // NotConfigured, Unsupported, and InvalidConfig are all
            // problems with how the gateway was asked to operate, so
            // they are reported as bad requests with the Display
            // message.
            other => ApiError::BadRequest(other.to_string()),
        }
    }
}

/// Convenience alias for fallible API handlers.
pub type ApiResult<T> = Result<T, ApiError>;

/// Wrap a serializable value in a `200 OK` JSON response.
///
/// The value is converted to a [`serde_json::Value`] first so callers
/// get a uniform `(StatusCode, Json<Value>)` return type regardless
/// of the concrete payload type.
pub fn ok<T: Serialize>(value: T) -> (StatusCode, Json<Value>) {
    let body = serde_json::to_value(value).unwrap_or(Value::Null);
    (StatusCode::OK, Json(body))
}

/// Response body for the health-check endpoint.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct HealthResponse {
    /// Service status, always `"ok"` while the service is serving.
    pub status: &'static str,
    /// Service version.
    pub version: &'static str,
    /// Service name.
    pub name: &'static str,
}

/// The current health response: MiraclePay v0.0.1, up and serving.
pub fn health() -> HealthResponse {
    HealthResponse {
        status: "ok",
        version: "0.0.2",
        name: "MiraclePay",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn bad_request_maps_to_400() {
        let response = ApiError::BadRequest("bad input".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn unauthorized_maps_to_401() {
        let response = ApiError::Unauthorized.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn forbidden_maps_to_403() {
        let response = ApiError::Forbidden("forbidden".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn not_found_maps_to_404() {
        let response = ApiError::NotFound("missing".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn conflict_maps_to_409() {
        let response = ApiError::Conflict("already exists".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn internal_maps_to_500() {
        let response = ApiError::Internal("secret detail".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn health_reports_miraclepay_v0_0_1() {
        let health = health();
        assert_eq!(health.status, "ok");
        assert_eq!(health.version, "0.0.2");
        assert_eq!(health.name, "MiraclePay");
    }

    #[test]
    fn domain_error_conversions_map_to_expected_statuses() {
        let cases: Vec<(DomainError, StatusCode)> = vec![
            (
                DomainError::NotFound("invoice inv_1".to_string()),
                StatusCode::NOT_FOUND,
            ),
            (
                DomainError::Validation("email is malformed".to_string()),
                StatusCode::BAD_REQUEST,
            ),
            (DomainError::Unauthorized, StatusCode::UNAUTHORIZED),
            (DomainError::Forbidden, StatusCode::FORBIDDEN),
            (
                DomainError::Conflict("invoice already paid".to_string()),
                StatusCode::CONFLICT,
            ),
            (DomainError::InvalidAmount, StatusCode::BAD_REQUEST),
            (DomainError::CurrencyMismatch, StatusCode::BAD_REQUEST),
            (
                DomainError::Internal("boom".to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (error, expected) in cases {
            let response = ApiError::from(error).into_response();
            assert_eq!(response.status(), expected);
        }
    }

    #[test]
    fn gateway_error_conversions_map_to_expected_statuses() {
        let cases: Vec<(GatewayError, StatusCode)> = vec![
            (
                GatewayError::NotConfigured("bkash".to_string()),
                StatusCode::BAD_REQUEST,
            ),
            (
                GatewayError::Unsupported("refund".to_string()),
                StatusCode::BAD_REQUEST,
            ),
            (
                GatewayError::InvalidConfig("missing key".to_string()),
                StatusCode::BAD_REQUEST,
            ),
            (
                GatewayError::Provider("provider rejected".to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                GatewayError::Network("timeout".to_string()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (error, expected) in cases {
            let response = ApiError::from(error).into_response();
            assert_eq!(response.status(), expected);
        }
    }
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    #[test]
    fn every_variant_has_a_stable_code_and_status() {
        let cases = [
            (ApiError::BadRequest("x".into()), "bad_request", 400),
            (ApiError::Unauthorized, "unauthorized", 401),
            (ApiError::TotpRequired, "totp_required", 401),
            (ApiError::TotpInvalid, "totp_invalid", 401),
            (ApiError::Forbidden("x".into()), "forbidden", 403),
            (ApiError::NotFound("x".into()), "not_found", 404),
            (ApiError::Conflict("x".into()), "conflict", 409),
            (ApiError::Internal("x".into()), "internal_error", 500),
        ];
        for (error, code, status) in cases {
            assert_eq!(error.code(), code);
            assert_eq!(error.status().as_u16(), status);
        }
    }
}
