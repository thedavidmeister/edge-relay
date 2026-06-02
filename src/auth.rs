//! Authorization: only one configured Telegram user may drive the toy.

/// Returns `true` if `user_id` is the configured `allowed_id`.
pub fn is_allowed(user_id: i64, allowed_id: i64) -> bool {
    user_id == allowed_id
}

/// Parse the `ALLOWED_USER_ID` value into a numeric Telegram id.
///
/// Surrounding whitespace is ignored; anything non-numeric yields `None`.
pub fn parse_allowed_id(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_matching_id() {
        assert!(is_allowed(123, 123));
        assert!(is_allowed(-1001, -1001));
        assert!(is_allowed(0, 0));
    }

    #[test]
    fn rejects_non_matching_id() {
        assert!(!is_allowed(123, 456));
        assert!(!is_allowed(0, 1));
        assert!(!is_allowed(-1, 1));
    }

    #[test]
    fn parses_valid_id() {
        assert_eq!(parse_allowed_id("789"), Some(789));
    }

    #[test]
    fn parses_id_with_surrounding_whitespace() {
        assert_eq!(parse_allowed_id("  789 \n"), Some(789));
    }

    #[test]
    fn parses_negative_channel_id() {
        assert_eq!(parse_allowed_id("-1001234567890"), Some(-1001234567890));
    }

    #[test]
    fn rejects_non_numeric() {
        assert_eq!(parse_allowed_id("abc"), None);
        assert_eq!(parse_allowed_id("12x"), None);
        assert_eq!(parse_allowed_id("1.5"), None);
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(parse_allowed_id(""), None);
        assert_eq!(parse_allowed_id("   "), None);
    }

    #[test]
    fn parsed_id_round_trips_through_is_allowed() {
        let allowed = parse_allowed_id("42").unwrap();
        assert!(is_allowed(42, allowed));
        assert!(!is_allowed(43, allowed));
    }

    #[test]
    fn rejects_id_that_overflows_i64() {
        // A number too large for i64 must be rejected, not wrap.
        assert_eq!(parse_allowed_id("99999999999999999999999"), None);
    }

    #[test]
    fn accepts_i64_bounds() {
        assert_eq!(parse_allowed_id(&i64::MAX.to_string()), Some(i64::MAX));
        assert_eq!(parse_allowed_id(&i64::MIN.to_string()), Some(i64::MIN));
    }
}
