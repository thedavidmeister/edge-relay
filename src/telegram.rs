//! Parsing of incoming Telegram message text into bot commands.

use crate::command::{Command, MAX_STRENGTH};

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
/// Accepts a bare number (seconds) or a `s`/`m`/`h` suffix: `30`, `30s`,
/// `2m`, `1h`. Overflow is reported as [`ParseError::InvalidDuration`].
pub fn parse_duration(s: &str) -> Result<u32, ParseError> {
    let s = s.trim();
    let invalid = || ParseError::InvalidDuration(s.to_string());
    if s.is_empty() {
        return Err(invalid());
    }

    let (num, mult) = if let Some(n) = s.strip_suffix('s') {
        (n, 1)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600)
    } else {
        (s, 1)
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
}
