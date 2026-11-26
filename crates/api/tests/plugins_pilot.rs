//! Pilot plugin integration tests (Waves 118–120).
//!
//! These run the real plugins from the repository's `plugins/`
//! directory through the real plugin path: scan → engine →
//! gateway dispatch / SMS parsing. No plugin logic is re-implemented
//! here.

use std::io::{Read, Write};
use std::path::PathBuf;

use miraclepay_api::plugins;
use miraclepay_api::state::AppState;
use miraclepay_domain::money::{Currency, Money};
use miraclepay_gateways::types::PaymentRequest;
use miraclepay_sms_verify::types::IncomingSms;

fn plugins_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins")
}

async fn scanned_state() -> AppState {
    let state = AppState::new("plugin-test-secret".to_string());
    plugins::scan_plugins(&state, &plugins_dir()).await;
    state
}

fn payment_request(reference: &str) -> PaymentRequest {
    PaymentRequest {
        merchant_id: "mch_test".to_string(),
        amount: Money::new(50_000, Currency::BDT),
        reference: reference.to_string(),
        customer_name: None,
        customer_email: None,
        customer_phone: None,
        callback_url: None,
        metadata: serde_json::Value::Null,
    }
}

#[tokio::test]
async fn both_pilot_plugins_load_cleanly() {
    let state = scanned_state().await;
    let plugins = state.plugins.read().await;
    for id in ["manual-bank-transfer", "brac-bank-sms"] {
        let plugin = plugins.get(id).unwrap_or_else(|| panic!("{id} scanned"));
        assert!(plugin.load_error.is_none(), "{id}: {:?}", plugin.load_error);
        assert!(plugin.enabled, "{id} enabled by default");
    }
    assert!(plugins::plugin_provides_gateway(&state, "manual_bank_transfer").await);
}

#[tokio::test]
async fn manual_gateway_plugin_completes_a_payment_flow() {
    let state = scanned_state().await;
    let initiated =
        plugins::plugin_initiate(&state, "manual_bank_transfer", &payment_request("order-42"))
            .await
            .expect("initiate through the plugin");
    assert_eq!(initiated.provider_ref, "MBT-order-42");
    assert!(initiated
        .instructions
        .as_deref()
        .unwrap_or("")
        .contains("order-42"));

    // Not paid yet: verify reports unverified with the amount.
    let before = plugins::plugin_verify(&state, "manual_bank_transfer", "MBT-order-42")
        .await
        .expect("verify before confirm");
    assert!(!before.verified);
    assert_eq!(before.amount_minor, 50_000);

    // The merchant confirms receipt (the admin invoke operation).
    {
        let plugins_map = state.plugins.read().await;
        let engine = plugins_map
            .get("manual-bank-transfer")
            .and_then(|p| p.engine.as_ref())
            .expect("engine loaded");
        let out = engine
            .call(
                "confirm_payment",
                &serde_json::json!({"provider_ref": "MBT-order-42", "sender": "A/C **7788"}),
            )
            .expect("confirm_payment");
        assert_eq!(out["status"], "paid");
    }

    let after = plugins::plugin_verify(&state, "manual_bank_transfer", "MBT-order-42")
        .await
        .expect("verify after confirm");
    assert!(after.verified);
    assert_eq!(after.amount_minor, 50_000);
    assert_eq!(after.sender.as_deref(), Some("A/C **7788"));
}

#[tokio::test]
async fn brac_sms_plugin_parses_its_corpus() {
    let state = scanned_state().await;
    let credit = IncomingSms {
        device_id: "dev-1".to_string(),
        sim_slot: None,
        sender: "BRAC BANK".to_string(),
        body: "Your A/C **1234 has been credited with BDT 5,000.00 on 26-Nov-2026 14:32. \
               Ref No: FT26330123456. Avl Bal: BDT 25,300.50. - BRAC BANK"
            .to_string(),
        received_at: chrono::Utc::now(),
    };
    let parsed = plugins::plugin_parse_sms(&state, &credit)
        .await
        .expect("credit alert parses");
    assert_eq!(parsed.amount_minor, Some(500_000));
    assert_eq!(parsed.trx_id.as_deref(), Some("FT26330123456"));
    assert_eq!(parsed.balance_minor, Some(2_530_050));
    assert!(parsed.is_payment());

    // A debit alert and another bank's message must not match.
    for body in [
        "Your A/C **1234 has been debited with BDT 1,200.00 on 26-Nov-2026. Ref No: FT26330999999. - BRAC BANK",
        "You have received Tk 5,000.00 from 01711000000. TrxID 9HX7K2LMN1. - bKash",
    ] {
        let sms = IncomingSms {
            device_id: "dev-1".to_string(),
            sim_slot: None,
            sender: "BRAC BANK".to_string(),
            body: body.to_string(),
            received_at: chrono::Utc::now(),
        };
        assert!(
            plugins::plugin_parse_sms(&state, &sms).await.is_none(),
            "must not match: {body}"
        );
    }
}

#[tokio::test]
async fn http_lane_plugin_answers_create_payment() {
    // A one-shot local HTTP plugin server implementing the
    // docs/plugins.md protocol for create_payment.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).expect("read");
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        assert!(request.starts_with("POST /create_payment"), "{request}");
        let body = "{\"provider_ref\":\"HTTP-1\",\"instructions\":\"pay at the counter\"}";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).expect("write");
    });

    let state = AppState::new("plugin-test-secret".to_string());
    let manifest = miraclepay_plugin_spi::PluginManifest::from_toml_str(&format!(
        r#"
id = "remote-gw"
name = "Remote GW"
version = "0.1.0"
kind = "http"
provides = ["remote_gw"]
min_api_version = "1.0"
endpoint = "http://localhost:{port}"
"#
    ))
    .expect("manifest");
    state.plugins.write().await.insert(
        "remote-gw".to_string(),
        plugins::LoadedPlugin {
            dir_name: "remote-gw".to_string(),
            manifest: Some(manifest),
            engine: None,
            enabled: true,
            load_error: None,
        },
    );

    let initiated = plugins::plugin_initiate(&state, "remote_gw", &payment_request("order-7"))
        .await
        .expect("http lane initiate");
    assert_eq!(initiated.provider_ref, "HTTP-1");
    server.join().expect("server thread");
}
