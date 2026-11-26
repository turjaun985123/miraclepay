//! Sandbox mode + demo seed (Wave 46).
//!
//! When the API starts with `SANDBOX_MODE=1`, an empty store is
//! seeded with a small, clearly-labelled demo dataset so a new
//! operator (or a curious merchant) can click through a working
//! dashboard in minutes: a demo owner, one merchant, two weeks of
//! transactions (so the Wave-41 chart has shape), a payment link
//! and an invoice.
//!
//! Safety rails, all deliberate:
//! - Seeding only happens when the user store is **empty** and
//!   only when the env flag is set; there is no HTTP trigger.
//! - The demo login uses a published, well-known password
//!   (`demo1234`) — acceptable *only* because sandbox mode is a
//!   local evaluation tool. The startup log says so loudly, the
//!   health endpoint reports `"sandbox": true`, and
//!   `docs/operations/sandbox.md` repeats: never enable it on a
//!   public deployment.
//! - All demo money is fake by construction: the seeded
//!   transactions exist only in the demo store.

use chrono::{Duration, Utc};

use miraclepay_domain::invoice::{Invoice, InvoiceItem, InvoiceStatus};
use miraclepay_domain::merchant::{Merchant, MerchantStatus};
use miraclepay_domain::money::{Currency, Money};
use miraclepay_domain::payment_link::PaymentLink;
use miraclepay_domain::transaction::{Transaction, TxStatus};
use miraclepay_domain::user::{User, UserRole};
use miraclepay_domain::{ids, password};

use crate::state::AppState;

/// The demo owner's email (documented, sandbox-only).
pub const DEMO_EMAIL: &str = "demo@miraclepay.local";
/// The demo owner's password (published on purpose — sandbox only).
pub const DEMO_PASSWORD: &str = "demo1234";

/// Whether sandbox mode is enabled via the environment.
pub fn sandbox_enabled() -> bool {
    matches!(
        std::env::var("SANDBOX_MODE").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

impl AppState {
    /// Seed the demo dataset. Returns the number of transactions
    /// created (0 when the store was not empty — seeding is a
    /// no-op then, so restarts never duplicate the demo).
    pub async fn seed_demo(&self) -> usize {
        {
            let users = self.users.read().await;
            if !users.is_empty() {
                return 0;
            }
        }
        let now = Utc::now();

        let owner = User {
            id: ids::user_id(),
            name: "Demo Owner".to_string(),
            email: DEMO_EMAIL.to_string(),
            password_hash: password::hash_password(DEMO_PASSWORD),
            role: UserRole::Merchant,
            email_verified: true,
            suspended: false,
            totp_secret: None,
            totp_enabled: false,
            created_at: now,
        };
        let owner_id = owner.id.clone();
        self.users.write().await.insert(owner_id.clone(), owner);

        let merchant = Merchant {
            id: ids::merchant_id(),
            owner_user_id: owner_id,
            business_name: "Demo Store (Sandbox)".to_string(),
            email: DEMO_EMAIL.to_string(),
            phone: Some("01712345678".to_string()),
            country: "BD".to_string(),
            status: MerchantStatus::Active,
            webhook_url: None,
            created_at: now,
        };
        let merchant_id = merchant.id.clone();
        self.merchants
            .write()
            .await
            .insert(merchant_id.clone(), merchant);

        // Fourteen days of history: a couple of completed payments
        // most days (varying amounts), a pending and a failed one
        // for texture. Deterministic pattern — no RNG needed.
        let gateways = ["bkash", "nagad", "rocket"];
        let mut created = 0usize;
        let mut transactions = self.transactions.write().await;
        for day in 0..14i64 {
            let date = now - Duration::days(13 - day);
            let count = 1 + (day % 3);
            for n in 0..count {
                let amount = Money::new((450 + day * 137 + n * 265) * 100, Currency::BDT);
                let status = if n == 0 && day == 13 {
                    TxStatus::Pending
                } else if n == 1 && day == 12 {
                    TxStatus::Failed
                } else {
                    TxStatus::Completed
                };
                let tx = Transaction {
                    id: ids::transaction_id(),
                    merchant_id: merchant_id.clone(),
                    gateway: gateways[(day as usize + n as usize) % gateways.len()].to_string(),
                    amount,
                    status,
                    customer_email: Some("customer@example.com".to_string()),
                    customer_phone: Some("01712345678".to_string()),
                    provider_trx_id: Some(format!("DEMO{:06}", day * 10 + n)),
                    sender_number: Some("01712345678".to_string()),
                    reference: Some(format!("demo-order-{}-{}", day, n)),
                    metadata: serde_json::json!({ "demo": true }),
                    created_at: date,
                    updated_at: date,
                };
                transactions.insert(tx.id.clone(), tx);
                created += 1;
            }
        }
        drop(transactions);

        let link = PaymentLink {
            id: ids::payment_link_id(),
            merchant_id: merchant_id.clone(),
            slug: "demo-store-pay".to_string(),
            title: "Pay Demo Store".to_string(),
            description: Some("Sandbox payment link — no real money.".to_string()),
            amount: Some(Money::new(500_00, Currency::BDT)),
            collect_phone: true,
            collect_email: false,
            active: true,
            uses: 0,
            max_uses: None,
            expires_at: None,
            created_at: now,
        };
        self.payment_links
            .write()
            .await
            .insert(link.id.clone(), link);

        let invoice = Invoice {
            id: ids::invoice_id(),
            merchant_id: merchant_id.clone(),
            number: "INV-DEMO-0001".to_string(),
            customer_name: "Demo Customer".to_string(),
            customer_email: "customer@example.com".to_string(),
            items: vec![InvoiceItem {
                description: "Sandbox item".to_string(),
                quantity: 2,
                unit_amount_minor: 250_00,
            }],
            currency: Currency::BDT,
            tax_bps: 0,
            discount_minor: 0,
            status: InvoiceStatus::Sent,
            due_at: Some(now + Duration::days(7)),
            paid_transaction_id: None,
            created_at: now,
        };
        self.invoices
            .write()
            .await
            .insert(invoice.id.clone(), invoice);

        created
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn seed_creates_a_demo_world_once() {
        let state = AppState::new("test-secret".to_string());
        let created = state.seed_demo().await;
        assert!(created >= 14, "expected at least 14 demo tx, got {created}");
        assert_eq!(state.users.read().await.len(), 1);
        assert_eq!(state.merchants.read().await.len(), 1);
        assert_eq!(state.payment_links.read().await.len(), 1);
        assert_eq!(state.invoices.read().await.len(), 1);
        // Idempotent: a second run changes nothing.
        assert_eq!(state.seed_demo().await, 0);
        assert_eq!(state.transactions.read().await.len(), created);
    }
}
