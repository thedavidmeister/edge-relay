//! Lovense cloud-command construction.
//!
//! Commands are sent to the cloud (server) endpoint, which works over the
//! internet — the toy does not need to share a network with the controller:
//!
//! ```text
//! POST https://api.lovense.com/api/lan/command
//! { "token", "uid", "command":"Function", "action":"Vibrate:16", "timeSec":20, "apiVer":1 }
//! ```
//!
//! See <https://github.com/lovense/Standard_solutions>.

use serde::Serialize;

/// Maximum vibration strength accepted by Lovense (range is `0..=20`).
pub const MAX_STRENGTH: u8 = 20;

/// A single `Function` command, independent of credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// e.g. `"Vibrate:15"` or `"Stop"`.
    action: String,
    /// Total run time in seconds; `0` means run until stopped.
    time_sec: u32,
}

impl Command {
    /// Vibrate at `strength` (clamped to `0..=20`) for `time_sec` seconds
    /// (`0` = until stopped).
    pub fn vibrate(strength: u8, time_sec: u32) -> Self {
        let strength = strength.min(MAX_STRENGTH);
        Command {
            action: format!("Vibrate:{strength}"),
            time_sec,
        }
    }

    /// Stop all motors immediately.
    pub fn stop() -> Self {
        Command {
            action: "Stop".to_string(),
            time_sec: 0,
        }
    }

    /// The Lovense action string, e.g. `"Vibrate:10"`.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Total run time in seconds (`0` = until stopped).
    pub fn time_sec(&self) -> u32 {
        self.time_sec
    }

    /// Build the full server-command body for the cloud endpoint, injecting the
    /// developer `token` and the `uid` chosen at pairing time.
    pub fn to_server_body(&self, token: &str, uid: &str) -> ServerBody {
        ServerBody {
            token: token.to_string(),
            uid: uid.to_string(),
            command: "Function",
            action: self.action.clone(),
            time_sec: self.time_sec,
            api_ver: 1,
        }
    }
}

/// Serializable body for `POST /api/lan/command`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServerBody {
    pub token: String,
    pub uid: String,
    pub command: &'static str,
    pub action: String,
    #[serde(rename = "timeSec")]
    pub time_sec: u32,
    #[serde(rename = "apiVer")]
    pub api_ver: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn vibrate_formats_action() {
        assert_eq!(Command::vibrate(15, 30).action(), "Vibrate:15");
    }

    #[test]
    fn vibrate_clamps_above_max() {
        assert_eq!(Command::vibrate(99, 0).action(), "Vibrate:20");
        assert_eq!(Command::vibrate(21, 0).action(), "Vibrate:20");
        assert_eq!(Command::vibrate(u8::MAX, 0).action(), "Vibrate:20");
    }

    #[test]
    fn vibrate_at_max_is_unchanged() {
        assert_eq!(Command::vibrate(20, 0).action(), "Vibrate:20");
    }

    #[test]
    fn vibrate_zero_is_valid() {
        assert_eq!(Command::vibrate(0, 0).action(), "Vibrate:0");
    }

    #[test]
    fn vibrate_keeps_time() {
        assert_eq!(Command::vibrate(5, 42).time_sec(), 42);
        assert_eq!(Command::vibrate(5, 0).time_sec(), 0);
    }

    #[test]
    fn stop_action_and_time() {
        let s = Command::stop();
        assert_eq!(s.action(), "Stop");
        assert_eq!(s.time_sec(), 0);
    }

    #[test]
    fn commands_compare_by_value() {
        assert_eq!(Command::vibrate(3, 1), Command::vibrate(3, 1));
        assert_ne!(Command::vibrate(3, 1), Command::vibrate(4, 1));
        assert_ne!(Command::vibrate(3, 1), Command::vibrate(3, 2));
        assert_ne!(Command::vibrate(0, 0), Command::stop());
    }

    #[test]
    fn server_body_has_expected_shape() {
        let body = Command::vibrate(10, 20).to_server_body("TOK", "gf");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            json!({
                "token": "TOK",
                "uid": "gf",
                "command": "Function",
                "action": "Vibrate:10",
                "timeSec": 20,
                "apiVer": 1,
            })
        );
    }

    #[test]
    fn server_body_renames_camel_case_fields() {
        let v = serde_json::to_value(Command::stop().to_server_body("T", "u")).unwrap();
        // The wire format uses camelCase keys, not snake_case.
        assert!(v.get("timeSec").is_some());
        assert!(v.get("apiVer").is_some());
        assert!(v.get("time_sec").is_none());
        assert!(v.get("api_ver").is_none());
    }

    #[test]
    fn server_body_stop_payload() {
        let v = serde_json::to_value(Command::stop().to_server_body("T", "u")).unwrap();
        assert_eq!(v["action"], "Stop");
        assert_eq!(v["command"], "Function");
        assert_eq!(v["apiVer"], 1);
        assert_eq!(v["timeSec"], 0);
    }

    #[test]
    fn server_body_carries_credentials() {
        let v =
            serde_json::to_value(Command::vibrate(1, 1).to_server_body("secret", "gf")).unwrap();
        assert_eq!(v["token"], "secret");
        assert_eq!(v["uid"], "gf");
    }

    #[test]
    fn max_strength_constant_is_twenty() {
        assert_eq!(MAX_STRENGTH, 20);
    }
}
