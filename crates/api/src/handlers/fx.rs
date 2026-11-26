//! FX endpoints (Wave 34) — multi-currency groundwork.
//!
//! - `GET /fx/rates` — the configured rate table (platform
//!   settings; BDT-based, indicative for reporting).
//! - `GET /fx/convert?from=USD&to=BDT&amount_minor=10000` — convert
//!   a minor-unit amount with `miraclepay_domain::fx::convert`.
//!
//! Both require authentication (they expose platform
//! configuration); neither prices a payment — checkout amounts are
//! always taken in the payment's own currency.

use axum::extract::{Query, State};
use axum::response::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use miraclepay_domain::money::{Currency, Money};

use crate::auth_middleware::AuthUser;
use crate::dto::ApiError;
use crate::state::AppState;

/// `GET /fx/rates` — the configured FX table.
pub async fn list_rates(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let settings = state.settings.read().await;
    Ok(Json(json!({
        "base": "BDT",
        "rates": settings.fx_rates,
        "note": "Indicative rates for reporting/display only; payments are always taken in their own currency.",
    })))
}

/// Query for `GET /fx/convert`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConvertQuery {
    pub from: String,
    pub to: String,
    pub amount_minor: i64,
}

/// `GET /fx/convert` — convert a minor-unit amount between
/// currencies using the configured table.
pub async fn convert(
    _auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<ConvertQuery>,
) -> Result<Json<Value>, ApiError> {
    let from = Currency::from_code(&query.from.to_uppercase())
        .ok_or_else(|| ApiError::BadRequest(format!("unknown currency: {}", query.from)))?;
    let to = Currency::from_code(&query.to.to_uppercase())
        .ok_or_else(|| ApiError::BadRequest(format!("unknown currency: {}", query.to)))?;
    if query.amount_minor < 0 {
        return Err(ApiError::BadRequest(
            "amount_minor must be non-negative".to_string(),
        ));
    }
    let settings = state.settings.read().await;
    let converted = miraclepay_domain::fx::convert(
        Money::new(query.amount_minor, from),
        to,
        &settings.fx_rates,
    )
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(json!({
        "from": from.code(),
        "to": to.code(),
        "amount_minor": query.amount_minor,
        "converted_minor": converted.amount_minor,
        "formatted": converted.format_display(),
    })))
}
