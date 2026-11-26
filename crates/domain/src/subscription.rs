//! Recurring billing: subscription plans and subscriptions.
//!
//! Original, clean-room code for MiraclePay.
//!
//! A [`Plan`] is a merchant-owned price point: an amount, charged on a
//! fixed [`BillingInterval`], optionally after a free trial. A
//! [`Subscription`] binds one customer to one plan and tracks where the
//! customer is in the current billing period.
//!
//! Intervals use fixed, approximate day counts (see
//! [`BillingInterval::days`]) rather than calendar arithmetic. That keeps
//! scheduling a pure, deterministic function of a timestamp — easy to
//! test, and identical no matter which month a period happens to start
//! in — at the cost of drifting a few days against the calendar over a
//! year. Callers that need calendar-exact renewal dates should layer
//! that policy on top of this type.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::money::Money;

/// How often a [`Plan`] bills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingInterval {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl BillingInterval {
    /// Approximate length of one billing period, in days.
    ///
    /// `Daily` = 1, `Weekly` = 7, `Monthly` = 30, `Quarterly` = 90,
    /// `Yearly` = 365.
    pub fn days(self) -> i64 {
        match self {
            BillingInterval::Daily => 1,
            BillingInterval::Weekly => 7,
            BillingInterval::Monthly => 30,
            BillingInterval::Quarterly => 90,
            BillingInterval::Yearly => 365,
        }
    }

    /// Length of one billing period as a [`Duration`].
    pub fn duration(self) -> Duration {
        Duration::days(self.days())
    }

    /// The billing date one period after `from`.
    pub fn next_billing_date(self, from: DateTime<Utc>) -> DateTime<Utc> {
        from + self.duration()
    }
}

/// A merchant-owned recurring price point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    /// Amount charged once per `interval`.
    pub amount: Money,
    pub interval: BillingInterval,
    /// Free trial length in days before the first charge; `0` = no trial.
    pub trial_days: u32,
    /// Inactive plans cannot be subscribed to; existing subscriptions
    /// are unaffected.
    pub active: bool,
}

impl Plan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        merchant_id: impl Into<String>,
        name: impl Into<String>,
        amount: Money,
        interval: BillingInterval,
        trial_days: u32,
        active: bool,
    ) -> Self {
        Self {
            id: id.into(),
            merchant_id: merchant_id.into(),
            name: name.into(),
            amount,
            interval,
            trial_days,
            active,
        }
    }

    /// Whether this plan offers a free trial.
    pub fn has_trial(&self) -> bool {
        self.trial_days > 0
    }

    /// When a trial starting at `start` ends (and the first charge is
    /// due). With `trial_days == 0` this is simply `start`.
    pub fn trial_end(&self, start: DateTime<Utc>) -> DateTime<Utc> {
        start + Duration::days(i64::from(self.trial_days))
    }

    /// End of the first paid period for a subscription starting at
    /// `start`, i.e. after any trial has elapsed.
    pub fn first_period_end(&self, start: DateTime<Utc>) -> DateTime<Utc> {
        self.next_billing_date(self.trial_end(start))
    }

    /// The billing date one period after `from`, per this plan's
    /// interval: `from + interval.days()`.
    pub fn next_billing_date(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        self.interval.next_billing_date(from)
    }
}

/// Lifecycle state of a [`Subscription`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    /// In the plan's free-trial window; not yet charged.
    Trialing,
    /// In good standing; charged every period.
    Active,
    /// The most recent charge failed; service may be in a grace window
    /// and the charge should be retried.
    PastDue,
    /// Cancelled by the customer or the merchant. Terminal.
    Cancelled,
    /// Ran out (trial or final period elapsed without renewal). Terminal.
    Expired,
}

impl SubscriptionStatus {
    /// Terminal states never bill again and cannot be reactivated
    /// in place; a returning customer needs a new subscription.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            SubscriptionStatus::Cancelled | SubscriptionStatus::Expired
        )
    }

    /// States in which a due subscription may legitimately be charged:
    /// a trialing subscription bills when its trial ends, an active one
    /// bills every period, and a past-due one bills on retry.
    pub fn allows_billing(self) -> bool {
        matches!(
            self,
            SubscriptionStatus::Trialing | SubscriptionStatus::Active | SubscriptionStatus::PastDue
        )
    }
}

/// One customer's enrolment in one [`Plan`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub plan_id: String,
    pub merchant_id: String,
    pub customer_email: String,
    pub status: SubscriptionStatus,
    /// End of the current (trial or paid) period. At or after this
    /// instant the subscription is due for its next charge — see
    /// [`Subscription::is_billable`].
    pub current_period_end: DateTime<Utc>,
    /// If `true`, the subscription ends at `current_period_end`
    /// instead of renewing: it must not be charged again.
    pub cancel_at_period_end: bool,
    pub created_at: DateTime<Utc>,
}

impl Subscription {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        plan_id: impl Into<String>,
        merchant_id: impl Into<String>,
        customer_email: impl Into<String>,
        status: SubscriptionStatus,
        current_period_end: DateTime<Utc>,
        cancel_at_period_end: bool,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.into(),
            plan_id: plan_id.into(),
            merchant_id: merchant_id.into(),
            customer_email: customer_email.into(),
            status,
            current_period_end,
            cancel_at_period_end,
            created_at,
        }
    }

    /// Start a subscription to `plan` at `now`.
    ///
    /// With a trial (`plan.trial_days > 0`) the status is
    /// [`SubscriptionStatus::Trialing`] and the first period ends when
    /// the trial does; otherwise the status is
    /// [`SubscriptionStatus::Active`] and the first period ends one
    /// interval from `now`.
    pub fn start(
        id: impl Into<String>,
        plan: &Plan,
        customer_email: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        let (status, current_period_end) = if plan.has_trial() {
            (SubscriptionStatus::Trialing, plan.trial_end(now))
        } else {
            (SubscriptionStatus::Active, plan.next_billing_date(now))
        };
        Self {
            id: id.into(),
            plan_id: plan.id.clone(),
            merchant_id: plan.merchant_id.clone(),
            customer_email: customer_email.into(),
            status,
            current_period_end,
            cancel_at_period_end: false,
            created_at: now,
        }
    }

    /// The billing date one period after `from`, using `plan`'s
    /// interval: `from + plan.interval.days()`.
    pub fn next_billing_date(&self, plan: &Plan, from: DateTime<Utc>) -> DateTime<Utc> {
        plan.next_billing_date(from)
    }

    /// The billing date one period after `from`, using an explicit
    /// `interval`. Useful when the caller has the interval at hand
    /// without loading the whole [`Plan`].
    pub fn next_billing_date_for_interval(
        &self,
        interval: BillingInterval,
        from: DateTime<Utc>,
    ) -> DateTime<Utc> {
        interval.next_billing_date(from)
    }

    /// The next billing date after the current period ends, using
    /// `plan`'s interval.
    pub fn next_period_end(&self, plan: &Plan) -> DateTime<Utc> {
        self.next_billing_date(plan, self.current_period_end)
    }

    /// Whether this subscription should be charged at `now`.
    ///
    /// All of the following must hold:
    ///
    /// * the status allows billing — `Trialing`, `Active` or `PastDue`
    ///   (never `Cancelled` or `Expired`);
    /// * cancellation is not scheduled (`cancel_at_period_end` is
    ///   `false` — such a subscription ends at the period boundary
    ///   instead of being charged); and
    /// * the current period has ended: `now >= current_period_end`.
    pub fn is_billable(&self, now: DateTime<Utc>) -> bool {
        self.status.allows_billing() && !self.cancel_at_period_end && now >= self.current_period_end
    }

    /// Whether the current period has ended by `now`, regardless of
    /// status or scheduled cancellation.
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        now >= self.current_period_end
    }

    /// Schedule cancellation at the end of the current period. The
    /// subscription keeps working until then and is not charged again.
    pub fn cancel_at_period_end(&mut self) {
        self.cancel_at_period_end = true;
    }

    /// Undo a scheduled cancellation, restoring normal renewal.
    pub fn resume(&mut self) {
        self.cancel_at_period_end = false;
    }

    /// Cancel immediately.
    pub fn cancel_now(&mut self) {
        self.status = SubscriptionStatus::Cancelled;
        self.cancel_at_period_end = false;
    }

    /// Record a failed charge.
    pub fn mark_past_due(&mut self) {
        if !self.status.is_terminal() {
            self.status = SubscriptionStatus::PastDue;
        }
    }

    /// Record a successful charge for the period ending at
    /// `current_period_end`, advancing the period by `plan`'s interval
    /// and restoring [`SubscriptionStatus::Active`] (also the
    /// trial-to-paid transition for a `Trialing` subscription).
    pub fn renew(&mut self, plan: &Plan) {
        self.current_period_end = self.next_period_end(plan);
        self.status = SubscriptionStatus::Active;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
    }

    fn subscription(status: SubscriptionStatus, period_end: DateTime<Utc>) -> Subscription {
        Subscription::new(
            "sub_1",
            "plan_1",
            "merch_1",
            "customer@example.com",
            status,
            period_end,
            false,
            at(2026, 1, 1),
        )
    }

    #[test]
    fn interval_days_are_the_documented_approximations() {
        assert_eq!(BillingInterval::Daily.days(), 1);
        assert_eq!(BillingInterval::Weekly.days(), 7);
        assert_eq!(BillingInterval::Monthly.days(), 30);
        assert_eq!(BillingInterval::Quarterly.days(), 90);
        assert_eq!(BillingInterval::Yearly.days(), 365);
    }

    #[test]
    fn interval_duration_matches_days() {
        for interval in [
            BillingInterval::Daily,
            BillingInterval::Weekly,
            BillingInterval::Monthly,
            BillingInterval::Quarterly,
            BillingInterval::Yearly,
        ] {
            assert_eq!(interval.duration(), Duration::days(interval.days()));
        }
    }

    #[test]
    fn next_billing_date_adds_one_interval() {
        let from = at(2026, 1, 1);
        assert_eq!(
            BillingInterval::Daily.next_billing_date(from),
            at(2026, 1, 2)
        );
        assert_eq!(
            BillingInterval::Weekly.next_billing_date(from),
            at(2026, 1, 8)
        );
        assert_eq!(
            BillingInterval::Monthly.next_billing_date(from),
            at(2026, 1, 31)
        );
        assert_eq!(
            BillingInterval::Quarterly.next_billing_date(from),
            at(2026, 4, 1)
        );
        assert_eq!(
            BillingInterval::Yearly.next_billing_date(from),
            at(2027, 1, 1)
        );
    }

    #[test]
    fn subscription_next_billing_date_uses_the_given_interval() {
        let sub = subscription(SubscriptionStatus::Active, at(2026, 1, 31));
        assert_eq!(
            sub.next_billing_date_for_interval(BillingInterval::Monthly, at(2026, 1, 1)),
            at(2026, 1, 31)
        );
        assert_eq!(
            sub.next_billing_date_for_interval(BillingInterval::Weekly, sub.current_period_end),
            at(2026, 2, 7)
        );
    }

    #[test]
    fn active_subscription_is_billable_once_the_period_ends() {
        let sub = subscription(SubscriptionStatus::Active, at(2026, 2, 1));
        assert!(!sub.is_billable(at(2026, 1, 31)));
        // Boundary: exactly at the period end it is due.
        assert!(sub.is_billable(at(2026, 2, 1)));
        assert!(sub.is_billable(at(2026, 2, 2)));
    }

    #[test]
    fn trialing_and_past_due_subscriptions_are_billable_when_due() {
        for status in [SubscriptionStatus::Trialing, SubscriptionStatus::PastDue] {
            let sub = subscription(status, at(2026, 2, 1));
            assert!(sub.is_billable(at(2026, 2, 1)), "{status:?}");
            assert!(!sub.is_billable(at(2026, 1, 15)), "{status:?}");
        }
    }

    #[test]
    fn cancelled_and_expired_subscriptions_are_never_billable() {
        for status in [SubscriptionStatus::Cancelled, SubscriptionStatus::Expired] {
            let sub = subscription(status, at(2026, 2, 1));
            assert!(!sub.is_billable(at(2026, 2, 1)), "{status:?}");
            assert!(!sub.is_billable(at(2027, 2, 1)), "{status:?}");
            assert!(status.is_terminal());
        }
    }

    #[test]
    fn scheduled_cancellation_blocks_billing_but_not_the_period() {
        let mut sub = subscription(SubscriptionStatus::Active, at(2026, 2, 1));
        sub.cancel_at_period_end();
        assert!(sub.cancel_at_period_end);
        assert!(sub.is_due(at(2026, 2, 1)));
        assert!(!sub.is_billable(at(2026, 2, 1)));

        sub.resume();
        assert!(!sub.cancel_at_period_end);
        assert!(sub.is_billable(at(2026, 2, 1)));
    }

    #[test]
    fn cancel_now_is_terminal_and_not_billable() {
        let mut sub = subscription(SubscriptionStatus::Active, at(2026, 2, 1));
        sub.cancel_now();
        assert_eq!(sub.status, SubscriptionStatus::Cancelled);
        assert!(!sub.cancel_at_period_end);
        assert!(!sub.is_billable(at(2026, 3, 1)));
    }

    #[test]
    fn mark_past_due_keeps_a_live_subscription_billable_but_not_a_terminal_one() {
        let mut sub = subscription(SubscriptionStatus::Active, at(2026, 2, 1));
        sub.mark_past_due();
        assert_eq!(sub.status, SubscriptionStatus::PastDue);
        assert!(sub.is_billable(at(2026, 2, 1)));

        let mut cancelled = subscription(SubscriptionStatus::Cancelled, at(2026, 2, 1));
        cancelled.mark_past_due();
        assert_eq!(cancelled.status, SubscriptionStatus::Cancelled);
    }

    #[test]
    fn only_trialing_active_and_past_due_allow_billing() {
        assert!(SubscriptionStatus::Trialing.allows_billing());
        assert!(SubscriptionStatus::Active.allows_billing());
        assert!(SubscriptionStatus::PastDue.allows_billing());
        assert!(!SubscriptionStatus::Cancelled.allows_billing());
        assert!(!SubscriptionStatus::Expired.allows_billing());
        assert!(!SubscriptionStatus::Active.is_terminal());
    }
}
