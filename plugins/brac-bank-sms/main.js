// BRAC Bank credit-alert parser — pilot sms_parser plugin.
//
// Documented example format (from BRAC Bank's alert style):
//   "Your A/C **1234 has been credited with BDT 5,000.00 on
//    26-Nov-2026 14:32. Ref No: FT26330123456. Avl Bal: BDT
//    25,300.50. - BRAC BANK"
// Sender id is typically "BRAC BANK" or "BRACBANK".
//
// Returns matched=false for anything that is not a credit alert
// in this format (debits, OTPs, other banks) so the core keeps
// looking — a parser must never guess.

function parse_amount(text) {
  if (!text) return null;
  var n = parseFloat(text.replace(/,/g, ""));
  if (isNaN(n)) return null;
  return Math.round(n * 100);
}

function parse_sms(sms) {
  var body = sms.body || "";
  var sender = (sms.sender || "").toUpperCase();
  var isBrac = sender.indexOf("BRAC") >= 0 || body.toUpperCase().indexOf("BRAC BANK") >= 0;
  if (!isBrac) return { matched: false };
  if (body.indexOf("credited") < 0) return { matched: false };

  var amount = body.match(/credited with BDT\s+([\d,]+\.\d{2})/);
  var ref = body.match(/Ref No:\s*([A-Za-z0-9]+)/);
  if (!amount || !ref) return { matched: false };

  var bal = body.match(/Avl Bal:\s*BDT\s+([\d,]+\.\d{2})/);
  var acct = body.match(/A\/C \*\*(\d{4})/);

  return {
    matched: true,
    amount_minor: parse_amount(amount[1]),
    currency: "BDT",
    trx_id: ref[1],
    reference: ref[1],
    balance_minor: bal ? parse_amount(bal[1]) : null,
    receiver_number: acct ? "****" + acct[1] : null
  };
}
