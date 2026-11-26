// Manual Bank Transfer — pilot gateway plugin (Wave 119).
//
// Flow: create_payment records a pending payment in the plugin KV
// and returns bank instructions carrying the payment reference.
// The merchant confirms receipt out of band (admin "invoke" of
// confirm_payment, e.g. from the dashboard), after which
// verify_payment reports the payment as verified — the core then
// settles the transaction through POST /transactions/:id/verify.
// Refunds are recorded as manual refunds for a human to execute.

function create_payment(req) {
  var ref = "MBT-" + req.reference;
  kv.set("payment:" + ref, {
    status: "pending",
    amount_minor: req.amount_minor,
    currency: req.currency,
    reference: req.reference,
    merchant_id: req.merchant_id
  });
  var bank = config.get("bank_name") || "the merchant's bank";
  var account = config.get("account_number") || "(account number not configured)";
  var name = config.get("account_name") || "";
  var major = (req.amount_minor / 100).toFixed(2);
  log.info("manual payment created " + ref + " for " + req.amount_minor);
  return {
    provider_ref: ref,
    instructions:
      "Transfer " + req.currency + " " + major + " to " + bank +
      ", account " + account + (name ? " (" + name + ")" : "") +
      ". Use reference " + req.reference + " in the transfer note."
  };
}

function verify_payment(req) {
  var p = kv.get("payment:" + req.provider_ref);
  if (!p) {
    throw new Error("unknown payment " + req.provider_ref);
  }
  return {
    verified: p.status === "paid",
    provider_trx_id: req.provider_ref,
    amount_minor: p.amount_minor,
    sender: p.sender || null
  };
}

// Admin operation (not a gateway operation): mark a payment as
// received after the merchant checks their bank statement.
function confirm_payment(req) {
  var p = kv.get("payment:" + req.provider_ref);
  if (!p) {
    throw new Error("unknown payment " + req.provider_ref);
  }
  p.status = "paid";
  p.sender = req.sender || null;
  kv.set("payment:" + req.provider_ref, p);
  log.info("manual payment confirmed " + req.provider_ref);
  return { ok: true, status: "paid" };
}

function refund(req) {
  var p = kv.get("payment:" + req.provider_ref);
  if (!p) {
    throw new Error("unknown payment " + req.provider_ref);
  }
  p.status = "refunded";
  kv.set("payment:" + req.provider_ref, p);
  return {
    provider_refund_id: "MBT-RF-" + req.provider_ref,
    status: "completed"
  };
}
