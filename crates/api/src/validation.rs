//! Input validation helpers for MiraclePay v0.0.1.
//!
//! Pure, dependency-free (std only) validation and sanitisation helpers
//! used by the API layer. Functions are intentionally small and
//! single-purpose so they can be reused by any handler.
//!
//! Conventions:
//! - Functions returning `Result<(), String>` produce a human-readable,
//!   field-oriented error message on failure.
//! - Functions returning `bool` are predicates suitable for quick checks
//!   where a detailed error message is not needed.

// ---------------------------------------------------------------------------
// Email
// ---------------------------------------------------------------------------

/// Validate an email address.
///
/// Rules (deliberately simple and predictable, not a full RFC 5322 parser):
/// - surrounding whitespace is trimmed before checking;
/// - the value must not be empty after trimming;
/// - the value must contain no spaces (or other whitespace) anywhere;
/// - there must be exactly one `@`;
/// - the local part (before `@`) must be non-empty;
/// - the domain part (after `@`) must be non-empty and contain at least
///   one `.`, with non-empty labels on both sides of the last dot.
pub fn validate_email(value: &str) -> Result<(), String> {
    let email = value.trim();

    if email.is_empty() {
        return Err("email is required".to_string());
    }

    if email.chars().any(char::is_whitespace) {
        return Err("email must not contain spaces".to_string());
    }

    // Exactly one '@'.
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = match parts.next() {
        Some(domain) => domain,
        None => return Err("email must contain a single @".to_string()),
    };
    if parts.next().is_some() {
        return Err("email must contain a single @".to_string());
    }

    if local.is_empty() {
        return Err("email local part must not be empty".to_string());
    }

    if domain.is_empty() {
        return Err("email domain must not be empty".to_string());
    }

    // Domain must contain a dot with non-empty labels around it.
    if !domain.contains('.') {
        return Err("email domain must contain a dot".to_string());
    }
    if domain.starts_with('.') || domain.ends_with('.') {
        return Err("email domain must not start or end with a dot".to_string());
    }
    if domain.contains("..") {
        return Err("email domain must not contain consecutive dots".to_string());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Phone numbers
// ---------------------------------------------------------------------------

/// Check whether `value` is a valid Bangladeshi mobile number.
///
/// Accepted shapes (spaces are NOT accepted; callers should trim first
/// if they want to be lenient — this helper trims surrounding
/// whitespace itself for convenience):
/// - Local form: 11 digits, `01[3-9]` followed by 8 digits,
///   e.g. `01712345678`.
/// - International form: `+880` followed by the 10-digit national
///   number (`1[3-9]` + 8 digits), e.g. `+8801712345678`.
/// - Without the plus: `880` followed by the 10-digit national number,
///   e.g. `8801712345678`.
pub fn validate_phone_bd(value: &str) -> bool {
    let phone = value.trim();

    // Normalise to the 11-digit local form ("01XXXXXXXXX").
    let local: &str = if let Some(rest) = phone.strip_prefix("+880") {
        // rest must be "1[3-9]XXXXXXXX" (10 digits); local form adds '0'.
        if rest.len() != 10 || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        // Validate the national significant number directly.
        return is_bd_national_number(rest);
    } else if let Some(rest) = phone.strip_prefix("880") {
        if rest.len() != 10 || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        return is_bd_national_number(rest);
    } else {
        phone
    };

    if local.len() != 11 || !local.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }

    // Local form: 01[3-9] + 8 digits.
    let bytes = local.as_bytes();
    bytes[0] == b'0' && bytes[1] == b'1' && (b'3'..=b'9').contains(&bytes[2])
}

/// The 10-digit national significant number for a BD mobile:
/// `1[3-9]` followed by 8 digits.
fn is_bd_national_number(national: &str) -> bool {
    if national.len() != 10 || !national.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let bytes = national.as_bytes();
    bytes[0] == b'1' && (b'3'..=b'9').contains(&bytes[1])
}

/// Check whether `value` is a plausible international phone number.
///
/// Shape: a leading `+` followed by 7 to 15 digits (the E.164 maximum
/// is 15 digits). No spaces, dashes, or other separators are accepted.
pub fn validate_phone_international(value: &str) -> bool {
    let phone = value.trim();

    let digits = match phone.strip_prefix('+') {
        Some(digits) => digits,
        None => return false,
    };

    (7..=15).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Generic field helpers
// ---------------------------------------------------------------------------

/// Require that `value` is non-empty after trimming.
///
/// On failure the error message is exactly `"{field} is required"`.
pub fn require_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} is required"))
    } else {
        Ok(())
    }
}

/// Require that an amount (in minor units, e.g. poisha / cents) is
/// strictly positive.
pub fn validate_positive_amount(field: &str, amount_minor: i64) -> Result<(), String> {
    if amount_minor <= 0 {
        Err(format!("{field} must be a positive amount"))
    } else {
        Ok(())
    }
}

/// Require that `value` is at most `max` characters long.
///
/// Length is measured in Unicode scalar values (`chars`), not bytes,
/// so multi-byte characters (e.g. Bangla text) count as one each.
pub fn validate_max_len(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.chars().count() > max {
        Err(format!("{field} must be at most {max} characters"))
    } else {
        Ok(())
    }
}

/// Trim surrounding whitespace and truncate to at most `max`
/// characters.
///
/// Truncation is by `chars`, never by bytes, so the result is always
/// valid UTF-8 and never splits a multi-byte character.
pub fn sanitize_trim(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

// ---------------------------------------------------------------------------
// URL
// ---------------------------------------------------------------------------

/// Validate a URL.
///
/// Rules:
/// - surrounding whitespace is trimmed before checking;
/// - the value must start with `http://` or `https://`;
/// - the host part (everything up to the first `/`, `?`, `#`, or `:`
///   port separator after the scheme) must be non-empty;
/// - the value must contain no spaces.
pub fn validate_url(value: &str) -> Result<(), String> {
    let url = value.trim();

    if url.is_empty() {
        return Err("url is required".to_string());
    }

    if url.chars().any(char::is_whitespace) {
        return Err("url must not contain spaces".to_string());
    }

    let rest = if let Some(rest) = url.strip_prefix("http://") {
        rest
    } else if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else {
        return Err("url must start with http:// or https://".to_string());
    };

    // Host is the segment before the first path/query/fragment delimiter
    // or port colon.
    let host_end = rest
        .find(|c| c == '/' || c == '?' || c == '#' || c == ':')
        .unwrap_or(rest.len());
    let host = &rest[..host_end];

    if host.is_empty() {
        return Err("url must have a non-empty host".to_string());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_email ----------------------------------------------------

    #[test]
    fn email_valid_examples() {
        assert!(validate_email("user@example.com").is_ok());
        assert!(validate_email("first.last+tag@sub.domain.co").is_ok());
        assert!(validate_email("a@b.cd").is_ok());
    }

    #[test]
    fn email_trims_surrounding_whitespace() {
        assert!(validate_email("  user@example.com  ").is_ok());
        assert!(validate_email("\tuser@example.com\n").is_ok());
    }

    #[test]
    fn email_rejects_empty_and_blank() {
        assert!(validate_email("").is_err());
        assert!(validate_email("   ").is_err());
    }

    #[test]
    fn email_rejects_missing_at() {
        assert!(validate_email("userexample.com").is_err());
        assert!(validate_email("user.example.com").is_err());
    }

    #[test]
    fn email_rejects_multiple_at() {
        assert!(validate_email("a@b@example.com").is_err());
        assert!(validate_email("a@@example.com").is_err());
    }

    #[test]
    fn email_rejects_empty_local_part() {
        assert!(validate_email("@example.com").is_err());
    }

    #[test]
    fn email_rejects_empty_domain() {
        assert!(validate_email("user@").is_err());
    }

    #[test]
    fn email_rejects_domain_without_dot() {
        assert!(validate_email("user@localhost").is_err());
        assert!(validate_email("user@example").is_err());
    }

    #[test]
    fn email_rejects_domain_with_edge_or_double_dots() {
        assert!(validate_email("user@.example.com").is_err());
        assert!(validate_email("user@example.com.").is_err());
        assert!(validate_email("user@example..com").is_err());
    }

    #[test]
    fn email_rejects_spaces_inside() {
        assert!(validate_email("user name@example.com").is_err());
        assert!(validate_email("user@exam ple.com").is_err());
        assert!(validate_email("user @example.com").is_err());
    }

    // -- validate_phone_bd ---------------------------------------------------

    #[test]
    fn phone_bd_valid_local_numbers_for_all_operator_prefixes() {
        // 013 through 019 are the allocated mobile prefixes.
        for prefix in ["013", "014", "015", "016", "017", "018", "019"] {
            let number = format!("{prefix}12345678");
            assert!(validate_phone_bd(&number), "expected {number} to be valid");
        }
        assert!(validate_phone_bd("01712345678"));
    }

    #[test]
    fn phone_bd_rejects_invalid_prefix_digit() {
        // Third digit must be 3-9.
        assert!(!validate_phone_bd("01212345678"));
        assert!(!validate_phone_bd("01012345678"));
        assert!(!validate_phone_bd("01112345678"));
        assert!(!validate_phone_bd("02712345678")); // does not start with 01
    }

    #[test]
    fn phone_bd_valid_with_country_code() {
        assert!(validate_phone_bd("+8801712345678"));
        assert!(validate_phone_bd("8801712345678"));
        assert!(validate_phone_bd("+8801312345678"));
        assert!(validate_phone_bd("8801912345678"));
    }

    #[test]
    fn phone_bd_rejects_invalid_with_country_code() {
        assert!(!validate_phone_bd("+8801212345678")); // 2 not in 3-9
        assert!(!validate_phone_bd("8801012345678")); // 0 not in 3-9
        assert!(!validate_phone_bd("+880171234567")); // too short
        assert!(!validate_phone_bd("+88017123456789")); // too long
    }

    #[test]
    fn phone_bd_rejects_wrong_lengths() {
        assert!(!validate_phone_bd("0171234567")); // 10 digits
        assert!(!validate_phone_bd("017123456789")); // 12 digits
        assert!(!validate_phone_bd(""));
        assert!(!validate_phone_bd("01712345"));
    }

    #[test]
    fn phone_bd_rejects_non_digits_and_separators() {
        assert!(!validate_phone_bd("0171234567a"));
        assert!(!validate_phone_bd("017-1234-5678"));
        assert!(!validate_phone_bd("017 1234 5678"));
        assert!(!validate_phone_bd("+880 1712345678"));
    }

    #[test]
    fn phone_bd_trims_surrounding_whitespace() {
        assert!(validate_phone_bd("  01712345678  "));
        assert!(validate_phone_bd(" +8801712345678 "));
    }

    // -- validate_phone_international ----------------------------------------

    #[test]
    fn phone_international_valid_examples() {
        assert!(validate_phone_international("+8801712345678"));
        assert!(validate_phone_international("+14155552671"));
        assert!(validate_phone_international("+442071838750"));
        assert!(validate_phone_international("+1234567")); // 7 digits: minimum
        assert!(validate_phone_international("+123456789012345")); // 15: maximum
    }

    #[test]
    fn phone_international_rejects_bad_shapes() {
        assert!(!validate_phone_international("01712345678")); // no '+'
        assert!(!validate_phone_international("+123456")); // 6 digits: too short
        assert!(!validate_phone_international("+1234567890123456")); // 16: too long
        assert!(!validate_phone_international("+"));
        assert!(!validate_phone_international(""));
        assert!(!validate_phone_international("++8801712345678"));
        assert!(!validate_phone_international("+880 1712345678"));
        assert!(!validate_phone_international("+880-1712-345678"));
        assert!(!validate_phone_international("+88017123456a"));
    }

    // -- require_non_empty -----------------------------------------------------

    #[test]
    fn require_non_empty_accepts_content() {
        assert!(require_non_empty("name", "Rahim").is_ok());
        assert!(require_non_empty("name", "  Rahim  ").is_ok());
    }

    #[test]
    fn require_non_empty_rejects_empty_and_blank_with_field_message() {
        assert_eq!(
            require_non_empty("name", ""),
            Err("name is required".to_string())
        );
        assert_eq!(
            require_non_empty("phone", "   "),
            Err("phone is required".to_string())
        );
        assert_eq!(
            require_non_empty("address", "\t\n"),
            Err("address is required".to_string())
        );
    }

    // -- validate_positive_amount ----------------------------------------------

    #[test]
    fn positive_amount_accepts_positive_values() {
        assert!(validate_positive_amount("amount", 1).is_ok());
        assert!(validate_positive_amount("amount", 100).is_ok()); // 1.00 in minor units
        assert!(validate_positive_amount("amount", 469_000).is_ok());
        assert!(validate_positive_amount("amount", i64::MAX).is_ok());
    }

    #[test]
    fn positive_amount_rejects_zero_and_negative() {
        assert!(validate_positive_amount("amount", 0).is_err());
        assert!(validate_positive_amount("amount", -1).is_err());
        assert!(validate_positive_amount("amount", -50_000).is_err());
        assert!(validate_positive_amount("amount", i64::MIN).is_err());
    }

    #[test]
    fn positive_amount_error_names_the_field() {
        let err = validate_positive_amount("price", 0).unwrap_err();
        assert!(err.contains("price"), "error should name the field: {err}");
    }

    // -- validate_max_len --------------------------------------------------------

    #[test]
    fn max_len_accepts_within_limit() {
        assert!(validate_max_len("name", "abc", 3).is_ok());
        assert!(validate_max_len("name", "ab", 3).is_ok());
        assert!(validate_max_len("name", "", 3).is_ok());
        assert!(validate_max_len("name", "", 0).is_ok());
    }

    #[test]
    fn max_len_rejects_over_limit() {
        assert!(validate_max_len("name", "abcd", 3).is_err());
        assert!(validate_max_len("name", "a", 0).is_err());
    }

    #[test]
    fn max_len_counts_chars_not_bytes() {
        // Each Bangla character below is multi-byte in UTF-8 but one char.
        let value = "ঢাকা";
        assert_eq!(value.chars().count(), 4);
        assert!(validate_max_len("city", value, 4).is_ok());
        assert!(validate_max_len("city", value, 3).is_err());
    }

    #[test]
    fn max_len_error_names_field_and_limit() {
        let err = validate_max_len("title", "abcdef", 5).unwrap_err();
        assert!(err.contains("title"), "error should name the field: {err}");
        assert!(err.contains('5'), "error should name the limit: {err}");
    }

    // -- sanitize_trim -----------------------------------------------------------

    #[test]
    fn sanitize_trim_trims_whitespace() {
        assert_eq!(sanitize_trim("  hello  ", 100), "hello");
        assert_eq!(sanitize_trim("\tworld\n", 100), "world");
        assert_eq!(sanitize_trim("   ", 100), "");
        assert_eq!(sanitize_trim("", 100), "");
    }

    #[test]
    fn sanitize_trim_truncates_to_max_chars() {
        assert_eq!(sanitize_trim("hello world", 5), "hello");
        assert_eq!(sanitize_trim("hello", 5), "hello");
        assert_eq!(sanitize_trim("hello", 10), "hello");
        assert_eq!(sanitize_trim("hello", 0), "");
    }

    #[test]
    fn sanitize_trim_trims_before_truncating() {
        assert_eq!(sanitize_trim("   hello world   ", 5), "hello");
    }

    #[test]
    fn sanitize_trim_truncates_by_chars_not_bytes() {
        let value = "ঢাকাবাংলা";
        let out = sanitize_trim(value, 4);
        assert_eq!(out.chars().count(), 4);
        assert_eq!(out, "ঢাকা");
    }

    // -- validate_url --------------------------------------------------------------

    #[test]
    fn url_valid_examples() {
        assert!(validate_url("http://example.com").is_ok());
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("https://example.com/path/to/page").is_ok());
        assert!(validate_url("https://example.com:8443/path").is_ok());
        assert!(validate_url("https://sub.example.com/path?q=1#frag").is_ok());
        assert!(validate_url("  https://example.com  ").is_ok());
    }

    #[test]
    fn url_rejects_missing_or_wrong_scheme() {
        assert!(validate_url("example.com").is_err());
        assert!(validate_url("www.example.com").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("//example.com").is_err());
    }

    #[test]
    fn url_rejects_empty_host() {
        assert!(validate_url("http://").is_err());
        assert!(validate_url("https://").is_err());
        assert!(validate_url("https:///path").is_err());
        assert!(validate_url("https://?q=1").is_err());
        assert!(validate_url("https://:8080/path").is_err());
    }

    #[test]
    fn url_rejects_empty_and_spaces() {
        assert!(validate_url("").is_err());
        assert!(validate_url("   ").is_err());
        assert!(validate_url("https://exam ple.com").is_err());
        assert!(validate_url("https://example.com/a b").is_err());
    }
}
