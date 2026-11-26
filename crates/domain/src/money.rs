//! Money and currency types for MiraclePay.
//!
//! Original clean-room implementation. All amounts are stored as an
//! integer count of the currency's minor unit (for example poisha,
//! paise, or cents), never as a floating-point value, so arithmetic on
//! money is exact. Every currency currently supported by the platform
//! uses 100 minor units per major unit.
//!
//! Operations that combine two amounts require both amounts to be in
//! the same currency; mixing currencies is an error, never an implicit
//! conversion. Callers that need a conversion must do it explicitly,
//! at a rate they supply, outside this module.

use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// A currency supported by the platform, identified by its ISO 4217
/// alphabetic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    /// Bangladeshi taka.
    BDT,
    /// Indian rupee.
    INR,
    /// Pakistani rupee.
    PKR,
    /// United States dollar.
    USD,
    /// Euro.
    EUR,
}

impl Currency {
    /// The ISO 4217 alphabetic code of this currency, e.g. `"BDT"`.
    pub fn code(&self) -> &'static str {
        match self {
            Currency::BDT => "BDT",
            Currency::INR => "INR",
            Currency::PKR => "PKR",
            Currency::USD => "USD",
            Currency::EUR => "EUR",
        }
    }

    /// How many minor units make one major unit of this currency.
    ///
    /// Every currency the platform currently supports subdivides into
    /// 100 minor units (poisha, paise, cents, ...), so this is `100`
    /// for all variants. Code that formats or converts amounts reads
    /// the factor from here rather than hard-coding `100`, so a future
    /// currency with a different subdivision only changes this method.
    pub fn minor_factor(&self) -> i64 {
        match self {
            Currency::BDT => 100,
            Currency::INR => 100,
            Currency::PKR => 100,
            Currency::USD => 100,
            Currency::EUR => 100,
        }
    }

    /// Number of decimal digits shown after the decimal point when an
    /// amount in this currency is formatted, derived from
    /// [`Currency::minor_factor`] (100 minor units -> 2 digits).
    fn decimal_digits(&self) -> u32 {
        let factor = self.minor_factor();
        debug_assert!(factor > 0 && [1, 10, 100, 1_000].contains(&factor));
        (factor as u64).ilog10()
    }

    /// Look up a currency by its ISO 4217 alphabetic code.
    ///
    /// Matching is case-insensitive and surrounding whitespace is
    /// ignored. Returns `None` for a code the platform does not
    /// support.
    pub fn from_code(code: &str) -> Option<Currency> {
        match code.trim().to_ascii_uppercase().as_str() {
            "BDT" => Some(Currency::BDT),
            "INR" => Some(Currency::INR),
            "PKR" => Some(Currency::PKR),
            "USD" => Some(Currency::USD),
            "EUR" => Some(Currency::EUR),
            _ => None,
        }
    }

    /// All currencies supported by the platform.
    pub fn all() -> &'static [Currency] {
        &[
            Currency::BDT,
            Currency::INR,
            Currency::PKR,
            Currency::USD,
            Currency::EUR,
        ]
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// An amount of money: an integer count of minor units in a specific
/// currency.
///
/// Amounts may be negative (for example a refund or a balance
/// adjustment); constructors do not reject negative values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    /// Amount in the currency's minor unit (e.g. `123456` = 1,234.56).
    pub amount_minor: i64,
    /// The currency the amount is denominated in.
    pub currency: Currency,
}

impl Money {
    /// Create an amount from a count of minor units.
    pub fn new(amount_minor: i64, currency: Currency) -> Money {
        Money {
            amount_minor,
            currency,
        }
    }

    /// A zero amount in the given currency.
    pub fn zero(currency: Currency) -> Money {
        Money {
            amount_minor: 0,
            currency,
        }
    }

    /// Create an amount from a whole number of major units (e.g. `5`
    /// taka), scaling by the currency's minor factor.
    ///
    /// Returns `None` when the scaled value does not fit in an `i64`
    /// count of minor units.
    pub fn from_major(major: i64, currency: Currency) -> Option<Money> {
        major
            .checked_mul(currency.minor_factor())
            .map(|amount_minor| Money {
                amount_minor,
                currency,
            })
    }

    /// Whether this amount is exactly zero.
    pub fn is_zero(&self) -> bool {
        self.amount_minor == 0
    }

    /// Whether this amount is strictly greater than zero.
    pub fn is_positive(&self) -> bool {
        self.amount_minor > 0
    }

    /// Whether this amount is strictly less than zero.
    pub fn is_negative(&self) -> bool {
        self.amount_minor < 0
    }

    /// Fail with [`DomainError::CurrencyMismatch`] unless `other` is
    /// denominated in the same currency as `self`.
    fn ensure_same_currency(&self, other: &Money) -> Result<(), DomainError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(DomainError::CurrencyMismatch)
        }
    }

    /// Add two amounts of the same currency.
    ///
    /// # Errors
    /// - [`DomainError::CurrencyMismatch`] if the currencies differ.
    /// - [`DomainError::Overflow`] if the result does not fit in an
    ///   `i64` count of minor units.
    // Not the Add trait on purpose: money addition is checked
    // (currency match + overflow) and returns a Result.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: Money) -> Result<Money, DomainError> {
        self.ensure_same_currency(&other)?;
        self.amount_minor
            .checked_add(other.amount_minor)
            .map(|amount_minor| Money {
                amount_minor,
                currency: self.currency,
            })
            .ok_or(DomainError::Overflow)
    }

    /// Subtract `other` from `self`; both must share a currency. The
    /// result may be negative.
    ///
    /// # Errors
    /// - [`DomainError::CurrencyMismatch`] if the currencies differ.
    /// - [`DomainError::Overflow`] if the result does not fit in an
    ///   `i64` count of minor units.
    // See `add`: checked arithmetic, deliberately not the trait.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, other: Money) -> Result<Money, DomainError> {
        self.ensure_same_currency(&other)?;
        self.amount_minor
            .checked_sub(other.amount_minor)
            .map(|amount_minor| Money {
                amount_minor,
                currency: self.currency,
            })
            .ok_or(DomainError::Overflow)
    }

    /// Checked addition returning `None` instead of an error when the
    /// currencies differ or the result overflows.
    pub fn checked_add(self, other: Money) -> Option<Money> {
        if self.currency != other.currency {
            return None;
        }
        self.amount_minor
            .checked_add(other.amount_minor)
            .map(|amount_minor| Money {
                amount_minor,
                currency: self.currency,
            })
    }

    /// Checked subtraction returning `None` instead of an error when
    /// the currencies differ or the result overflows.
    pub fn checked_sub(self, other: Money) -> Option<Money> {
        if self.currency != other.currency {
            return None;
        }
        self.amount_minor
            .checked_sub(other.amount_minor)
            .map(|amount_minor| Money {
                amount_minor,
                currency: self.currency,
            })
    }

    /// Checked multiplication by an integer quantity (for example a
    /// number of identical line items). Returns `None` on overflow.
    pub fn checked_mul(self, quantity: i64) -> Option<Money> {
        self.amount_minor
            .checked_mul(quantity)
            .map(|amount_minor| Money {
                amount_minor,
                currency: self.currency,
            })
    }

    /// The negation of this amount, or `None` if it cannot be
    /// represented (only possible for `i64::MIN` minor units).
    pub fn checked_neg(self) -> Option<Money> {
        self.amount_minor.checked_neg().map(|amount_minor| Money {
            amount_minor,
            currency: self.currency,
        })
    }

    /// The absolute value of this amount, or `None` if it cannot be
    /// represented (only possible for `i64::MIN` minor units).
    pub fn checked_abs(self) -> Option<Money> {
        self.amount_minor.checked_abs().map(|amount_minor| Money {
            amount_minor,
            currency: self.currency,
        })
    }

    /// The fee charged on this amount at `bps` basis points
    /// (1 bp = 0.01%, so 10_000 bps = 100%).
    ///
    /// The fee keeps the sign of the amount (a fee on a negative
    /// amount, such as a refund, is negative). The exact product is
    /// computed in `i128` and rounded to the nearest minor unit, with
    /// ties rounding away from zero. Rates above 10_000 bps are
    /// allowed (a surcharge can exceed the principal); if the result
    /// would not fit in an `i64` count of minor units it saturates at
    /// the nearest `i64` bound instead of overflowing.
    pub fn fee_bps(&self, bps: u32) -> Money {
        // i64 * u32 fits comfortably in i128, so this cannot overflow.
        let product = self.amount_minor as i128 * bps as i128;
        let divisor: i128 = 10_000;
        // Round to nearest, ties away from zero: add half the divisor
        // (with the sign of the product) before truncating division.
        let adjusted = if product >= 0 {
            product + divisor / 2
        } else {
            product - divisor / 2
        };
        let fee = adjusted / divisor;
        let amount_minor = i64::try_from(fee).unwrap_or(if fee < 0 { i64::MIN } else { i64::MAX });
        Money {
            amount_minor,
            currency: self.currency,
        }
    }

    /// Human-readable form: the currency code, a space, and the amount
    /// with thousands separators and exactly the currency's number of
    /// decimal digits, e.g. `"BDT 1,234.56"`. A negative amount puts
    /// the minus sign before the code: `"-BDT 1,234.56"`.
    pub fn format_display(&self) -> String {
        let factor = self.currency.minor_factor() as u64;
        let digits = self.currency.decimal_digits() as usize;
        // unsigned_abs keeps i64::MIN correct, where abs() would overflow.
        let magnitude = self.amount_minor.unsigned_abs();
        let major = magnitude / factor;
        let minor = magnitude % factor;
        let sign = if self.amount_minor < 0 { "-" } else { "" };
        format!(
            "{}{} {}.{:0width$}",
            sign,
            self.currency.code(),
            group_thousands(major),
            minor,
            width = digits,
        )
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format_display())
    }
}

/// Format a non-negative integer with `,` thousands separators,
/// e.g. `1234567` -> `"1,234,567"`.
fn group_thousands(value: u64) -> String {
    let raw = value.to_string();
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len() + raw.len() / 3);
    for (i, byte) in bytes.iter().enumerate() {
        // A separator goes before a digit when the number of digits
        // remaining after it is a multiple of three.
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*byte as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_codes_are_iso_4217() {
        assert_eq!(Currency::BDT.code(), "BDT");
        assert_eq!(Currency::INR.code(), "INR");
        assert_eq!(Currency::PKR.code(), "PKR");
        assert_eq!(Currency::USD.code(), "USD");
        assert_eq!(Currency::EUR.code(), "EUR");
    }

    #[test]
    fn every_currency_has_minor_factor_100() {
        for currency in Currency::all() {
            assert_eq!(currency.minor_factor(), 100, "{}", currency.code());
        }
    }

    #[test]
    fn currency_from_code_round_trips() {
        for currency in Currency::all() {
            assert_eq!(Currency::from_code(currency.code()), Some(*currency));
        }
        assert_eq!(Currency::from_code("bdt"), Some(Currency::BDT));
        assert_eq!(Currency::from_code(" Usd "), Some(Currency::USD));
        assert_eq!(Currency::from_code("GBP"), None);
        assert_eq!(Currency::from_code(""), None);
    }

    #[test]
    fn new_and_zero() {
        let money = Money::new(1_500, Currency::BDT);
        assert_eq!(money.amount_minor, 1_500);
        assert_eq!(money.currency, Currency::BDT);
        assert!(!money.is_zero());
        assert!(money.is_positive());
        assert!(!money.is_negative());

        let zero = Money::zero(Currency::INR);
        assert_eq!(zero.amount_minor, 0);
        assert_eq!(zero.currency, Currency::INR);
        assert!(zero.is_zero());

        assert!(Money::new(-1, Currency::USD).is_negative());
    }

    #[test]
    fn from_major_scales_by_minor_factor() {
        assert_eq!(
            Money::from_major(5, Currency::BDT),
            Some(Money::new(500, Currency::BDT))
        );
        assert_eq!(
            Money::from_major(-3, Currency::EUR),
            Some(Money::new(-300, Currency::EUR))
        );
        assert_eq!(Money::from_major(i64::MAX, Currency::BDT), None);
    }

    #[test]
    fn add_and_sub_same_currency() {
        let a = Money::new(1_000, Currency::BDT);
        let b = Money::new(250, Currency::BDT);
        assert_eq!(a.add(b), Ok(Money::new(1_250, Currency::BDT)));
        assert_eq!(a.sub(b), Ok(Money::new(750, Currency::BDT)));
        // Subtraction may legitimately go negative.
        assert_eq!(b.sub(a), Ok(Money::new(-750, Currency::BDT)));
    }

    #[test]
    fn add_and_sub_reject_currency_mismatch() {
        let taka = Money::new(100, Currency::BDT);
        let rupees = Money::new(100, Currency::INR);
        assert_eq!(taka.add(rupees), Err(DomainError::CurrencyMismatch));
        assert_eq!(taka.sub(rupees), Err(DomainError::CurrencyMismatch));
        assert_eq!(taka.checked_add(rupees), None);
        assert_eq!(taka.checked_sub(rupees), None);
    }

    #[test]
    fn add_and_sub_report_overflow() {
        let big = Money::new(i64::MAX, Currency::USD);
        let one = Money::new(1, Currency::USD);
        assert_eq!(big.add(one), Err(DomainError::Overflow));
        assert_eq!(big.checked_add(one), None);

        let small = Money::new(i64::MIN, Currency::USD);
        assert_eq!(small.sub(one), Err(DomainError::Overflow));
        assert_eq!(small.checked_sub(one), None);
    }

    #[test]
    fn checked_mul_neg_and_abs() {
        let money = Money::new(-250, Currency::PKR);
        assert_eq!(
            money.checked_mul(4),
            Some(Money::new(-1_000, Currency::PKR))
        );
        assert_eq!(money.checked_mul(0), Some(Money::zero(Currency::PKR)));
        assert_eq!(Money::new(i64::MAX, Currency::PKR).checked_mul(2), None);
        assert_eq!(money.checked_neg(), Some(Money::new(250, Currency::PKR)));
        assert_eq!(money.checked_abs(), Some(Money::new(250, Currency::PKR)));
        assert_eq!(Money::new(i64::MIN, Currency::PKR).checked_neg(), None);
        assert_eq!(Money::new(i64::MIN, Currency::PKR).checked_abs(), None);
    }

    #[test]
    fn fee_bps_exact_and_zero_rates() {
        let amount = Money::new(100_00, Currency::BDT);
        // 250 bps = 2.5% of 100.00 -> 2.50.
        assert_eq!(amount.fee_bps(250), Money::new(250, Currency::BDT));
        assert_eq!(amount.fee_bps(0), Money::zero(Currency::BDT));
        assert_eq!(amount.fee_bps(10_000), amount);
        assert_eq!(
            Money::zero(Currency::BDT).fee_bps(500),
            Money::zero(Currency::BDT)
        );
    }

    #[test]
    fn fee_bps_rounds_to_nearest_minor_unit() {
        // 1% of 1.99 = 0.0199 -> 0.02 (2 minor units).
        assert_eq!(
            Money::new(199, Currency::USD).fee_bps(100),
            Money::new(2, Currency::USD)
        );
        // 0.5% of 1.01 = 0.00505 -> 0.01 (1 minor unit).
        assert_eq!(
            Money::new(101, Currency::USD).fee_bps(50),
            Money::new(1, Currency::USD)
        );
        // 0.5% of 0.49 = 0.00245 -> 0.00.
        assert_eq!(
            Money::new(49, Currency::USD).fee_bps(50),
            Money::new(0, Currency::USD)
        );
        // An exact half minor unit rounds away from zero.
        assert_eq!(
            Money::new(50, Currency::USD).fee_bps(100),
            Money::new(1, Currency::USD)
        );
    }

    #[test]
    fn fee_bps_keeps_currency_and_sign() {
        let refund = Money::new(-100_00, Currency::EUR);
        assert_eq!(refund.fee_bps(250), Money::new(-250, Currency::EUR));
        assert_eq!(
            Money::new(100, Currency::INR).fee_bps(100).currency,
            Currency::INR
        );
    }

    #[test]
    fn fee_bps_above_one_hundred_percent_and_saturation() {
        // 200% of 10.00 -> 20.00.
        assert_eq!(
            Money::new(10_00, Currency::BDT).fee_bps(20_000),
            Money::new(20_00, Currency::BDT)
        );
        // An absurd rate on a huge amount saturates instead of overflowing.
        assert_eq!(
            Money::new(i64::MAX, Currency::BDT)
                .fee_bps(u32::MAX)
                .amount_minor,
            i64::MAX
        );
        assert_eq!(
            Money::new(i64::MIN, Currency::BDT)
                .fee_bps(u32::MAX)
                .amount_minor,
            i64::MIN
        );
    }

    #[test]
    fn format_display_examples() {
        assert_eq!(
            Money::new(123_456, Currency::BDT).format_display(),
            "BDT 1,234.56"
        );
        assert_eq!(Money::zero(Currency::USD).format_display(), "USD 0.00");
        assert_eq!(Money::new(5, Currency::EUR).format_display(), "EUR 0.05");
        assert_eq!(Money::new(100, Currency::INR).format_display(), "INR 1.00");
        assert_eq!(
            Money::new(1_000_000_00, Currency::PKR).format_display(),
            "PKR 1,000,000.00"
        );
        assert_eq!(
            Money::new(12_345_678_999, Currency::BDT).format_display(),
            "BDT 123,456,789.99"
        );
    }

    #[test]
    fn format_display_negative_amounts() {
        assert_eq!(
            Money::new(-123_456, Currency::BDT).format_display(),
            "-BDT 1,234.56"
        );
        assert_eq!(Money::new(-5, Currency::USD).format_display(), "-USD 0.05");
        // i64::MIN must format without overflowing on negation.
        assert_eq!(
            Money::new(i64::MIN, Currency::USD).format_display(),
            "-USD 92,233,720,368,547,758.08"
        );
    }

    #[test]
    fn display_matches_format_display() {
        let money = Money::new(123_456, Currency::BDT);
        assert_eq!(format!("{money}"), money.format_display());
        assert_eq!(format!("{}", Currency::BDT), "BDT");
    }

    #[test]
    fn money_is_copy_and_compares_by_value() {
        let a = Money::new(100, Currency::BDT);
        let b = a; // Copy: `a` is still usable.
        assert_eq!(a, b);
        assert_ne!(a, Money::new(100, Currency::USD));
        assert_ne!(a, Money::new(101, Currency::BDT));
    }
}
