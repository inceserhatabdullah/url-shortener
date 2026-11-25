use url::Url;

use crate::error::{AppError, AppResult};

pub const MAX_URL_LENGTH: usize = 2048;
pub const MIN_ALIAS_LENGTH: usize = 3;
pub const MAX_ALIAS_LENGTH: usize = 32;
// router içerisindeki sabit rotalar ile çakışmaması için alias olarak kullanılmayan yollar
pub const RESERVED_ALIASES: &[&str] = &["api", "health"];

// hedef url doğrular ve normalize edilmiş halini gösterir
pub fn validate_url(raw: &str, base_url: &str) -> AppResult<String> {
    let raw = raw.trim();

    if raw.is_empty() {
        return Err(AppError::Validation("url must not be empty".into()));
    }

    if raw.len() > MAX_URL_LENGTH {
        return Err(AppError::Validation(format!(
            "url must be at most {MAX_URL_LENGTH} characters"
        )));
    }

    let url = Url::parse(raw).map_err(|e| AppError::Validation(format!("invalid url: {e}")))?;

    // http/https; javascript;, ftp;, file:, vb. reddedilir
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::Validation(
            "url scheme must be http or https".into(),
        ));
    }

    if url.host_str().is_none() {
        return Err(AppError::Validation("url must contain a host".into()));
    }

    if points_to_self(&url, base_url)? {
        return Err(AppError::Validation(
            "url must not point to this service".into(),
        ));
    }

    Ok(url.to_string())
}

fn points_to_self(target: &Url, base_url: &str) -> AppResult<bool> {
    // config base_url zaten doğruluyor; buraya düşmesi bir programlama hatası
    let base = Url::parse(base_url)
        .map_err(|e| AppError::Internal(format!("Invalid BASE_URL in config: {e}")))?;

    if target.host_str() != base.host_str() {
        return Ok(false);
    }

    // base_url de açık port yok ise (80/443) sadece host karşılaştırılır; açık port var ise (örn: localhost:3000) port da eşleşmeli
    match base.port() {
        None => Ok(true),
        Some(port) => Ok(target.port_or_known_default() == Some(port)),
    }
}

pub fn validate_alias(alias: &str) -> AppResult<()> {
    let len = alias.chars().count();

    if !(MIN_ALIAS_LENGTH..=MAX_ALIAS_LENGTH).contains(&len) {
        return Err(AppError::Validation(format!(
            "alias length must be between {MIN_ALIAS_LENGTH} and {MAX_ALIAS_LENGTH}"
        )));
    }

    if !alias
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(
            "alias may only contain letters, digits, '-' and '_'".into(),
        ));
    }

    if RESERVED_ALIASES
        .iter()
        .any(|r| r.eq_ignore_ascii_case(alias))
    {
        return Err(AppError::Validation(format!("alias '{alias}' is reserved")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://localhost:3000";

    fn is_validation(r: AppResult<impl std::fmt::Debug>) -> bool {
        matches!(r, Err(AppError::Validation(_)))
    }

    // ---- URL ----

    #[test]
    fn accepts_valid_http_and_https() {
        assert!(validate_url("http://example.com/a", BASE).is_ok());
        assert!(validate_url("https://example.com/a?b=1#c", BASE).is_ok());
    }

    #[test]
    fn normalizes_and_trims() {
        let out = validate_url("  HTTPS://Example.COM  ", BASE).unwrap();
        assert_eq!(out, "https://example.com/");
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert!(is_validation(validate_url("", BASE)));
        assert!(is_validation(validate_url("   ", BASE)));
    }

    #[test]
    fn rejects_too_long_url() {
        let long = format!("https://example.com/{}", "a".repeat(MAX_URL_LENGTH));
        assert!(is_validation(validate_url(&long, BASE)));
    }

    #[test]
    fn rejects_unparsable_url() {
        assert!(is_validation(validate_url("not a url", BASE)));
        assert!(is_validation(validate_url("example.com", BASE)));
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(is_validation(validate_url("ftp://example.com/f", BASE)));
        assert!(is_validation(validate_url("javascript:alert(1)", BASE)));
        assert!(is_validation(validate_url("file:///etc/passwd", BASE)));
    }

    #[test]
    fn rejects_url_pointing_to_self_with_explicit_port() {
        assert!(is_validation(validate_url(
            "http://localhost:3000/abc",
            BASE
        )));
        assert!(is_validation(validate_url(
            "HTTP://LOCALHOST:3000/abc",
            BASE
        )));
        // Farklı port farklı servis sayılır.
        assert!(validate_url("http://localhost:4000/abc", BASE).is_ok());
    }

    #[test]
    fn rejects_self_url_on_default_port_for_both_schemes() {
        let base = "https://sho.rt";
        assert!(is_validation(validate_url("https://sho.rt/abc", base)));
        assert!(is_validation(validate_url("http://sho.rt/abc", base)));
        assert!(validate_url("https://other.com/abc", base).is_ok());
    }

    #[test]
    fn invalid_base_url_is_internal_error() {
        let r = validate_url("https://example.com", "not a url");
        assert!(matches!(r, Err(AppError::Internal(_))));
    }

    // ---- Alias ----

    #[test]
    fn accepts_valid_aliases() {
        for a in ["abc", "my-link", "my_link", "Link123", &"a".repeat(32)] {
            assert!(validate_alias(a).is_ok(), "{a} should be valid");
        }
    }

    #[test]
    fn rejects_alias_with_bad_length() {
        assert!(is_validation(validate_alias("ab")));
        assert!(is_validation(validate_alias(&"a".repeat(33))));
        assert!(is_validation(validate_alias("")));
    }

    #[test]
    fn rejects_alias_with_bad_characters() {
        for a in ["my link", "a/b", "a.b", "çay-1", "a?b", "a#b"] {
            assert!(is_validation(validate_alias(a)), "{a} should be rejected");
        }
    }

    #[test]
    fn rejects_reserved_aliases_case_insensitively() {
        assert!(is_validation(validate_alias("api")));
        assert!(is_validation(validate_alias("HEALTH")));
        assert!(is_validation(validate_alias("Api")));
    }
}
