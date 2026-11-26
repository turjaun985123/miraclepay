//! Platform analytics aggregation (Wave 19).
//!
//! Pure functions over transaction slices, in the same style as
//! [`crate::stats`]: the admin analytics endpoint feeds them the
//! full transaction list and serialises the result.
//!
//! Conventions (shared with `compute_stats`): counts cover every
//! transaction regardless of currency; money volumes sum only
//! completed transactions in the reporting currency (BDT), so a
//! multi-currency platform never adds taka to rupees.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;

use crate::money::Currency;
use crate::transaction::{Transaction, TxStatus};

/// One day of platform activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DayBucket {
    /// Calendar day, `YYYY-MM-DD` (UTC).
    pub date: String,
    /// All transactions created that day.
    pub count: u64,
    /// Transactions created that day that are now completed.
    pub completed: u64,
    /// Sum of completed BDT transaction amounts that day, minor units.
    pub volume_minor: i64,
}

/// Transaction activity for the last `days` days ending at `now`'s
/// calendar day (UTC), oldest first, zero-filled for quiet days.
pub fn daily_series(transactions: &[Transaction], days: u32, now: DateTime<Utc>) -> Vec<DayBucket> {
    let days = days.max(1);
    let today: NaiveDate = now.date_naive();
    let start = today - Duration::days(i64::from(days) - 1);

    let mut buckets: BTreeMap<NaiveDate, DayBucket> = BTreeMap::new();
    for offset in 0..i64::from(days) {
        let date = start + Duration::days(offset);
        buckets.insert(
            date,
            DayBucket {
                date: date.format("%Y-%m-%d").to_string(),
                count: 0,
                completed: 0,
                volume_minor: 0,
            },
        );
    }
    for tx in transactions {
        let date = tx.created_at.date_naive();
        if let Some(bucket) = buckets.get_mut(&date) {
            bucket.count += 1;
            if tx.status == TxStatus::Completed {
                bucket.completed += 1;
                if tx.amount.currency == Currency::BDT {
                    bucket.volume_minor =
                        bucket.volume_minor.saturating_add(tx.amount.amount_minor);
                }
            }
        }
    }
    buckets.into_values().collect()
}

/// Per-gateway activity breakdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayBreakdown {
    pub gateway: String,
    pub count: u64,
    pub completed: u64,
    /// Completed BDT volume, minor units.
    pub volume_minor: i64,
}

/// Aggregate transactions by gateway, most active first (ties by
/// gateway name for determinism).
pub fn gateway_breakdown(transactions: &[Transaction]) -> Vec<GatewayBreakdown> {
    let mut by_gateway: BTreeMap<String, GatewayBreakdown> = BTreeMap::new();
    for tx in transactions {
        let entry = by_gateway
            .entry(tx.gateway.clone())
            .or_insert_with(|| GatewayBreakdown {
                gateway: tx.gateway.clone(),
                count: 0,
                completed: 0,
                volume_minor: 0,
            });
        entry.count += 1;
        if tx.status == TxStatus::Completed {
            entry.completed += 1;
            if tx.amount.currency == Currency::BDT {
                entry.volume_minor = entry.volume_minor.saturating_add(tx.amount.amount_minor);
            }
        }
    }
    let mut out: Vec<GatewayBreakdown> = by_gateway.into_values().collect();
    out.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.gateway.cmp(&b.gateway))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;
    use chrono::TimeZone;

    fn at(day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, day, hour, 0, 0).unwrap()
    }

    fn tx(
        id: &str,
        gateway: &str,
        status: TxStatus,
        minor: i64,
        when: DateTime<Utc>,
    ) -> Transaction {
        Transaction {
            id: id.to_string(),
            merchant_id: "m-1".to_string(),
            gateway: gateway.to_string(),
            amount: Money::new(minor, Currency::BDT),
            status,
            customer_email: None,
            customer_phone: None,
            provider_trx_id: None,
            sender_number: None,
            reference: None,
            metadata: serde_json::Value::Null,
            created_at: when,
            updated_at: when,
        }
    }

    #[test]
    fn daily_series_zero_fills_and_buckets_correctly() {
        let txs = vec![
            tx("a", "bkash", TxStatus::Completed, 10_000, at(26, 1)),
            tx("b", "bkash", TxStatus::Pending, 5_000, at(26, 2)),
            tx("c", "nagad", TxStatus::Completed, 7_500, at(24, 9)),
        ];
        let series = daily_series(&txs, 3, at(26, 12));
        assert_eq!(series.len(), 3);
        assert_eq!(series[0].date, "2026-11-24");
        assert_eq!(series[0].count, 1);
        assert_eq!(series[0].volume_minor, 7_500);
        assert_eq!(series[1].count, 0); // quiet day zero-filled
        assert_eq!(series[2].count, 2);
        assert_eq!(series[2].completed, 1);
        assert_eq!(series[2].volume_minor, 10_000); // pending adds no volume
    }

    #[test]
    fn gateway_breakdown_sorts_by_count() {
        let txs = vec![
            tx("a", "bkash", TxStatus::Completed, 100, at(26, 1)),
            tx("b", "bkash", TxStatus::Completed, 200, at(26, 2)),
            tx("c", "nagad", TxStatus::Failed, 300, at(26, 3)),
        ];
        let out = gateway_breakdown(&txs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].gateway, "bkash");
        assert_eq!(out[0].count, 2);
        assert_eq!(out[0].volume_minor, 300);
        assert_eq!(out[1].gateway, "nagad");
        assert_eq!(out[1].completed, 0);
    }
}
