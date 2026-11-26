//! Fraud signals (Wave 39) — a small, explainable rule engine.
//!
//! Every signal is a named rule with a weight; the assessment is
//! the sum, bucketed into low / medium / high. Rules consume
//! *precomputed aggregates* (counts and medians the caller derives
//! from the transaction store) so this module stays pure and
//! testable. Signals never block a payment by themselves in
//! v0.0.1 — they surface flagged payments to admins for review.
//! That is deliberate: a false positive must not strand a real
//! customer's money.

use serde::{Deserialize, Serialize};

/// Aggregates the caller computes for one candidate payment.
#[derive(Debug, Clone, Copy)]
pub struct RiskInput {
    /// The payment amount in minor units.
    pub amount_minor: i64,
    /// Payments this merchant created in the last 10 minutes.
    pub merchant_payments_last_10m: u32,
    /// Completed payments from this sender number across *other*
    /// merchants in the last 24 hours (sender reuse across shops
    /// is a classic mule pattern).
    pub sender_cross_merchant_24h: u32,
    /// The merchant's median completed amount in minor units, if
    /// they have enough history to have one.
    pub merchant_median_minor: Option<i64>,
}

/// One fired rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FraudSignal {
    pub code: String,
    pub weight: u32,
    pub detail: String,
}

/// Risk bucket for a total score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// The outcome of assessing one payment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub score: u32,
    pub level: RiskLevel,
    pub signals: Vec<FraudSignal>,
}

/// ৳50,000 — above this a single payment is "high value" for a
/// typical BD merchant.
pub const HIGH_VALUE_MINOR: i64 = 50_000_00;
/// Payments per merchant per 10 minutes that look like a burst.
pub const VELOCITY_THRESHOLD: u32 = 10;
/// Cross-merchant sender completions in 24 h that look like a mule.
pub const SENDER_REUSE_THRESHOLD: u32 = 3;
/// Multiple of the merchant median that counts as an outlier.
pub const OUTLIER_MULTIPLE: i64 = 5;

/// Assess one payment from its aggregates.
pub fn assess(input: &RiskInput) -> RiskAssessment {
    let mut signals = Vec::new();

    if input.amount_minor >= HIGH_VALUE_MINOR {
        signals.push(FraudSignal {
            code: "high_value".to_string(),
            weight: 20,
            detail: format!(
                "amount {} minor is at or above the high-value threshold {}",
                input.amount_minor, HIGH_VALUE_MINOR
            ),
        });
    }
    if input.merchant_payments_last_10m >= VELOCITY_THRESHOLD {
        signals.push(FraudSignal {
            code: "velocity".to_string(),
            weight: 30,
            detail: format!(
                "merchant created {} payments in 10 minutes",
                input.merchant_payments_last_10m
            ),
        });
    }
    if input.sender_cross_merchant_24h >= SENDER_REUSE_THRESHOLD {
        signals.push(FraudSignal {
            code: "sender_reuse".to_string(),
            weight: 40,
            detail: format!(
                "sender completed payments at {} other merchants in 24h",
                input.sender_cross_merchant_24h
            ),
        });
    }
    if let Some(median) = input.merchant_median_minor {
        if median > 0 && input.amount_minor > median.saturating_mul(OUTLIER_MULTIPLE) {
            signals.push(FraudSignal {
                code: "amount_outlier".to_string(),
                weight: 25,
                detail: format!(
                    "amount {} is over {}x the merchant median {}",
                    input.amount_minor, OUTLIER_MULTIPLE, median
                ),
            });
        }
    }

    let score: u32 = signals.iter().map(|signal| signal.weight).sum();
    let level = if score >= 60 {
        RiskLevel::High
    } else if score >= 30 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };
    RiskAssessment { score, level, signals }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calm() -> RiskInput {
        RiskInput {
            amount_minor: 500_00,
            merchant_payments_last_10m: 1,
            sender_cross_merchant_24h: 0,
            merchant_median_minor: Some(450_00),
        }
    }

    #[test]
    fn an_ordinary_payment_scores_zero() {
        let assessment = assess(&calm());
        assert_eq!(assessment.score, 0);
        assert_eq!(assessment.level, RiskLevel::Low);
        assert!(assessment.signals.is_empty());
    }

    #[test]
    fn signals_stack_into_levels() {
        let mut input = calm();
        input.amount_minor = 60_000_00; // high_value 20 + outlier 25 = 45
        let assessment = assess(&input);
        assert_eq!(assessment.score, 45);
        assert_eq!(assessment.level, RiskLevel::Medium);

        input.sender_cross_merchant_24h = 4; // + 40 = 85
        let assessment = assess(&input);
        assert_eq!(assessment.score, 85);
        assert_eq!(assessment.level, RiskLevel::High);
        assert_eq!(assessment.signals.len(), 3);
    }

    #[test]
    fn velocity_alone_is_medium() {
        let mut input = calm();
        input.merchant_payments_last_10m = 25;
        let assessment = assess(&input);
        assert_eq!(assessment.level, RiskLevel::Medium);
        assert_eq!(assessment.signals[0].code, "velocity");
    }

    #[test]
    fn no_median_means_no_outlier_signal() {
        let mut input = calm();
        input.merchant_median_minor = None;
        input.amount_minor = 10_000_00; // below high-value, no median
        assert!(assess(&input).signals.is_empty());
    }
}
