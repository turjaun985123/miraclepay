//! Startup configuration validation (Wave 93).
//!
//! Misconfiguration should fail loudly at boot in production and
//! warn in development — never surface as a surprise 500 later.
//! [`validate_env`] is a pure function over a variable lookup so
//! the rules are unit-tested without touching the real env.

/// Validate the process configuration. Returns a list of problems
/// (empty = valid). `production` is `APP_ENV=production`.
pub fn validate_env(get: impl Fn(&str) -> Option<String>, production: bool) -> Vec<String> {
    let mut problems = Vec::new();
    if let Some(port) = get("PORT") {
        if port.parse::<u16>().is_err() {
            problems.push(format!("PORT: {port:?} is not a valid port number"));
        }
    }
    match get("JWT_SECRET") {
        None => {
            if production {
                problems.push(
                    "JWT_SECRET: must be set in production (the dev fallback is public knowledge)"
                        .to_string(),
                );
            }
        }
        Some(secret) => {
            if secret == "dev-secret-change-me" && production {
                problems.push(
                    "JWT_SECRET: the development fallback must not be used in production"
                        .to_string(),
                );
            } else if secret.len() < 32 && production {
                problems.push("JWT_SECRET: shorter than 32 characters in production".to_string());
            }
        }
    }
    if let Some(url) = get("DATABASE_URL") {
        if !url.trim().is_empty()
            && !url.starts_with("postgres://")
            && !url.starts_with("postgresql://")
        {
            problems.push("DATABASE_URL: must start with postgres:// or postgresql://".to_string());
        }
    }
    let smtp_host = get("SMTP_HOST").is_some_and(|v| !v.trim().is_empty());
    let smtp_from = get("SMTP_FROM").is_some_and(|v| !v.trim().is_empty());
    if smtp_host != smtp_from {
        problems.push(
            "SMTP: SMTP_HOST and SMTP_FROM must be set together (email is disabled otherwise)"
                .to_string(),
        );
    }
    if let Some(secs) = get("PERSIST_SNAPSHOT_SECS") {
        if secs.parse::<u64>().is_err() {
            problems.push(format!(
                "PERSIST_SNAPSHOT_SECS: {secs:?} is not a non-negative integer"
            ));
        }
    }
    if let (Some(_), None) = (get("PAYPAL_WEBHOOK_ID"), get("PAYPAL_CLIENT_ID")) {
        problems.push(
            "PAYPAL_WEBHOOK_ID is set but PAYPAL_CLIENT_ID is not — the PayPal webhook receiver will answer 503"
                .to_string(),
        );
    }
    problems
}

/// Whether the process is running with `APP_ENV=production`.
pub fn is_production() -> bool {
    std::env::var("APP_ENV").is_ok_and(|v| v.eq_ignore_ascii_case("production"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn empty_env_is_valid_in_dev_but_not_production() {
        assert!(validate_env(lookup(&[]), false).is_empty());
        let problems = validate_env(lookup(&[]), true);
        assert!(problems.iter().any(|p| p.contains("JWT_SECRET")));
    }

    #[test]
    fn bad_values_are_named() {
        let problems = validate_env(
            lookup(&[
                ("PORT", "abc"),
                ("DATABASE_URL", "mysql://x"),
                ("SMTP_HOST", "mail.local"),
                ("PERSIST_SNAPSHOT_SECS", "-1"),
            ]),
            false,
        );
        assert_eq!(problems.len(), 4, "{problems:?}");
    }

    #[test]
    fn production_ready_config_passes() {
        let problems = validate_env(
            lookup(&[
                ("JWT_SECRET", "a-very-long-and-secret-value-for-production"),
                ("DATABASE_URL", "postgres://u:p@db:5432/miraclepay"),
                ("PORT", "8080"),
            ]),
            true,
        );
        assert!(problems.is_empty(), "{problems:?}");
    }
}
