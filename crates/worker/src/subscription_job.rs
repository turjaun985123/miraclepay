//! Subscription billing-cycle engine (Wave 16).
//!
//! The domain layer has always known *when* a subscription is due
//! ([`Subscription::is_billable`], [`Subscription::renew`],
//! [`Subscription::mark_past_due`]) but nothing ever *ran* the
//! cycle. This job is that runner: given the subscriptions, their
//! plans, the current time, and a charge function, it walks every
//! due subscription exactly once and reports what happened.
//!
//! Policy:
//! - Not due yet, terminal, or plan missing → skipped, untouched.
//! - Due with `cancel_at_period_end` → `Expired` (the period ran
//!   out; never charged).
//! - Due and billable → charge via the injected [`Charger`]:
//!   success renews (period advances by the plan interval; a
//!   trialing subscription becomes active); failure marks the
//!   subscription `PastDue` (dunning) — and a subscription that is
//!   *already* `PastDue` and still failing once its period end is
//!   older than the grace window is `Expired` instead, so dunning
//!   cannot retry forever.
//!
//! The charge itself is injected (same pattern as the webhook
//! transport) so the policy is unit-testable without a gateway.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use miraclepay_domain::subscription::{Plan, Subscription, SubscriptionStatus};

/// Default dunning grace: a past-due subscription keeps retrying
/// for 7 days after its period end before it expires.
pub const DEFAULT_GRACE: Duration = Duration::days(7);

/// Performs one charge attempt. `Ok` carries the provider
/// reference for the successful charge; `Err` a failure message.
pub trait Charger {
    fn charge(&mut self, subscription: &Subscription, plan: &Plan) -> Result<String, String>;
}

/// What one billing cycle did, by subscription id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BillingReport {
    /// Charged successfully and renewed.
    pub renewed: Vec<String>,
    /// Charge failed; marked (or kept) past due for dunning retry.
    pub past_due: Vec<String>,
    /// Ended this cycle: scheduled cancellation reached its period
    /// end, or dunning grace was exhausted.
    pub expired: Vec<String>,
    /// Not due, terminal, or plan missing — untouched.
    pub skipped: usize,
}

/// Run one billing cycle over `subscriptions` at `now`.
pub fn run_billing_cycle<C: Charger>(
    subscriptions: &mut [Subscription],
    plans: &HashMap<String, Plan>,
    now: DateTime<Utc>,
    grace: Duration,
    charger: &mut C,
) -> BillingReport {
    let mut report = BillingReport::default();
    for sub in subscriptions.iter_mut() {
        if sub.status.is_terminal() || !sub.is_due(now) {
            report.skipped += 1;
            continue;
        }
        // Period ended with a scheduled cancellation: expire,
        // never charge.
        if sub.cancel_at_period_end {
            sub.status = SubscriptionStatus::Expired;
            report.expired.push(sub.id.clone());
            continue;
        }
        let Some(plan) = plans.get(&sub.plan_id) else {
            report.skipped += 1;
            continue;
        };
        if !sub.is_billable(now) {
            report.skipped += 1;
            continue;
        }
        match charger.charge(sub, plan) {
            Ok(_provider_ref) => {
                sub.renew(plan);
                report.renewed.push(sub.id.clone());
            }
            Err(_err) => {
                let grace_exhausted = sub.status == SubscriptionStatus::PastDue
                    && now >= sub.current_period_end + grace;
                if grace_exhausted {
                    sub.status = SubscriptionStatus::Expired;
                    report.expired.push(sub.id.clone());
                } else {
                    sub.mark_past_due();
                    report.past_due.push(sub.id.clone());
                }
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use miraclepay_domain::money::{Currency, Money};
    use miraclepay_domain::subscription::BillingInterval;

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, 0, 0, 0).unwrap()
    }

    fn plan() -> Plan {
        Plan::new(
            "plan_1",
            "merch_1",
            "Pro monthly",
            Money::new(10_000, Currency::BDT),
            BillingInterval::Monthly,
            0,
            true,
        )
    }

    fn sub(id: &str, status: SubscriptionStatus, period_end: DateTime<Utc>) -> Subscription {
        Subscription::new(
            id,
            "plan_1",
            "merch_1",
            "c@example.com",
            status,
            period_end,
            false,
            at(1),
        )
    }

    struct OkCharger;
    impl Charger for OkCharger {
        fn charge(&mut self, _s: &Subscription, _p: &Plan) -> Result<String, String> {
            Ok("ref-1".to_string())
        }
    }
    struct FailCharger;
    impl Charger for FailCharger {
        fn charge(&mut self, _s: &Subscription, _p: &Plan) -> Result<String, String> {
            Err("card declined".to_string())
        }
    }

    fn plans() -> HashMap<String, Plan> {
        HashMap::from([("plan_1".to_string(), plan())])
    }

    #[test]
    fn due_active_subscription_renews_and_advances_period() {
        let mut subs = vec![sub("s1", SubscriptionStatus::Active, at(10))];
        let report = run_billing_cycle(&mut subs, &plans(), at(11), DEFAULT_GRACE, &mut OkCharger);
        assert_eq!(report.renewed, vec!["s1"]);
        assert_eq!(subs[0].status, SubscriptionStatus::Active);
        // Monthly interval = 30 days: Nov 10 + 30d = Dec 10.
        assert_eq!(subs[0].current_period_end, at(10) + Duration::days(30));
    }

    #[test]
    fn trial_end_converts_to_active_on_first_charge() {
        let mut subs = vec![sub("s1", SubscriptionStatus::Trialing, at(10))];
        let report = run_billing_cycle(&mut subs, &plans(), at(10), DEFAULT_GRACE, &mut OkCharger);
        assert_eq!(report.renewed.len(), 1);
        assert_eq!(subs[0].status, SubscriptionStatus::Active);
    }

    #[test]
    fn failed_charge_marks_past_due_then_grace_expiry_ends_it() {
        let mut subs = vec![sub("s1", SubscriptionStatus::Active, at(10))];
        // First failure at due date: past due, not expired.
        let r1 = run_billing_cycle(&mut subs, &plans(), at(10), DEFAULT_GRACE, &mut FailCharger);
        assert_eq!(r1.past_due, vec!["s1"]);
        assert_eq!(subs[0].status, SubscriptionStatus::PastDue);
        // Still failing 8 days later (grace = 7): expired.
        let r2 = run_billing_cycle(&mut subs, &plans(), at(18), DEFAULT_GRACE, &mut FailCharger);
        assert_eq!(r2.expired, vec!["s1"]);
        assert_eq!(subs[0].status, SubscriptionStatus::Expired);
    }

    #[test]
    fn scheduled_cancellation_expires_without_charging() {
        let mut s = sub("s1", SubscriptionStatus::Active, at(10));
        s.cancel_at_period_end();
        let mut subs = vec![s];
        // OkCharger would succeed — it must never be reached.
        let report = run_billing_cycle(&mut subs, &plans(), at(12), DEFAULT_GRACE, &mut OkCharger);
        assert_eq!(report.expired, vec!["s1"]);
        assert!(report.renewed.is_empty());
    }

    #[test]
    fn not_due_and_terminal_are_skipped_untouched() {
        let mut subs = vec![
            sub("future", SubscriptionStatus::Active, at(28)),
            sub("gone", SubscriptionStatus::Cancelled, at(10)),
        ];
        let report = run_billing_cycle(&mut subs, &plans(), at(11), DEFAULT_GRACE, &mut OkCharger);
        assert_eq!(report.skipped, 2);
        assert_eq!(subs[0].status, SubscriptionStatus::Active);
        assert_eq!(subs[1].status, SubscriptionStatus::Cancelled);
    }

    #[test]
    fn missing_plan_is_skipped_not_charged() {
        let mut subs = vec![sub("s1", SubscriptionStatus::Active, at(10))];
        let report = run_billing_cycle(
            &mut subs,
            &HashMap::new(),
            at(11),
            DEFAULT_GRACE,
            &mut OkCharger,
        );
        assert_eq!(report.skipped, 1);
        assert!(report.renewed.is_empty());
    }
}
