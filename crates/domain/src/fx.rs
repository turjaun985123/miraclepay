//! Multi-currency groundwork: FX conversion (Wave 34).
//!
//! MiraclePay's reporting currency is BDT; merchants may *take*
//! payments in BDT/INR/PKR/USD/EUR. This module is the single
//! conversion point used whenever a foreign-currency amount must
//! be expressed in another currency (reporting, settlement
//! estimates, API conversion endpoint).
//!
//! Rates are **rational** (`num/den` = units of the currency per
//! 1 BDT) so conversion stays in integer arithmetic — no floats
//! anywhere near money. All supported currencies use 2 decimal
//! places, so minor units convert directly:
//!
//! `converted_minor = amount_minor * rate(to) / rate(from)`
//! computed as `amount * num(to) * den(from) / (den(to) * num(from))`
//! with round-half-up on the final division.
//!
//! The default table is an indicative snapshot (Nov 2026) for
//! display/reporting — operators override it in platform settings;
//! it is NOT a dealing rate and no payment is priced from it.

use serde::{Deserialize, Serialize};

use crate::money::{Currency, Money};

/// One currency's rate against BDT: `num/den` units per 1 BDT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FxRate {
    pub currency: Currency,
    pub num: u64,
    pub den: u64,
}

/// The built-in indicative rate table (per 1 BDT):
/// 1 BDT ≈ 0.00837 USD, 0.701 INR, 2.33 PKR, 0.00771 EUR.
pub fn default_rates() -> Vec<FxRate> {
    vec![
        FxRate {
            currency: Currency::BDT,
            num: 1,
            den: 1,
        },
        FxRate {
            currency: Currency::USD,
            num: 837,
            den: 100_000,
        },
        FxRate {
            currency: Currency::INR,
            num: 701,
            den: 1_000,
        },
        FxRate {
            currency: Currency::PKR,
            num: 233,
            den: 100,
        },
        FxRate {
            currency: Currency::EUR,
            num: 771,
            den: 100_000,
        },
    ]
}

/// Look up a currency's rate in `rates`.
pub fn rate_of(rates: &[FxRate], currency: Currency) -> Option<FxRate> {
    rates.iter().find(|rate| rate.currency == currency).copied()
}

/// Why a conversion cannot be performed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FxError {
    #[error("no FX rate configured for {0}")]
    MissingRate(&'static str),
    #[error("FX rate for {0} is zero or invalid")]
    InvalidRate(&'static str),
}

/// Convert `money` into `target` using `rates`, rounding half-up.
/// Identity conversions return the input unchanged (and need no
/// rate entry).
pub fn convert(money: Money, target: Currency, rates: &[FxRate]) -> Result<Money, FxError> {
    if money.currency == target {
        return Ok(money);
    }
    let from = rate_of(rates, money.currency).ok_or(FxError::MissingRate(money.currency.code()))?;
    let to = rate_of(rates, target).ok_or(FxError::MissingRate(target.code()))?;
    if from.num == 0 || from.den == 0 || to.num == 0 || to.den == 0 {
        return Err(FxError::InvalidRate(target.code()));
    }
    // converted = amount * to.num * from.den / (to.den * from.num)
    let numerator = i128::from(money.amount_minor) * i128::from(to.num) * i128::from(from.den);
    let denominator = i128::from(to.den) * i128::from(from.num);
    let converted = div_round_half_up(numerator, denominator);
    Ok(Money::new(converted as i64, target))
}

/// Integer division rounding half away from zero on ties
/// (amounts here are non-negative in practice; refunds keep the
/// sign symmetric).
fn div_round_half_up(numerator: i128, denominator: i128) -> i128 {
    if denominator == 0 {
        return 0;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder.abs() * 2 >= denominator.abs() {
        quotient + remainder.signum() * denominator.signum()
    } else {
        quotient
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_conversion_needs_no_rate() {
        let money = Money::new(12_500, Currency::BDT);
        assert_eq!(convert(money, Currency::BDT, &[]).unwrap(), money);
    }

    #[test]
    fn usd_to_bdt_and_back() {
        let rates = default_rates();
        // $100.00 = 10000 minor. 1 USD = 1/0.00837 BDT ≈ 119.47 BDT,
        // so $100 ≈ ৳11,947.43 = 1,194,743 minor.
        let bdt = convert(Money::new(10_000, Currency::USD), Currency::BDT, &rates).unwrap();
        assert_eq!(bdt.currency, Currency::BDT);
        assert_eq!(bdt.amount_minor, 1_194_743);
        // Round trip lands within a poisha of the original.
        let back = convert(bdt, Currency::USD, &rates).unwrap();
        assert!((back.amount_minor - 10_000).abs() <= 1);
    }

    #[test]
    fn bdt_to_inr_uses_the_table() {
        let rates = default_rates();
        // ৳1,000.00 → ₹701.00 exactly per the default table.
        let inr = convert(Money::new(100_000, Currency::BDT), Currency::INR, &rates).unwrap();
        assert_eq!(inr.amount_minor, 70_100);
    }

    #[test]
    fn missing_rate_is_an_error_not_a_guess() {
        let rates = vec![FxRate {
            currency: Currency::BDT,
            num: 1,
            den: 1,
        }];
        let err = convert(Money::new(100, Currency::EUR), Currency::BDT, &rates).unwrap_err();
        assert_eq!(err, FxError::MissingRate("EUR"));
    }

    #[test]
    fn rounding_is_half_up() {
        // Custom table: 1 BDT = 0.5 XYZ-like USD units → 1 minor → 0.5 → 1.
        let rates = vec![
            FxRate {
                currency: Currency::BDT,
                num: 1,
                den: 1,
            },
            FxRate {
                currency: Currency::USD,
                num: 1,
                den: 2,
            },
        ];
        let out = convert(Money::new(1, Currency::BDT), Currency::USD, &rates).unwrap();
        assert_eq!(out.amount_minor, 1);
        let out = convert(Money::new(3, Currency::BDT), Currency::USD, &rates).unwrap();
        assert_eq!(out.amount_minor, 2); // 1.5 → 2
    }
}
