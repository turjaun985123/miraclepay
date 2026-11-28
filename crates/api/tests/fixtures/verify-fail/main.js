// Test fixture: a gateway plugin that accepts payment creation
// but never verifies — verify_payment always returns
// { verified: false }, which must surface as a payment.failed
// notification (reason "provider_unverified") at the F1 site and
// must NOT change the transaction's status.
function create_payment(req) {
  return {
    provider_ref: "VF-" + req.reference,
    instructions: "Fixture gateway: verification always fails."
  };
}

function verify_payment(req) {
  return { verified: false, provider_trx_id: req.provider_ref };
}
