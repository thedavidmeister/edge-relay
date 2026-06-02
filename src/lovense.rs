//! Lovense pairing: `getQrCode` request construction and the pairing callback.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default base URL for the Lovense cloud API. Overridable via the
/// `LOVENSE_API_BASE` worker var (used by integration tests to point the
/// worker at a stub server).
pub const DEFAULT_API_BASE: &str = "https://api.lovense.com";

/// The `getQrCode` endpoint for a given API base.
pub fn qr_url(base: &str) -> String {
    format!("{base}/api/lan/getQrCode")
}

/// The cloud command endpoint for a given API base.
pub fn command_url(base: &str) -> String {
    format!("{base}/api/lan/command")
}

/// Body for `POST /api/lan/getQrCode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QrRequest {
    pub token: String,
    pub uid: String,
    pub uname: String,
    pub utoken: String,
    pub v: u8,
}

impl QrRequest {
    /// Build a QR request for `uid`/`uname`, deriving `utoken` from `salt`.
    pub fn new(token: &str, uid: &str, uname: &str, salt: &str) -> Self {
        QrRequest {
            token: token.to_string(),
            uid: uid.to_string(),
            uname: uname.to_string(),
            utoken: utoken(uid, salt),
            v: 2,
        }
    }
}

/// `utoken = md5(uid + salt)` as lowercase hex, per the Lovense Standard API.
pub fn utoken(uid: &str, salt: &str) -> String {
    format!("{:x}", md5::compute(format!("{uid}{salt}")))
}

/// Payload Lovense POSTs to the developer callback after a user scans the QR.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Callback {
    pub uid: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(rename = "httpsPort", default)]
    pub https_port: Option<String>,
    #[serde(default)]
    pub toys: HashMap<String, Toy>,
}

/// A single toy entry within a [`Callback`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Toy {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub status: i32,
}

impl Callback {
    /// Toys currently online (`status == 1`).
    pub fn online_toys(&self) -> Vec<&Toy> {
        self.toys.values().filter(|t| t.status == 1).collect()
    }
}

/// Extract the QR image URL (`data.qr`) from a `getQrCode` response body.
///
/// Returns `None` if the body is not JSON, has no `data.qr`, or `qr` is not a
/// string — e.g. an error response such as `{"code":401,"message":"..."}`.
pub fn extract_qr_url(response_body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(response_body).ok()?;
    value
        .get("data")?
        .get("qr")?
        .as_str()
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utoken_is_md5_hex_of_uid_then_salt() {
        let expected = format!("{:x}", md5::compute("gfsalt"));
        assert_eq!(utoken("gf", "salt"), expected);
    }

    #[test]
    fn utoken_is_32_hex_chars() {
        let t = utoken("anything", "pepper");
        assert_eq!(t.len(), 32);
        assert!(t
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    #[test]
    fn utoken_matches_known_md5_vector() {
        // md5("abc") == 900150983cd24fb0d6963f7d28e17f72
        assert_eq!(utoken("a", "bc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn utoken_depends_on_full_concatenation() {
        // Changing either the uid or the salt changes the hash.
        assert_ne!(utoken("a", "b"), utoken("a", "c"));
        assert_ne!(utoken("a", "b"), utoken("c", "b"));
        // Order matters: uid+salt is not symmetric.
        assert_ne!(utoken("a", "b"), utoken("b", "a"));
        // But uid and salt are concatenated with no separator, so the split
        // point is not encoded: ("a","b") and ("ab","") both hash "ab".
        assert_eq!(utoken("a", "b"), utoken("ab", ""));
    }

    #[test]
    fn qr_request_has_expected_shape() {
        let r = QrRequest::new("TOK", "gf", "telegram", "salt");
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["token"], "TOK");
        assert_eq!(v["uid"], "gf");
        assert_eq!(v["uname"], "telegram");
        assert_eq!(v["v"], 2);
        assert_eq!(v["utoken"], utoken("gf", "salt"));
    }

    #[test]
    fn urls_are_built_from_the_base() {
        assert_eq!(
            qr_url(DEFAULT_API_BASE),
            "https://api.lovense.com/api/lan/getQrCode"
        );
        assert_eq!(
            command_url(DEFAULT_API_BASE),
            "https://api.lovense.com/api/lan/command"
        );
        // A test/stub base is honoured.
        assert_eq!(
            qr_url("http://127.0.0.1:8788"),
            "http://127.0.0.1:8788/api/lan/getQrCode"
        );
        assert_eq!(
            command_url("http://127.0.0.1:8788"),
            "http://127.0.0.1:8788/api/lan/command"
        );
    }

    #[test]
    fn parses_callback_with_online_toy() {
        let body = r#"{
            "uid": "gf",
            "appVersion": "4.0.3",
            "toys": { "abc": { "name": "max", "id": "abc", "status": 1 } },
            "domain": "192-168-1-44.lovense.club",
            "httpsPort": "34568",
            "platform": "android"
        }"#;
        let cb: Callback = serde_json::from_str(body).unwrap();
        assert_eq!(cb.uid, "gf");
        assert_eq!(cb.domain.as_deref(), Some("192-168-1-44.lovense.club"));
        assert_eq!(cb.https_port.as_deref(), Some("34568"));
        assert_eq!(cb.online_toys().len(), 1);
        assert_eq!(cb.online_toys()[0].name, "max");
        assert_eq!(cb.online_toys()[0].id, "abc");
    }

    #[test]
    fn online_toys_filters_offline() {
        let body = r#"{
            "uid": "x",
            "toys": {
                "a": { "name": "lush", "id": "a", "status": 0 },
                "b": { "name": "max", "id": "b", "status": 1 }
            }
        }"#;
        let cb: Callback = serde_json::from_str(body).unwrap();
        assert_eq!(cb.toys.len(), 2);
        let online = cb.online_toys();
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].id, "b");
    }

    #[test]
    fn online_toys_empty_when_all_offline() {
        let body = r#"{ "uid": "x", "toys": { "a": { "status": 0 } } }"#;
        let cb: Callback = serde_json::from_str(body).unwrap();
        assert!(cb.online_toys().is_empty());
    }

    #[test]
    fn callback_tolerates_missing_optional_fields() {
        let cb: Callback = serde_json::from_str(r#"{ "uid": "x" }"#).unwrap();
        assert_eq!(cb.uid, "x");
        assert!(cb.domain.is_none());
        assert!(cb.https_port.is_none());
        assert!(cb.toys.is_empty());
        assert!(cb.online_toys().is_empty());
    }

    #[test]
    fn toy_defaults_fill_missing_fields() {
        // A toy object with only a status still deserializes.
        let body = r#"{ "uid": "x", "toys": { "k": { "status": 1 } } }"#;
        let cb: Callback = serde_json::from_str(body).unwrap();
        let t = &cb.toys["k"];
        assert_eq!(t.name, "");
        assert_eq!(t.id, "");
        assert_eq!(t.status, 1);
    }

    #[test]
    fn callback_without_uid_fails() {
        // uid is required (no default), so this must error.
        let err = serde_json::from_str::<Callback>(r#"{ "toys": {} }"#);
        assert!(err.is_err());
    }

    #[test]
    fn extract_qr_url_from_success_response() {
        let body = r#"{
            "code": 0, "message": "Success", "result": true,
            "data": { "qr": "https://api.lovense.com/x.jpg", "code": "ab12" }
        }"#;
        assert_eq!(
            extract_qr_url(body).as_deref(),
            Some("https://api.lovense.com/x.jpg")
        );
    }

    #[test]
    fn extract_qr_url_none_when_data_missing() {
        // A typical error response carries no `data`.
        assert_eq!(
            extract_qr_url(r#"{ "code": 401, "message": "token invalid" }"#),
            None
        );
    }

    #[test]
    fn extract_qr_url_none_when_qr_missing() {
        assert_eq!(extract_qr_url(r#"{ "data": { "code": "ab12" } }"#), None);
    }

    #[test]
    fn extract_qr_url_none_when_qr_not_a_string() {
        assert_eq!(extract_qr_url(r#"{ "data": { "qr": 123 } }"#), None);
    }

    #[test]
    fn extract_qr_url_none_for_non_json() {
        assert_eq!(extract_qr_url("not json at all"), None);
        assert_eq!(extract_qr_url(""), None);
    }

    #[test]
    fn extract_qr_url_none_when_data_is_not_an_object() {
        // `data` present but a string, not an object — must not panic.
        assert_eq!(extract_qr_url(r#"{ "data": "oops" }"#), None);
    }

    #[test]
    fn online_toys_treats_unexpected_status_as_offline() {
        // Only status == 1 is online; other values (2, -1) are not.
        let body = r#"{ "uid": "x", "toys": {
            "a": { "status": 2 }, "b": { "status": -1 }, "c": { "status": 1 }
        } }"#;
        let cb: Callback = serde_json::from_str(body).unwrap();
        let online = cb.online_toys();
        assert_eq!(online.len(), 1);
        assert_eq!(online[0].status, 1);
    }
}
