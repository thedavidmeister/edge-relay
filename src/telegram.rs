//! Parsing of incoming Telegram updates into bot commands and dispatch
//! decisions.

use crate::command::{Command, MAX_STRENGTH};
use serde::Deserialize;

/// A parsed bot command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BotCommand {
    /// Vibrate at `strength` (already clamped to `0..=20`) for `time_sec`
    /// seconds (`0` = until stopped).
    Vibrate { strength: u8, time_sec: u32 },
    /// Stop all motors.
    Stop,
    /// Request a pairing QR code.
    Pair,
    /// Show help.
    Help,
    /// Report connection/toy status.
    Status,
}

/// Why a message could not be parsed into a [`BotCommand`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The text was empty or whitespace only.
    Empty,
    /// The text did not start with `/`.
    NotACommand,
    /// The command name is not recognised.
    UnknownCommand(String),
    /// `/vibrate` was given without a strength argument.
    MissingStrength,
    /// The strength argument was not a valid number.
    InvalidStrength(String),
    /// The duration argument was not a valid duration.
    InvalidDuration(String),
}

/// Default vibration duration when none is supplied (`0` = until `/stop`).
pub const DEFAULT_TIME_SEC: u32 = 0;

/// Help text listing the available commands.
pub const HELP: &str = "Commands:\n\
    /vibrate <0-20> [secs|30s|2m|1h]\n\
    /stop\n\
    /pair\n\
    /help";

/// Default base URL for the Telegram Bot API. Overridable via the
/// `TELEGRAM_API_BASE` worker var (used by integration tests to redirect the
/// worker's outbound replies at a stub server).
pub const DEFAULT_TELEGRAM_BASE: &str = "https://api.telegram.org";

/// The `sendMessage` endpoint for a given API base and bot token.
pub fn send_message_url(base: &str, token: &str) -> String {
    format!("{base}/bot{token}/sendMessage")
}

/// Parse raw Telegram message `text` into a [`BotCommand`].
///
/// Accepts a leading `/`, an optional `@botname` suffix, and is
/// case-insensitive on the command name. Examples: `/vibrate 15`,
/// `/v 10 30s`, `/stop@my_bot`, `/pair`.
pub fn parse(text: &str) -> Result<BotCommand, ParseError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ParseError::Empty);
    }

    let mut parts = text.split_whitespace();
    let raw = parts.next().ok_or(ParseError::Empty)?;
    if !raw.starts_with('/') {
        return Err(ParseError::NotACommand);
    }

    // Drop the leading '/' and any "@botname" suffix, then lowercase.
    let name = raw[1..].split('@').next().unwrap_or("").to_lowercase();

    match name.as_str() {
        "vibrate" | "v" => {
            let strength_str = parts.next().ok_or(ParseError::MissingStrength)?;
            let strength: u8 = strength_str
                .parse()
                .map_err(|_| ParseError::InvalidStrength(strength_str.to_string()))?;
            let strength = strength.min(MAX_STRENGTH);
            let time_sec = match parts.next() {
                Some(d) => parse_duration(d)?,
                None => DEFAULT_TIME_SEC,
            };
            Ok(BotCommand::Vibrate { strength, time_sec })
        }
        "stop" | "s" | "halt" => Ok(BotCommand::Stop),
        "pair" | "link" => Ok(BotCommand::Pair),
        "help" | "start" => Ok(BotCommand::Help),
        "status" => Ok(BotCommand::Status),
        other => Err(ParseError::UnknownCommand(other.to_string())),
    }
}

/// Parse a duration token into seconds.
///
/// Accepts a bare number (seconds) or a case-insensitive `s`/`m`/`h` suffix:
/// `30`, `30s`, `2M`, `1h`. Overflow (of the value or the unit multiply) is
/// reported as [`ParseError::InvalidDuration`].
pub fn parse_duration(s: &str) -> Result<u32, ParseError> {
    let s = s.trim();
    let invalid = || ParseError::InvalidDuration(s.to_string());
    if s.is_empty() {
        return Err(invalid());
    }

    // Match the unit suffix case-insensitively (30S behaves like 30s).
    let lower = s.to_ascii_lowercase();
    let (num, mult) = if let Some(n) = lower.strip_suffix('s') {
        (n, 1)
    } else if let Some(n) = lower.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = lower.strip_suffix('h') {
        (n, 3600)
    } else {
        (lower.as_str(), 1)
    };

    let value: u32 = num.parse().map_err(|_| invalid())?;
    value.checked_mul(mult).ok_or_else(invalid)
}

impl BotCommand {
    /// Map a command to the Lovense [`Command`] it triggers, if any. Meta
    /// commands (`Pair`, `Help`, `Status`) drive no toy and return `None`.
    pub fn to_lovense(&self) -> Option<Command> {
        match self {
            BotCommand::Vibrate { strength, time_sec } => {
                Some(Command::vibrate(*strength, *time_sec))
            }
            BotCommand::Stop => Some(Command::stop()),
            BotCommand::Pair | BotCommand::Help | BotCommand::Status => None,
        }
    }

    /// The reply text confirming this command, for commands whose reply is
    /// self-contained. `Pair` returns `None` because its reply needs the QR
    /// URL from a network call.
    pub fn ack(&self) -> Option<String> {
        Some(match self {
            BotCommand::Vibrate { strength, time_sec } => {
                if *time_sec == 0 {
                    // 0 means "run until stopped", not "0 seconds".
                    format!("Vibrating at {strength} until /stop.")
                } else {
                    format!("Vibrating at {strength} for {time_sec}s.")
                }
            }
            BotCommand::Stop => "Stopped.".to_string(),
            BotCommand::Help | BotCommand::Status => HELP.to_string(),
            BotCommand::Pair => return None,
        })
    }
}

/// A Telegram `Update` (only the fields we use).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Update {
    pub message: Option<Message>,
}

/// A Telegram message. Non-text messages (stickers, photos) carry no `text`,
/// which defaults to empty.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub text: String,
    pub from: Option<User>,
    pub chat: Chat,
}

/// The sender of a message.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub id: i64,
}

/// The chat a message belongs to (where replies go).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Chat {
    pub id: i64,
}

/// What the worker should do with an incoming update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    /// Do nothing and send no reply: no message, an unauthorized sender, or
    /// plain chatter that isn't a command.
    Ignore,
    /// A recognized command from the authorized user; run it and reply.
    Command { chat_id: i64, command: BotCommand },
    /// The authorized user sent a malformed command; reply with the reason.
    Invalid { chat_id: i64, error: ParseError },
}

/// Decide what to do with an `update`, given the single `allowed_id`.
///
/// Strangers and plain (non-command) chatter are ignored silently — that keeps
/// the bot discreet and avoids "couldn't parse" spam on normal messages. Only
/// the authorized user's actual command attempts produce a reply.
pub fn dispatch(update: &Update, allowed_id: i64) -> Dispatch {
    let Some(msg) = update.message.as_ref() else {
        return Dispatch::Ignore;
    };
    // A message with no `from` (e.g. a channel post) is never authorized — we
    // never fall back to a default id that could match a misconfigured 0.
    let Some(user) = msg.from.as_ref() else {
        return Dispatch::Ignore;
    };
    if !crate::auth::is_allowed(user.id, allowed_id) {
        return Dispatch::Ignore;
    }
    match parse(&msg.text) {
        Ok(command) => Dispatch::Command {
            chat_id: msg.chat.id,
            command,
        },
        // Plain text (not a slash command) is conversation, not an error.
        Err(ParseError::Empty | ParseError::NotACommand) => Dispatch::Ignore,
        Err(error) => Dispatch::Invalid {
            chat_id: msg.chat.id,
            error,
        },
    }
}

/// A friendly, user-facing message for a parse error (instead of the raw
/// `Debug` form). Total over [`ParseError`]; `dispatch` only surfaces the
/// command-attempt variants, but the others are handled for completeness.
pub fn invalid_reply(error: &ParseError) -> String {
    match error {
        ParseError::MissingStrength => "Usage: /vibrate <0-20> [30s|2m|1h]".to_string(),
        ParseError::InvalidStrength(s) => {
            format!("Strength must be a number 0-20 (got \"{s}\").")
        }
        ParseError::InvalidDuration(d) => {
            format!("Couldn't read the duration \"{d}\". Try 30s, 2m, or 1h.")
        }
        ParseError::UnknownCommand(c) => format!("Unknown command /{c}. Send /help."),
        ParseError::Empty | ParseError::NotACommand => "Send /help.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vibrate_with_strength() {
        assert_eq!(
            parse("/vibrate 15"),
            Ok(BotCommand::Vibrate {
                strength: 15,
                time_sec: 0
            })
        );
    }

    #[test]
    fn parses_vibrate_alias() {
        assert_eq!(
            parse("/v 7"),
            Ok(BotCommand::Vibrate {
                strength: 7,
                time_sec: 0
            })
        );
    }

    #[test]
    fn parses_vibrate_with_plain_seconds() {
        assert_eq!(
            parse("/vibrate 10 30"),
            Ok(BotCommand::Vibrate {
                strength: 10,
                time_sec: 30
            })
        );
    }

    #[test]
    fn parses_vibrate_with_second_suffix() {
        assert_eq!(
            parse("/vibrate 10 30s"),
            Ok(BotCommand::Vibrate {
                strength: 10,
                time_sec: 30
            })
        );
    }

    #[test]
    fn parses_vibrate_with_minutes() {
        assert_eq!(
            parse("/vibrate 10 2m"),
            Ok(BotCommand::Vibrate {
                strength: 10,
                time_sec: 120
            })
        );
    }

    #[test]
    fn parses_vibrate_with_hours() {
        assert_eq!(
            parse("/vibrate 1 1h"),
            Ok(BotCommand::Vibrate {
                strength: 1,
                time_sec: 3600
            })
        );
    }

    #[test]
    fn clamps_strength_over_max() {
        assert_eq!(
            parse("/vibrate 50"),
            Ok(BotCommand::Vibrate {
                strength: 20,
                time_sec: 0
            })
        );
    }

    #[test]
    fn accepts_strength_at_bounds() {
        assert!(matches!(
            parse("/vibrate 0"),
            Ok(BotCommand::Vibrate { strength: 0, .. })
        ));
        assert!(matches!(
            parse("/vibrate 20"),
            Ok(BotCommand::Vibrate { strength: 20, .. })
        ));
    }

    #[test]
    fn vibrate_missing_strength_errors() {
        assert_eq!(parse("/vibrate"), Err(ParseError::MissingStrength));
        assert_eq!(parse("/v"), Err(ParseError::MissingStrength));
    }

    #[test]
    fn vibrate_non_numeric_strength_errors() {
        assert_eq!(
            parse("/vibrate hard"),
            Err(ParseError::InvalidStrength("hard".into()))
        );
    }

    #[test]
    fn vibrate_strength_above_u8_errors() {
        // 300 does not fit in u8, so it is rejected before clamping.
        assert_eq!(
            parse("/vibrate 300"),
            Err(ParseError::InvalidStrength("300".into()))
        );
    }

    #[test]
    fn vibrate_negative_strength_errors() {
        assert_eq!(
            parse("/vibrate -5"),
            Err(ParseError::InvalidStrength("-5".into()))
        );
    }

    #[test]
    fn vibrate_bad_duration_errors() {
        assert_eq!(
            parse("/vibrate 5 abc"),
            Err(ParseError::InvalidDuration("abc".into()))
        );
    }

    #[test]
    fn parses_stop_and_aliases() {
        assert_eq!(parse("/stop"), Ok(BotCommand::Stop));
        assert_eq!(parse("/s"), Ok(BotCommand::Stop));
        assert_eq!(parse("/halt"), Ok(BotCommand::Stop));
    }

    #[test]
    fn strips_bot_username_suffix() {
        assert_eq!(parse("/stop@edge_relay_bot"), Ok(BotCommand::Stop));
        assert_eq!(
            parse("/vibrate@edge_relay_bot 9"),
            Ok(BotCommand::Vibrate {
                strength: 9,
                time_sec: 0
            })
        );
    }

    #[test]
    fn command_name_is_case_insensitive() {
        assert_eq!(parse("/STOP"), Ok(BotCommand::Stop));
        assert_eq!(
            parse("/Vibrate 3"),
            Ok(BotCommand::Vibrate {
                strength: 3,
                time_sec: 0
            })
        );
    }

    #[test]
    fn parses_pair_help_status_and_aliases() {
        assert_eq!(parse("/pair"), Ok(BotCommand::Pair));
        assert_eq!(parse("/link"), Ok(BotCommand::Pair));
        assert_eq!(parse("/help"), Ok(BotCommand::Help));
        assert_eq!(parse("/start"), Ok(BotCommand::Help));
        assert_eq!(parse("/status"), Ok(BotCommand::Status));
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        assert_eq!(parse("   /stop   "), Ok(BotCommand::Stop));
        assert_eq!(
            parse("\t/vibrate   8\n"),
            Ok(BotCommand::Vibrate {
                strength: 8,
                time_sec: 0
            })
        );
    }

    #[test]
    fn empty_text_errors() {
        assert_eq!(parse(""), Err(ParseError::Empty));
        assert_eq!(parse("   "), Err(ParseError::Empty));
        assert_eq!(parse("\t\n"), Err(ParseError::Empty));
    }

    #[test]
    fn non_command_text_errors() {
        assert_eq!(parse("hello there"), Err(ParseError::NotACommand));
        assert_eq!(parse("vibrate 5"), Err(ParseError::NotACommand));
    }

    #[test]
    fn unknown_command_errors() {
        assert_eq!(
            parse("/foobar"),
            Err(ParseError::UnknownCommand("foobar".into()))
        );
        assert_eq!(
            parse("/foobar@bot extra"),
            Err(ParseError::UnknownCommand("foobar".into()))
        );
    }

    #[test]
    fn bare_slash_is_unknown_empty_command() {
        assert_eq!(parse("/"), Err(ParseError::UnknownCommand("".into())));
    }

    #[test]
    fn duration_plain_number_is_seconds() {
        assert_eq!(parse_duration("45"), Ok(45));
        assert_eq!(parse_duration("0"), Ok(0));
    }

    #[test]
    fn duration_suffixes() {
        assert_eq!(parse_duration("90s"), Ok(90));
        assert_eq!(parse_duration("3m"), Ok(180));
        assert_eq!(parse_duration("2h"), Ok(7200));
    }

    #[test]
    fn duration_empty_errors() {
        assert_eq!(
            parse_duration(""),
            Err(ParseError::InvalidDuration("".into()))
        );
        assert_eq!(
            parse_duration("   "),
            Err(ParseError::InvalidDuration("".into()))
        );
    }

    #[test]
    fn duration_bare_suffix_errors() {
        // "s" with no number is invalid.
        assert_eq!(
            parse_duration("s"),
            Err(ParseError::InvalidDuration("s".into()))
        );
        assert_eq!(
            parse_duration("m"),
            Err(ParseError::InvalidDuration("m".into()))
        );
    }

    #[test]
    fn duration_overflow_errors() {
        // 100_000_000 minutes overflows u32 seconds.
        assert_eq!(
            parse_duration("100000000m"),
            Err(ParseError::InvalidDuration("100000000m".into()))
        );
    }

    #[test]
    fn duration_non_numeric_errors() {
        assert_eq!(
            parse_duration("abc"),
            Err(ParseError::InvalidDuration("abc".into()))
        );
    }

    #[test]
    fn to_lovense_maps_vibrate() {
        let c = BotCommand::Vibrate {
            strength: 8,
            time_sec: 5,
        }
        .to_lovense()
        .unwrap();
        assert_eq!(c.action(), "Vibrate:8");
        assert_eq!(c.time_sec(), 5);
    }

    #[test]
    fn to_lovense_maps_stop() {
        assert_eq!(BotCommand::Stop.to_lovense().unwrap().action(), "Stop");
    }

    #[test]
    fn to_lovense_none_for_meta_commands() {
        assert!(BotCommand::Pair.to_lovense().is_none());
        assert!(BotCommand::Help.to_lovense().is_none());
        assert!(BotCommand::Status.to_lovense().is_none());
    }

    #[test]
    fn default_time_sec_is_zero() {
        assert_eq!(DEFAULT_TIME_SEC, 0);
    }

    // --- boundary eventualities ---

    #[test]
    fn duration_suffix_is_case_insensitive() {
        assert_eq!(parse_duration("30S"), Ok(30));
        assert_eq!(parse_duration("2M"), Ok(120));
        assert_eq!(parse_duration("1H"), Ok(3600));
    }

    #[test]
    fn vibrate_uppercase_duration_suffix() {
        assert_eq!(
            parse("/vibrate 5 2M"),
            Ok(BotCommand::Vibrate {
                strength: 5,
                time_sec: 120
            })
        );
    }

    #[test]
    fn strength_at_u8_max_clamps_to_twenty() {
        // 255 is a valid u8, so it parses then clamps — it is not an error.
        assert_eq!(
            parse("/vibrate 255"),
            Ok(BotCommand::Vibrate {
                strength: 20,
                time_sec: 0
            })
        );
    }

    #[test]
    fn strength_just_above_u8_errors() {
        // 256 is the first value that does not fit in u8.
        assert_eq!(
            parse("/vibrate 256"),
            Err(ParseError::InvalidStrength("256".into()))
        );
    }

    #[test]
    fn strength_leading_plus_is_accepted() {
        // u8 parsing allows a leading '+', so "+5" is strength 5.
        assert_eq!(
            parse("/vibrate +5"),
            Ok(BotCommand::Vibrate {
                strength: 5,
                time_sec: 0
            })
        );
    }

    #[test]
    fn duration_at_u32_max_is_ok() {
        assert_eq!(parse_duration("4294967295"), Ok(u32::MAX));
    }

    #[test]
    fn duration_just_above_u32_errors() {
        assert_eq!(
            parse_duration("4294967296"),
            Err(ParseError::InvalidDuration("4294967296".into()))
        );
    }

    #[test]
    fn duration_negative_errors() {
        assert_eq!(
            parse_duration("-30"),
            Err(ParseError::InvalidDuration("-30".into()))
        );
    }

    #[test]
    fn extra_trailing_tokens_are_ignored() {
        // Tokens after the duration are ignored rather than rejected.
        assert_eq!(
            parse("/vibrate 5 30 garbage here"),
            Ok(BotCommand::Vibrate {
                strength: 5,
                time_sec: 30
            })
        );
    }

    // --- dispatch decision eventualities ---

    fn update(json: &str) -> Update {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn dispatch_ignores_update_without_message() {
        assert_eq!(dispatch(&update(r#"{}"#), 42), Dispatch::Ignore);
    }

    #[test]
    fn dispatch_ignores_unauthorized_sender() {
        let u = update(r#"{"message":{"text":"/stop","from":{"id":999},"chat":{"id":5}}}"#);
        assert_eq!(dispatch(&u, 42), Dispatch::Ignore);
    }

    #[test]
    fn dispatch_ignores_message_without_sender() {
        // No `from` → never authorized.
        let u = update(r#"{"message":{"text":"/stop","chat":{"id":5}}}"#);
        assert_eq!(dispatch(&u, 42), Dispatch::Ignore);
    }

    #[test]
    fn dispatch_runs_authorized_stop() {
        let u = update(r#"{"message":{"text":"/stop","from":{"id":42},"chat":{"id":5}}}"#);
        assert_eq!(
            dispatch(&u, 42),
            Dispatch::Command {
                chat_id: 5,
                command: BotCommand::Stop
            }
        );
    }

    #[test]
    fn dispatch_runs_authorized_vibrate_with_chat_id() {
        let u = update(r#"{"message":{"text":"/vibrate 9 30s","from":{"id":42},"chat":{"id":7}}}"#);
        assert_eq!(
            dispatch(&u, 42),
            Dispatch::Command {
                chat_id: 7,
                command: BotCommand::Vibrate {
                    strength: 9,
                    time_sec: 30
                }
            }
        );
    }

    #[test]
    fn dispatch_ignores_plain_chatter_from_authorized_user() {
        let u = update(r#"{"message":{"text":"hello love","from":{"id":42},"chat":{"id":5}}}"#);
        assert_eq!(dispatch(&u, 42), Dispatch::Ignore);
    }

    #[test]
    fn dispatch_ignores_empty_text_from_authorized_user() {
        let u = update(r#"{"message":{"text":"","from":{"id":42},"chat":{"id":5}}}"#);
        assert_eq!(dispatch(&u, 42), Dispatch::Ignore);
    }

    #[test]
    fn dispatch_ignores_non_text_message() {
        // A sticker/photo message has no `text` field at all.
        let u = update(r#"{"message":{"from":{"id":42},"chat":{"id":5}}}"#);
        assert_eq!(dispatch(&u, 42), Dispatch::Ignore);
    }

    #[test]
    fn dispatch_reports_missing_strength_to_authorized_user() {
        let u = update(r#"{"message":{"text":"/vibrate","from":{"id":42},"chat":{"id":5}}}"#);
        assert_eq!(
            dispatch(&u, 42),
            Dispatch::Invalid {
                chat_id: 5,
                error: ParseError::MissingStrength
            }
        );
    }

    #[test]
    fn dispatch_reports_unknown_command_to_authorized_user() {
        let u = update(r#"{"message":{"text":"/wat","from":{"id":42},"chat":{"id":5}}}"#);
        assert_eq!(
            dispatch(&u, 42),
            Dispatch::Invalid {
                chat_id: 5,
                error: ParseError::UnknownCommand("wat".into())
            }
        );
    }

    #[test]
    fn dispatch_ignores_missing_sender_even_when_allowed_is_zero() {
        // The default-0 footgun is closed: a message with no `from` is ignored
        // even if the allowed id were somehow 0.
        let u = update(r#"{"message":{"text":"/stop","chat":{"id":5}}}"#);
        assert_eq!(dispatch(&u, 0), Dispatch::Ignore);
    }

    #[test]
    fn dispatch_authorizes_real_zero_sender_when_allowed_is_zero() {
        // A present sender with id 0 still matches an allowed id of 0 — the
        // gate is exact equality, only the *missing* sender is special-cased.
        let u = update(r#"{"message":{"text":"/stop","from":{"id":0},"chat":{"id":5}}}"#);
        assert_eq!(
            dispatch(&u, 0),
            Dispatch::Command {
                chat_id: 5,
                command: BotCommand::Stop
            }
        );
    }

    // --- reply rendering ---

    #[test]
    fn ack_vibrate_with_duration_reads_naturally() {
        let r = BotCommand::Vibrate {
            strength: 9,
            time_sec: 30,
        }
        .ack()
        .unwrap();
        assert_eq!(r, "Vibrating at 9 for 30s.");
    }

    #[test]
    fn ack_vibrate_zero_duration_says_until_stop_not_zero_seconds() {
        // Regression: 0 means "until /stop", so the reply must not say "0s".
        let r = BotCommand::Vibrate {
            strength: 12,
            time_sec: 0,
        }
        .ack()
        .unwrap();
        assert_eq!(r, "Vibrating at 12 until /stop.");
        assert!(!r.contains("0s"));
    }

    #[test]
    fn ack_stop() {
        assert_eq!(BotCommand::Stop.ack().as_deref(), Some("Stopped."));
    }

    #[test]
    fn ack_help_and_status_return_help_text() {
        assert_eq!(BotCommand::Help.ack().as_deref(), Some(HELP));
        assert_eq!(BotCommand::Status.ack().as_deref(), Some(HELP));
    }

    #[test]
    fn ack_pair_is_none_because_it_needs_a_url() {
        assert_eq!(BotCommand::Pair.ack(), None);
    }

    #[test]
    fn help_text_lists_each_command() {
        for c in ["/vibrate", "/stop", "/pair", "/help"] {
            assert!(HELP.contains(c), "HELP missing {c}");
        }
    }

    #[test]
    fn send_message_url_built_from_base_and_token() {
        assert_eq!(
            send_message_url(DEFAULT_TELEGRAM_BASE, "123:abc"),
            "https://api.telegram.org/bot123:abc/sendMessage"
        );
        assert_eq!(
            send_message_url("http://127.0.0.1:8788", "t"),
            "http://127.0.0.1:8788/bott/sendMessage"
        );
    }

    #[test]
    fn invalid_reply_missing_strength_shows_usage() {
        let r = invalid_reply(&ParseError::MissingStrength);
        assert!(r.contains("/vibrate"), "got: {r}");
        // The raw Debug name must not leak to the user.
        assert!(!r.contains("MissingStrength"));
    }

    #[test]
    fn invalid_reply_echoes_bad_strength() {
        let r = invalid_reply(&ParseError::InvalidStrength("hard".into()));
        assert!(r.contains("0-20"));
        assert!(r.contains("hard"));
    }

    #[test]
    fn invalid_reply_echoes_bad_duration() {
        let r = invalid_reply(&ParseError::InvalidDuration("5y".into()));
        assert!(r.contains("5y"));
        assert!(r.contains("30s"));
    }

    #[test]
    fn invalid_reply_names_unknown_command() {
        let r = invalid_reply(&ParseError::UnknownCommand("wat".into()));
        assert!(r.contains("/wat"));
        assert!(r.contains("/help"));
    }

    #[test]
    fn invalid_reply_is_total_over_all_errors() {
        // Even the variants dispatch never surfaces produce a non-empty reply.
        for e in [ParseError::Empty, ParseError::NotACommand] {
            assert!(!invalid_reply(&e).is_empty());
        }
    }

    // --- robustness / never-panic eventualities ---

    #[test]
    fn parse_handles_unicode_command_gracefully() {
        // Non-ASCII command names must not panic on the `raw[1..]` byte slice.
        assert_eq!(
            parse("/café"),
            Err(ParseError::UnknownCommand("café".into()))
        );
        assert_eq!(parse("/🔥"), Err(ParseError::UnknownCommand("🔥".into())));
        // Unicode is lowercased like ASCII.
        assert_eq!(
            parse("/CAFÉ"),
            Err(ParseError::UnknownCommand("café".into()))
        );
    }

    #[test]
    fn parse_never_panics_on_adversarial_input() {
        let long = "/".repeat(10_000);
        let inputs = [
            "",
            " ",
            "\t",
            "/",
            "//",
            "/@",
            "/@bot",
            "/ x",
            "/\u{0}",
            "/vibrate\u{0}",
            "/vibrate 99999999999999999999",
            "/vibrate -1 -1",
            "/vibrate 5 99999999999999h",
            "🔥/stop",
            long.as_str(),
        ];
        // The only contract under fuzz-like input is: it returns, never panics.
        for s in inputs {
            let _ = parse(s);
            let _ = parse_duration(s);
        }
    }
}
