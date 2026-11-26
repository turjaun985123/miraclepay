//! Gateway registry for MiraclePay v0.0.1.
//!
//! Original clean-room implementation.
//!
//! The registry is the one place where concrete gateway adapters
//! meet the rest of the platform. Adapters are constructed once —
//! either from stored [`crate::types::GatewayConfig`] values via
//! [`GatewayRegistry::from_configs`], or unconfigured via
//! [`GatewayRegistry::all_unconfigured`] — and stored behind
//! `Arc<dyn PaymentGateway>`, keyed by their
//! [`crate::types::GatewayId`]. Callers then look a gateway up by
//! id (or by its string identifier) and use it only through the
//! [`crate::traits::PaymentGateway`] trait, so checkout, webhooks,
//! and settlement code never depend on a concrete adapter type.
//!
//! Registering a second adapter under an id that is already
//! present replaces the first one: the last registration wins.
//! This makes it straightforward to rebuild the registry when a
//! merchant's gateway configuration changes.

use std::collections::HashMap;
use std::sync::Arc;

use crate::traits::PaymentGateway;
use crate::types::GatewayId;

/// A collection of payment gateway adapters, keyed by gateway id.
pub struct GatewayRegistry {
    gateways: HashMap<crate::types::GatewayId, Arc<dyn crate::traits::PaymentGateway>>,
}

impl GatewayRegistry {
    /// An empty registry. No gateway can be looked up until one is
    /// registered with [`GatewayRegistry::register`].
    pub fn new() -> Self {
        GatewayRegistry {
            gateways: HashMap::new(),
        }
    }

    /// Register a gateway adapter, keyed by the id the adapter
    /// itself reports via [`PaymentGateway::id`]. If an adapter is
    /// already registered under that id, it is replaced.
    pub fn register(&mut self, gw: Arc<dyn PaymentGateway>) {
        self.gateways.insert(gw.id(), gw);
    }

    /// Look up the gateway registered under `id`, if any.
    pub fn get(&self, id: GatewayId) -> Option<Arc<dyn PaymentGateway>> {
        self.gateways.get(&id).cloned()
    }

    /// Look up a gateway by its string identifier, e.g. `"bkash"`.
    ///
    /// Matching follows [`GatewayId::from_str`]: it is
    /// case-insensitive (so `"BKASH"` and `"bKash"` both find the
    /// bKash gateway) and surrounding whitespace is ignored.
    /// Unknown identifiers return `None`, as do known identifiers
    /// for gateways that are not registered.
    pub fn get_by_str(&self, s: &str) -> Option<Arc<dyn PaymentGateway>> {
        GatewayId::from_str(s).and_then(|id| self.get(id))
    }

    /// The ids of all registered gateways, in the canonical order
    /// of [`GatewayId::all`].
    pub fn list(&self) -> Vec<GatewayId> {
        GatewayId::all()
            .iter()
            .copied()
            .filter(|id| self.gateways.contains_key(id))
            .collect()
    }

    /// The ids of the registered gateways that report themselves
    /// as configured (see [`PaymentGateway::is_configured`]) and
    /// can therefore actually take payments, in the canonical
    /// order of [`GatewayId::all`].
    pub fn configured_ids(&self) -> Vec<GatewayId> {
        GatewayId::all()
            .iter()
            .copied()
            .filter(|id| {
                self.gateways
                    .get(id)
                    .map(|gw| gw.is_configured())
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Build a registry from stored gateway configurations.
    ///
    /// Each config produces exactly one adapter of the concrete
    /// type matching `config.gateway`, constructed with that
    /// gateway's `with_config`. Configs for the same gateway
    /// appearing more than once collapse to the last one, because
    /// registration is keyed by gateway id. Gateways with no
    /// config in `configs` are simply not registered; use
    /// [`GatewayRegistry::all_unconfigured`] when every gateway
    /// should be present.
    pub fn from_configs(configs: &[crate::types::GatewayConfig]) -> GatewayRegistry {
        let mut registry = GatewayRegistry::new();
        for config in configs {
            let gateway: Arc<dyn PaymentGateway> = match config.gateway {
                GatewayId::Bkash => {
                    Arc::new(crate::bkash::BkashGateway::with_config(config.clone()))
                }
                GatewayId::Nagad => {
                    Arc::new(crate::nagad::NagadGateway::with_config(config.clone()))
                }
                GatewayId::Rocket => {
                    Arc::new(crate::rocket::RocketGateway::with_config(config.clone()))
                }
                GatewayId::Upay => Arc::new(crate::upay::UpayGateway::with_config(config.clone())),
                GatewayId::Surecash => Arc::new(crate::surecash::SurecashGateway::with_config(
                    config.clone(),
                )),
                GatewayId::Upi => Arc::new(crate::upi::UpiGateway::with_config(config.clone())),
                GatewayId::Paytm => {
                    Arc::new(crate::paytm::PaytmGateway::with_config(config.clone()))
                }
                GatewayId::Phonepe => {
                    Arc::new(crate::phonepe::PhonepeGateway::with_config(config.clone()))
                }
                GatewayId::Razorpay => Arc::new(crate::razorpay::RazorpayGateway::with_config(
                    config.clone(),
                )),
                GatewayId::Easypaisa => Arc::new(crate::easypaisa::EasypaisaGateway::with_config(
                    config.clone(),
                )),
                GatewayId::Jazzcash => Arc::new(crate::jazzcash::JazzcashGateway::with_config(
                    config.clone(),
                )),
                GatewayId::Stripe => {
                    Arc::new(crate::stripe::StripeGateway::with_config(config.clone()))
                }
                GatewayId::Paypal => {
                    Arc::new(crate::paypal::PaypalGateway::with_config(config.clone()))
                }
                GatewayId::Manual => {
                    Arc::new(crate::manual::ManualGateway::with_config(config.clone()))
                }
            };
            registry.register(gateway);
        }
        registry
    }

    /// A registry containing every supported gateway, each built
    /// unconfigured.
    ///
    /// This is the starting point for a merchant who has not set
    /// up any gateway yet: all fourteen gateways are listed (so
    /// they can be offered for setup) but none of them reports
    /// itself as configured or can take a payment.
    pub fn all_unconfigured() -> GatewayRegistry {
        let mut registry = GatewayRegistry::new();
        registry.register(Arc::new(crate::bkash::BkashGateway::new_unconfigured()));
        registry.register(Arc::new(crate::nagad::NagadGateway::new_unconfigured()));
        registry.register(Arc::new(crate::rocket::RocketGateway::new_unconfigured()));
        registry.register(Arc::new(crate::upay::UpayGateway::new_unconfigured()));
        registry.register(Arc::new(
            crate::surecash::SurecashGateway::new_unconfigured(),
        ));
        registry.register(Arc::new(crate::upi::UpiGateway::new_unconfigured()));
        registry.register(Arc::new(crate::paytm::PaytmGateway::new_unconfigured()));
        registry.register(Arc::new(crate::phonepe::PhonepeGateway::new_unconfigured()));
        registry.register(Arc::new(
            crate::razorpay::RazorpayGateway::new_unconfigured(),
        ));
        registry.register(Arc::new(
            crate::easypaisa::EasypaisaGateway::new_unconfigured(),
        ));
        registry.register(Arc::new(
            crate::jazzcash::JazzcashGateway::new_unconfigured(),
        ));
        registry.register(Arc::new(crate::stripe::StripeGateway::new_unconfigured()));
        registry.register(Arc::new(crate::paypal::PaypalGateway::new_unconfigured()));
        registry.register(Arc::new(crate::manual::ManualGateway::new_unconfigured()));
        registry
    }
}

impl Default for GatewayRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GatewayConfig;

    fn bkash_config() -> GatewayConfig {
        GatewayConfig {
            gateway: GatewayId::Bkash,
            merchant_number: Some("01700000000".to_string()),
            api_key: None,
            api_secret: None,
            extra: serde_json::Value::Null,
            sandbox: false,
            enabled: true,
        }
    }

    #[test]
    fn from_configs_with_enabled_bkash_number_is_configured() {
        let registry = GatewayRegistry::from_configs(&[bkash_config()]);

        assert!(registry.configured_ids().contains(&GatewayId::Bkash));
        assert_eq!(registry.list(), vec![GatewayId::Bkash]);

        let gateway = registry
            .get(GatewayId::Bkash)
            .expect("bkash gateway must be registered");
        assert_eq!(gateway.id(), GatewayId::Bkash);
        assert!(gateway.is_configured());
    }

    #[test]
    fn from_configs_with_no_configs_is_empty() {
        let registry = GatewayRegistry::from_configs(&[]);
        assert!(registry.list().is_empty());
        assert!(registry.configured_ids().is_empty());
        assert!(registry.get(GatewayId::Bkash).is_none());
    }

    #[test]
    fn all_unconfigured_registers_all_fourteen_gateways_none_configured() {
        let registry = GatewayRegistry::all_unconfigured();

        assert_eq!(registry.list().len(), 14);
        assert_eq!(
            registry.list(),
            GatewayId::all().to_vec(),
            "every gateway in GatewayId::all() must be registered"
        );
        assert!(
            registry.configured_ids().is_empty(),
            "no unconfigured gateway may report itself as configured"
        );
        for id in GatewayId::all() {
            let gateway = registry
                .get(*id)
                .unwrap_or_else(|| panic!("gateway {id:?} must be registered"));
            assert_eq!(gateway.id(), *id);
            assert!(!gateway.is_configured(), "{id:?} must be unconfigured");
        }
    }

    #[test]
    fn get_by_str_is_case_insensitive() {
        let registry = GatewayRegistry::all_unconfigured();

        for spelling in ["bkash", "BKASH", "bKash", "Bkash"] {
            let gateway = registry
                .get_by_str(spelling)
                .unwrap_or_else(|| panic!("get_by_str({spelling:?}) must find bKash"));
            assert_eq!(gateway.id(), GatewayId::Bkash);
        }
        assert_eq!(
            registry
                .get_by_str("STRIPE")
                .expect("get_by_str(\"STRIPE\") must find Stripe")
                .id(),
            GatewayId::Stripe
        );
        assert_eq!(
            registry
                .get_by_str("PayPal")
                .expect("get_by_str(\"PayPal\") must find PayPal")
                .id(),
            GatewayId::Paypal
        );
    }

    #[test]
    fn get_by_str_rejects_unknown_identifiers() {
        let registry = GatewayRegistry::all_unconfigured();
        assert!(registry.get_by_str("").is_none());
        assert!(registry.get_by_str("unknown").is_none());
        assert!(registry.get_by_str("bkash2").is_none());
    }

    #[test]
    fn register_keys_by_gateway_id_and_replaces_duplicates() {
        let mut registry = GatewayRegistry::new();
        assert!(registry.get(GatewayId::Bkash).is_none());

        registry.register(Arc::new(crate::bkash::BkashGateway::new_unconfigured()));
        assert_eq!(registry.list(), vec![GatewayId::Bkash]);
        assert!(registry.configured_ids().is_empty());

        // Re-registering under the same id replaces, not duplicates.
        registry.register(Arc::new(crate::bkash::BkashGateway::with_config(
            bkash_config(),
        )));
        assert_eq!(registry.list(), vec![GatewayId::Bkash]);
        assert_eq!(registry.configured_ids(), vec![GatewayId::Bkash]);
    }
}
