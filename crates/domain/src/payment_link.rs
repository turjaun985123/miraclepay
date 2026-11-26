//! Shareable payment links.
//!
//! A [`PaymentLink`] is a merchant-owned, slug-addressed page that a
//! customer can open to pay. A link either carries a fixed amount — in
//! which case the currency is fixed too, because it travels inside the
//! [`Money`](crate::money::Money) value — or it carries no amount, in
//! which case the customer chooses how much to pay at checkout time.
//!
//! Links can be deactivated, can expire, and can optionally be capped at
//! a maximum number of successful uses. This module is original,
//! clean-room domain code: it models those rules and nothing about how
//! any particular link is stored, routed, or rendered.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::DomainError;
use crate::money::Money;

/// A single shareable payment link belonging to one merchant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentLink {
    /// Unique identifier of this link.
    pub id: String,
    /// Identifier of the merchant that owns this link.
    pub merchant_id: String,
    /// URL-safe identifier used to address the link publicly.
    ///
    /// Normally derived from the title with [`slugify`].
    pub slug: String,
    /// Human-readable title shown to the customer.
    pub title: String,
    /// Optional longer description shown to the customer.
    pub description: Option<String>,
    /// Fixed amount the customer must pay, including its currency.
    ///
    /// `None` means the link is open-amount: the customer chooses the
    /// amount (and therefore the currency context) at checkout time.
    pub amount: Option<Money>,
    /// Whether the checkout form must collect the customer's phone number.
    pub collect_phone: bool,
    /// Whether the checkout form must collect the customer's email address.
    pub collect_email: bool,
    /// Maximum number of times this link may be used.
    ///
    /// `None` means the link can be used without limit.
    pub max_uses: Option<u32>,
    /// Number of times this link has already been used.
    pub uses: u32,
    /// Whether the merchant has this link switched on.
    pub active: bool,
    /// Optional point in time after which the link stops working.
    pub expires_at: Option<DateTime<Utc>>,
    /// When this link was created.
    pub created_at: DateTime<Utc>,
}

impl PaymentLink {
    /// Whether this link has passed its expiry time at `now`.
    ///
    /// A link with no expiry never expires. A link whose expiry is
    /// exactly `now` counts as expired: it is usable strictly before
    /// its `expires_at` instant.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(expires_at) => now >= expires_at,
            None => false,
        }
    }

    /// How many uses remain before the cap is reached.
    ///
    /// Returns `None` for an unlimited link, and `Some(0)` once a capped
    /// link has been used up.
    pub fn remaining_uses(&self) -> Option<u32> {
        self.max_uses
            .map(|max_uses| max_uses.saturating_sub(self.uses))
    }

    /// Whether this link has reached its use cap.
    fn is_exhausted(&self) -> bool {
        self.remaining_uses() == Some(0)
    }

    /// Whether a customer can pay through this link at `now`.
    ///
    /// A link is usable only when it is active, not expired at `now`,
    /// and not exhausted.
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.active && !self.is_expired(now) && !self.is_exhausted()
    }

    /// Record one successful use of this link.
    ///
    /// # Errors
    ///
    /// * [`DomainError::LinkInactive`] if the link is switched off; the
    ///   use count is left unchanged.
    /// * [`DomainError::LinkExhausted`] if the link has already reached
    ///   its `max_uses` cap; the use count is left unchanged.
    pub fn record_use(&mut self) -> Result<(), DomainError> {
        if !self.active {
            return Err(DomainError::LinkInactive);
        }
        if self.is_exhausted() {
            return Err(DomainError::LinkExhausted);
        }
        self.uses = self.uses.saturating_add(1);
        Ok(())
    }
}

/// Turn a free-form title into a URL-safe slug.
///
/// The title is lowercased, every run of characters that are not
/// alphanumeric collapses into a single hyphen, and no hyphen is left
/// at either end. A title with no alphanumeric characters at all
/// produces an empty slug.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(slugify("Pay for Coaching!"), "pay-for-coaching");
/// assert_eq!(slugify("  Order #42 -- Deluxe  "), "order-42-deluxe");
/// assert_eq!(slugify("!!!"), "");
/// ```
pub fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut pending_separator = false;

    for ch in title.chars() {
        for lowered in ch.to_lowercase() {
            if lowered.is_alphanumeric() {
                if pending_separator && !slug.is_empty() {
                    slug.push('-');
                }
                pending_separator = false;
                slug.push(lowered);
            } else if !slug.is_empty() {
                pending_separator = true;
            }
        }
    }

    slug
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
    }

    /// An open-amount, unlimited, active link created on 2026-01-01.
    fn sample_link() -> PaymentLink {
        PaymentLink {
            id: "link_1".to_string(),
            merchant_id: "merchant_1".to_string(),
            slug: slugify("Coaching Session"),
            title: "Coaching Session".to_string(),
            description: None,
            amount: None,
            collect_phone: true,
            collect_email: false,
            max_uses: None,
            uses: 0,
            active: true,
            expires_at: None,
            created_at: at(2026, 1, 1, 0),
        }
    }

    // ---- slugify -------------------------------------------------------

    #[test]
    fn slugify_lowercases_and_hyphenates_words() {
        assert_eq!(slugify("Coaching Session"), "coaching-session");
        assert_eq!(slugify("PAY NOW"), "pay-now");
    }

    #[test]
    fn slugify_keeps_digits_and_drops_punctuation() {
        assert_eq!(slugify("Order #42: Deluxe!"), "order-42-deluxe");
        assert_eq!(slugify("bkash payment"), "bkash-payment");
    }

    #[test]
    fn slugify_collapses_separator_runs_and_trims_ends() {
        assert_eq!(slugify("  --Hello   World--  "), "hello-world");
        assert_eq!(slugify("a_b.c/d"), "a-b-c-d");
        assert_eq!(slugify("one---two"), "one-two");
    }

    #[test]
    fn slugify_handles_empty_and_symbol_only_titles() {
        assert_eq!(slugify(""), "");
        assert_eq!(slugify("   "), "");
        assert_eq!(slugify("!!! ???"), "");
    }

    #[test]
    fn slugify_leaves_an_existing_slug_unchanged() {
        assert_eq!(slugify("already-a-slug-7"), "already-a-slug-7");
    }

    // ---- is_usable -----------------------------------------------------

    #[test]
    fn active_unlimited_unexpired_link_is_usable() {
        let link = sample_link();
        assert!(link.is_usable(at(2026, 6, 1, 12)));
    }

    #[test]
    fn inactive_link_is_not_usable() {
        let mut link = sample_link();
        link.active = false;
        assert!(!link.is_usable(at(2026, 6, 1, 12)));
    }

    #[test]
    fn link_is_usable_before_expiry_and_not_at_or_after_it() {
        let mut link = sample_link();
        link.expires_at = Some(at(2026, 6, 1, 12));
        assert!(link.is_usable(at(2026, 6, 1, 11)));
        assert!(!link.is_usable(at(2026, 6, 1, 12)));
        assert!(!link.is_usable(at(2026, 6, 1, 13)));
    }

    #[test]
    fn link_without_expiry_never_expires() {
        let link = sample_link();
        assert!(!link.is_expired(at(2999, 1, 1, 0)));
        assert!(link.is_usable(at(2999, 1, 1, 0)));
    }

    #[test]
    fn exhausted_link_is_not_usable() {
        let mut link = sample_link();
        link.max_uses = Some(3);
        link.uses = 3;
        assert!(!link.is_usable(at(2026, 6, 1, 12)));
        assert_eq!(link.remaining_uses(), Some(0));
    }

    #[test]
    fn link_below_its_cap_is_usable_and_reports_remaining_uses() {
        let mut link = sample_link();
        link.max_uses = Some(3);
        link.uses = 1;
        assert!(link.is_usable(at(2026, 6, 1, 12)));
        assert_eq!(link.remaining_uses(), Some(2));
    }

    #[test]
    fn unlimited_link_reports_no_remaining_uses() {
        assert_eq!(sample_link().remaining_uses(), None);
    }

    // ---- record_use ----------------------------------------------------

    #[test]
    fn record_use_increments_the_use_count() {
        let mut link = sample_link();
        link.record_use().unwrap();
        link.record_use().unwrap();
        assert_eq!(link.uses, 2);
    }

    #[test]
    fn record_use_on_inactive_link_fails_without_counting() {
        let mut link = sample_link();
        link.active = false;
        let err = link.record_use().unwrap_err();
        assert!(matches!(err, DomainError::LinkInactive));
        assert_eq!(link.uses, 0);
    }

    #[test]
    fn record_use_on_exhausted_link_fails_without_counting() {
        let mut link = sample_link();
        link.max_uses = Some(1);
        link.uses = 1;
        let err = link.record_use().unwrap_err();
        assert!(matches!(err, DomainError::LinkExhausted));
        assert_eq!(link.uses, 1);
    }

    #[test]
    fn record_use_allows_the_last_use_then_reports_exhaustion() {
        let mut link = sample_link();
        link.max_uses = Some(2);
        link.record_use().unwrap();
        link.record_use().unwrap();
        assert_eq!(link.uses, 2);
        assert!(!link.is_usable(at(2026, 6, 1, 12)));
        assert!(matches!(
            link.record_use().unwrap_err(),
            DomainError::LinkExhausted
        ));
    }

    #[test]
    fn zero_cap_link_is_exhausted_from_the_start() {
        let mut link = sample_link();
        link.max_uses = Some(0);
        assert!(!link.is_usable(at(2026, 6, 1, 12)));
        assert!(matches!(
            link.record_use().unwrap_err(),
            DomainError::LinkExhausted
        ));
    }
}
