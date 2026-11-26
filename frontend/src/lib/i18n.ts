/**
 * Minimal i18n for the customer-facing pages (Wave 22).
 *
 * English is the source language; Bangla (bn) translations cover
 * the public pay page and the hosted checkout — the two pages a
 * Bangladeshi customer actually sees. Dashboard/admin pages remain
 * English in v0.0.1 (noted in WAVES.md).
 *
 * Usage: mark static strings with `data-i18n="key"`, call
 * `applyI18n()` on page load, and use `t('key')` for strings set
 * from script. The choice persists in localStorage (`mp-lang`) and
 * `document.documentElement.lang` is kept in sync for screen
 * readers and browser translation.
 */

export type Lang = 'en' | 'bn';

const STRINGS: Record<string, { en: string; bn: string }> = {
  // Public pay page
  pay_loading: { en: 'Loading payment link…', bn: 'পেমেন্ট লিংক লোড হচ্ছে…' },
  pay_error_heading: {
    en: 'This payment link can’t be used',
    bn: 'এই পেমেন্ট লিংকটি ব্যবহার করা যাচ্ছে না',
  },
  pay_error_hint: {
    en: 'The link may be inactive, expired, or fully used. Please ask the merchant for a fresh link.',
    bn: 'লিংকটি নিষ্ক্রিয়, মেয়াদোত্তীর্ণ বা সম্পূর্ণ ব্যবহৃত হতে পারে। অনুগ্রহ করে বিক্রেতার কাছ থেকে নতুন একটি লিংক নিন।',
  },
  pay_title_default: { en: 'Payment', bn: 'পেমেন্ট' },
  pay_amount: { en: 'Amount', bn: 'পরিমাণ' },
  pay_phone: { en: 'Phone number', bn: 'ফোন নম্বর' },
  pay_email: { en: 'Email address', bn: 'ইমেইল ঠিকানা' },
  pay_with: { en: 'Pay with', bn: 'যেভাবে পেমেন্ট করবেন' },
  pay_now: { en: 'Pay now', bn: 'এখনই পেমেন্ট করুন' },
  pay_now_amount: { en: 'Pay {amount}', bn: '{amount} পেমেন্ট করুন' },
  pay_amount_hint: {
    en: 'Enter the amount in {currency}.',
    bn: '{currency} মুদ্রায় পরিমাণটি লিখুন।',
  },
  pay_started: { en: 'Payment started', bn: 'পেমেন্ট শুরু হয়েছে' },
  pay_started_body: {
    en: 'Your payment has been created and is waiting for the money to arrive.',
    bn: 'আপনার পেমেন্ট তৈরি হয়েছে এবং অর্থ পৌঁছানোর অপেক্ষায় রয়েছে।',
  },
  pay_transaction_id: { en: 'Transaction ID', bn: 'লেনদেন আইডি' },
  pay_reference: { en: 'Reference', bn: 'রেফারেন্স' },
  pay_finish_body: {
    en: 'To finish, send the money with your chosen payment app and use the reference above exactly as shown. Once the payment SMS reaches the merchant, MiraclePay verifies it automatically — you don’t need to upload a screenshot or do anything else on this page.',
    bn: 'সম্পন্ন করতে, আপনার পছন্দের পেমেন্ট অ্যাপ দিয়ে টাকা পাঠান এবং উপরের রেফারেন্সটি হুবহু ব্যবহার করুন। পেমেন্টের SMS বিক্রেতার কাছে পৌঁছালেই MiraclePay স্বয়ংক্রিয়ভাবে যাচাই করে নেবে — স্ক্রিনশট আপলোড করা বা এই পেজে আর কিছু করার দরকার নেই।',
  },
  pay_keep_note: {
    en: 'Keep your transaction ID and reference until the payment is confirmed.',
    bn: 'পেমেন্ট নিশ্চিত না হওয়া পর্যন্ত আপনার লেনদেন আইডি ও রেফারেন্স সংরক্ষণ করুন।',
  },
  pay_err_amount: {
    en: 'Please enter an amount greater than zero.',
    bn: 'অনুগ্রহ করে শূন্যের চেয়ে বড় একটি পরিমাণ লিখুন।',
  },
  pay_err_phone: {
    en: 'Please enter your phone number.',
    bn: 'অনুগ্রহ করে আপনার ফোন নম্বর লিখুন।',
  },
  pay_err_email: {
    en: 'Please enter your email address.',
    bn: 'অনুগ্রহ করে আপনার ইমেইল ঠিকানা লিখুন।',
  },
  pay_err_gateway: {
    en: 'Please choose how you want to pay.',
    bn: 'অনুগ্রহ করে পেমেন্টের মাধ্যম বেছে নিন।',
  },
  pay_err_start: {
    en: 'The payment could not be started. Please try again.',
    bn: 'পেমেন্ট শুরু করা যায়নি। অনুগ্রহ করে আবার চেষ্টা করুন।',
  },
  pay_err_load: {
    en: 'This payment link could not be loaded.',
    bn: 'এই পেমেন্ট লিংকটি লোড করা যায়নি।',
  },
  pay_err_no_link: {
    en: 'No payment link was specified in this URL.',
    bn: 'এই URL-এ কোনো পেমেন্ট লিংক দেওয়া হয়নি।',
  },
  // Hosted checkout
  co_loading: { en: 'Loading checkout…', bn: 'চেকআউট লোড হচ্ছে…' },
  step_review: { en: 'Review', bn: 'পর্যালোচনা' },
  step_pay: { en: 'Pay', bn: 'পেমেন্ট' },
  step_done: { en: 'Confirmed', bn: 'নিশ্চিত হয়েছে' },
  co_check_status: { en: 'Check status', bn: 'অবস্থা দেখুন' },
  co_reference: { en: 'Reference', bn: 'রেফারেন্স' },
  co_unavailable: { en: 'Checkout unavailable', bn: 'চেকআউট পাওয়া যাচ্ছে না' },
  co_pay: { en: 'Pay', bn: 'পেমেন্ট' },
  co_amount_due: { en: 'Amount due', bn: 'প্রদেয় পরিমাণ' },
  co_method: { en: 'Payment method', bn: 'পেমেন্টের মাধ্যম' },
  co_expires: { en: 'Expires', bn: 'মেয়াদ শেষ' },
  co_how_to_pay: { en: 'How to pay', bn: 'কীভাবে পেমেন্ট করবেন' },
  co_wallet_number: { en: 'Your wallet number', bn: 'আপনার ওয়ালেট নম্বর' },
  co_trx_id: { en: 'Transaction ID', bn: 'লেনদেন আইডি' },
  co_submitted: { en: 'Payment submitted', bn: 'পেমেন্ট জমা দেওয়া হয়েছে' },
  co_awaiting: { en: 'Awaiting confirmation', bn: 'নিশ্চিতকরণের অপেক্ষায়' },
  co_complete: { en: 'Payment complete', bn: 'পেমেন্ট সম্পন্ন' },
  co_complete_body: {
    en: 'Payment complete. Thank you — the merchant has been notified.',
    bn: 'পেমেন্ট সম্পন্ন হয়েছে। ধন্যবাদ — বিক্রেতাকে জানানো হয়েছে।',
  },
  co_status: { en: 'Payment status', bn: 'পেমেন্টের অবস্থা' },

  // Dashboard navigation (Wave 133).
  nav_overview: { en: 'Overview', bn: 'ওভারভিউ' },
  nav_transactions: { en: 'Transactions', bn: 'লেনদেন' },
  nav_payment_links: { en: 'Payment Links', bn: 'পেমেন্ট লিংক' },
  nav_invoices: { en: 'Invoices', bn: 'ইনভয়েস' },
  nav_gateways: { en: 'Gateways', bn: 'গেটওয়ে' },
  nav_webhooks: { en: 'Webhooks', bn: 'ওয়েবহুক' },
  nav_api_keys: { en: 'API Keys', bn: 'API কী' },
  nav_settings: { en: 'Settings', bn: 'সেটিংস' },
  nav_admin: { en: 'Admin', bn: 'অ্যাডমিন' },

  // Auth pages (Wave 133).
  auth_login_heading: { en: 'Log in to MiraclePay', bn: 'MiraclePay-তে লগ ইন করুন' },
  auth_register_heading: { en: 'Create your MiraclePay account', bn: 'আপনার MiraclePay অ্যাকাউন্ট তৈরি করুন' },
  auth_name: { en: 'Name', bn: 'নাম' },
  auth_email: { en: 'Email', bn: 'ইমেইল' },
  auth_password: { en: 'Password', bn: 'পাসওয়ার্ড' },
  auth_totp: { en: 'Two-factor code', bn: 'দ্বি-ধাপ কোড' },
  auth_login: { en: 'Log in', bn: 'লগ ইন' },
  auth_register: { en: 'Create account', bn: 'অ্যাকাউন্ট তৈরি করুন' },
};

const STORAGE_KEY = 'mp-lang';

export function getLang(): Lang {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === 'bn' ? 'bn' : 'en';
  } catch {
    return 'en';
  }
}

export function setLang(lang: Lang): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    // Storage unavailable (private mode): language applies to this
    // page view only.
  }
  applyI18n(lang);
}

/** Translate `key` in `lang` (default: stored choice), substituting
 * `{name}` placeholders from `params`. Unknown keys fall back to
 * the key itself so a missing string is visible, never silent. */
export function t(key: string, params: Record<string, string> = {}, lang: Lang = getLang()): string {
  const entry = STRINGS[key];
  if (!entry) return key;
  let text = entry[lang] ?? entry.en;
  for (const [name, value] of Object.entries(params)) {
    text = text.replaceAll(`{${name}}`, value);
  }
  return text;
}

/** Apply `lang` to every `[data-i18n]` element on the page and sync
 * the document language. Returns the applied language. */
export function applyI18n(lang: Lang = getLang()): Lang {
  document.documentElement.lang = lang === 'bn' ? 'bn' : 'en';
  document.querySelectorAll<HTMLElement>('[data-i18n]').forEach((el) => {
    const key = el.dataset.i18n;
    if (key) el.textContent = t(key, {}, lang);
  });
  return lang;
}

/** The other language, for the toggle button label/behaviour. */
export function otherLang(lang: Lang): Lang {
  return lang === 'bn' ? 'en' : 'bn';
}

/** Label shown on the toggle: the language you switch TO. */
export function toggleLabel(lang: Lang): string {
  return lang === 'bn' ? 'English' : 'বাংলা';
}
