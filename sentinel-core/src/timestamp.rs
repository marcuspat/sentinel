//! UTC timestamp and weekday types used at Sentinel's persisted data boundaries.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use calendrical_calculations::gregorian::{fixed_from_gregorian, gregorian_from_fixed};
use calendrical_calculations::rata_die::RataDie;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const SECONDS_PER_DAY: i64 = 86_400;
const NANOS_PER_SECOND: u32 = 1_000_000_000;

// Match Sentinel v0.2.1's representable UTC date range. Dates outside it
// cannot be emitted if an older binary must still read these records.
const MIN_YEAR: i32 = -262_143;
const MAX_YEAR: i32 = 262_142;
const MIN_DATE: RataDie = fixed_from_gregorian(MIN_YEAR, 1, 1);
const MAX_DATE: RataDie = fixed_from_gregorian(MAX_YEAR, 12, 31);
const EPOCH_DATE: RataDie = fixed_from_gregorian(1970, 1, 1);

/// A UTC instant with the same serialized representation used by Sentinel v0.2.1.
///
/// The extra nanosecond range in the last second of a minute preserves leap
/// seconds. It also makes ordinary and leap seconds sort in their original
/// order without rounding either to the next minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcTimestamp {
    day: RataDie,
    second_of_day: u32,
    nanoseconds: u32,
}

/// Subsecond precision for rendering UTC timestamps.
#[derive(Debug, Clone, Copy)]
pub enum TimestampPrecision {
    Seconds,
    Milliseconds,
    Nanoseconds,
    /// Use 0, 3, 6, or 9 digits, whichever first preserves the value.
    Auto,
}

/// The timestamp is malformed or outside Sentinel's supported date range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid or out-of-range UTC timestamp")]
pub struct ParseTimestampError;

impl UtcTimestamp {
    /// Read the trusted system wall clock.
    ///
    /// Panics if the clock is outside the supported calendar range.
    pub fn now() -> Self {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos() as i128,
            Err(error) => -(error.duration().as_nanos() as i128),
        };
        let scale = i128::from(NANOS_PER_SECOND);
        let seconds = i64::try_from(nanos.div_euclid(scale)).expect("system clock epoch seconds");
        let nanos = nanos.rem_euclid(scale) as u32;
        Self::from_day_seconds(EPOCH_DATE, seconds, nanos).expect("system clock in timestamp range")
    }

    fn from_day_seconds(
        day: RataDie,
        seconds: i64,
        nanoseconds: u32,
    ) -> Result<Self, ParseTimestampError> {
        let day = day.add(seconds.div_euclid(SECONDS_PER_DAY));
        if !(MIN_DATE..=MAX_DATE).contains(&day) {
            return Err(ParseTimestampError);
        }
        Ok(Self {
            day,
            second_of_day: seconds.rem_euclid(SECONDS_PER_DAY) as u32,
            nanoseconds,
        })
    }

    /// Returns the hour in the UTC calendar day.
    pub fn hour(self) -> u8 {
        (self.second_of_day / 3_600) as u8
    }

    /// Returns the weekday in UTC.
    pub fn weekday(self) -> UtcWeekday {
        // Rata Die day 1 is a Monday.
        let index = (self.day.to_i64_date() - 1).rem_euclid(7) as usize;
        UtcWeekday::ALL[index]
    }

    fn minute(self) -> u32 {
        self.second_of_day / 60 % 60
    }

    fn second(self) -> u32 {
        self.second_of_day % 60 + u32::from(self.nanoseconds >= NANOS_PER_SECOND)
    }

    /// Render the UTC time of day as HH:MM:SS.
    pub fn clock_time(self) -> String {
        format!(
            "{:02}:{:02}:{:02}",
            self.hour(),
            self.minute(),
            self.second()
        )
    }

    /// Render an ISO 8601 timestamp with a UTC suffix and explicit precision.
    pub fn rfc3339(self, precision: TimestampPrecision) -> impl fmt::Display {
        Rfc3339 {
            timestamp: self,
            precision,
        }
    }

    fn write_date_time(
        self,
        f: &mut fmt::Formatter<'_>,
        separator: char,
        precision: TimestampPrecision,
    ) -> fmt::Result {
        let (year, month, day) = gregorian_from_fixed(self.day).map_err(|_| fmt::Error)?;
        if (0..10_000).contains(&year) {
            write!(f, "{year:04}")?;
        } else {
            write!(f, "{year:+05}")?;
        }
        write!(
            f,
            "-{month:02}-{day:02}{separator}{:02}:{:02}:{:02}",
            self.hour(),
            self.minute(),
            self.second()
        )?;

        let nanos = self.nanoseconds % NANOS_PER_SECOND;
        let width = match precision {
            TimestampPrecision::Seconds => 0,
            TimestampPrecision::Milliseconds => 3,
            TimestampPrecision::Nanoseconds => 9,
            TimestampPrecision::Auto if nanos == 0 => 0,
            TimestampPrecision::Auto if nanos % 1_000_000 == 0 => 3,
            TimestampPrecision::Auto if nanos % 1_000 == 0 => 6,
            TimestampPrecision::Auto => 9,
        };
        if width != 0 {
            let fraction = nanos / 10_u32.pow(9 - width);
            write!(f, ".{fraction:0width$}", width = width as usize)?;
        }
        Ok(())
    }
}

struct Rfc3339 {
    timestamp: UtcTimestamp,
    precision: TimestampPrecision,
}

impl fmt::Display for Rfc3339 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.timestamp.write_date_time(f, 'T', self.precision)?;
        f.write_str("Z")
    }
}

impl fmt::Display for UtcTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_date_time(f, ' ', TimestampPrecision::Auto)?;
        f.write_str(" UTC")
    }
}

impl Serialize for UtcTimestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.rfc3339(TimestampPrecision::Auto))
    }
}

impl<'de> Deserialize<'de> for UtcTimestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Cow::<'de, str>::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl FromStr for UtcTimestamp {
    type Err = ParseTimestampError;

    fn from_str(mut input: &str) -> Result<Self, Self::Err> {
        input = input.trim_start();
        let year = if let Some(signed_digits) = input.strip_prefix(['+', '-']) {
            let negative = input.starts_with('-');
            input = signed_digits;
            let year = digits(&mut input, usize::MAX).ok_or(ParseTimestampError)?;
            let year = i32::try_from(year).map_err(|_| ParseTimestampError)?;
            if negative {
                -year
            } else {
                year
            }
        } else {
            digits(&mut input, 4).ok_or(ParseTimestampError)? as i32
        };
        separator(&mut input, '-')?;
        let month = component(&mut input)?;
        separator(&mut input, '-')?;
        let day = component(&mut input)?;
        if !(MIN_YEAR..=MAX_YEAR).contains(&year) || !(1..=12).contains(&month) || day == 0 {
            return Err(ParseTimestampError);
        }
        let date = fixed_from_gregorian(year, month, day);
        if gregorian_from_fixed(date).ok() != Some((year, month, day)) {
            return Err(ParseTimestampError);
        }
        match input.as_bytes().first() {
            Some(b'T' | b't' | b' ') => input = &input[1..],
            _ => return Err(ParseTimestampError),
        }

        let hour = component(&mut input)?;
        separator(&mut input, ':')?;
        let minute = component(&mut input)?;
        separator(&mut input, ':')?;
        let second = component(&mut input)?;
        if hour > 23 || minute > 59 || second > 60 {
            return Err(ParseTimestampError);
        }

        let mut nanos = 0;
        if let Some(fraction) = input.strip_prefix('.') {
            let count = fraction.bytes().take_while(u8::is_ascii_digit).count();
            if count == 0 {
                return Err(ParseTimestampError);
            }
            let width = count.min(9);
            nanos = fraction[..width]
                .parse::<u32>()
                .map_err(|_| ParseTimestampError)?
                * 10_u32.pow((9 - width) as u32);
            input = &fraction[count..];
        }
        // Store the extra second against :59, even when an explicit offset
        // changes the UTC minute or date.
        let seconds = i64::from(hour) * 3_600 + i64::from(minute) * 60 + i64::from(second.min(59));
        if second == 60 {
            nanos += NANOS_PER_SECOND;
        }
        Self::from_day_seconds(date, seconds - offset_seconds(input.trim())?, nanos)
    }
}

fn digits(input: &mut &str, max: usize) -> Option<u32> {
    let count = input
        .bytes()
        .take(max)
        .take_while(u8::is_ascii_digit)
        .count();
    let (value, rest) = input.split_at(count);
    let value = value.parse().ok()?;
    *input = rest;
    Some(value)
}

fn component(input: &mut &str) -> Result<u8, ParseTimestampError> {
    *input = input.trim_start();
    digits(input, 2).map(|n| n as u8).ok_or(ParseTimestampError)
}

fn separator(input: &mut &str, ch: char) -> Result<(), ParseTimestampError> {
    *input = input
        .trim_start()
        .strip_prefix(ch)
        .ok_or(ParseTimestampError)?;
    Ok(())
}

fn offset_seconds(input: &str) -> Result<i64, ParseTimestampError> {
    if input.eq_ignore_ascii_case("Z") || input.eq_ignore_ascii_case("UTC") {
        return Ok(0);
    }
    let (sign, digits) = if let Some(rest) = input.strip_prefix('+') {
        (1, rest)
    } else if let Some(rest) = input.strip_prefix('-').or_else(|| input.strip_prefix('−')) {
        (-1, rest)
    } else {
        return Err(ParseTimestampError);
    };
    let hour = digits.get(..2).ok_or(ParseTimestampError)?;
    let minute = digits[2..].trim_start_matches(|ch: char| ch == ':' || ch.is_whitespace());
    if !hour.bytes().all(|b| b.is_ascii_digit())
        || minute.len() != 2
        || !minute.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(ParseTimestampError);
    }
    let hour = hour.parse::<i64>().map_err(|_| ParseTimestampError)?;
    let minute = minute.parse::<i64>().map_err(|_| ParseTimestampError)?;
    if hour > 23 || minute > 59 {
        return Err(ParseTimestampError);
    }
    Ok(sign * (hour * 3_600 + minute * 60))
}

/// UTC weekday with Sentinel's original abbreviated serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UtcWeekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl UtcWeekday {
    const ALL: [Self; 7] = [
        Self::Mon,
        Self::Tue,
        Self::Wed,
        Self::Thu,
        Self::Fri,
        Self::Sat,
        Self::Sun,
    ];
    const NAMES: [(&'static str, &'static str); 7] = [
        ("Mon", "Monday"),
        ("Tue", "Tuesday"),
        ("Wed", "Wednesday"),
        ("Thu", "Thursday"),
        ("Fri", "Friday"),
        ("Sat", "Saturday"),
        ("Sun", "Sunday"),
    ];
}

impl Serialize for UtcWeekday {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::NAMES[*self as usize].0)
    }
}

impl<'de> Deserialize<'de> for UtcWeekday {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = Cow::<'de, str>::deserialize(deserializer)?;
        Self::NAMES
            .iter()
            .position(|(short, long)| {
                input.eq_ignore_ascii_case(short) || input.eq_ignore_ascii_case(long)
            })
            .map(|index| Self::ALL[index])
            .ok_or_else(|| serde::de::Error::custom("short or long weekday names expected"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_input_is_reported() {
        for input in [
            "2023-02-29T00:00:00Z",
            "2024-01-32T00:00:00Z",
            "2024-00-01T00:00:00Z",
            "2024-01-01T12:00:61Z",
            "2024-01-01T12:00:00.Z",
            "2024-01-01T24:00:00Z",
            "2024-01-01T12:00:00+24:00",
            "2024-01-01T12:00:00-01:60",
            "2024-01-01T12:00:00+０1:00",
            "2024-01-01T12:00:00",
            "2024-01-01T12:00:00Z suffix",
            "-262144-12-31T23:59:59Z",
            "-262143-01-01T00:00:00+00:01",
            "+262142-12-31T23:59:59-00:01",
            "+262143-01-01T00:00:00Z",
        ] {
            assert!(input.parse::<UtcTimestamp>().is_err(), "{input}");
        }
    }

    #[test]
    fn display_can_drop_fractional_seconds_without_rounding() {
        let timestamp: UtcTimestamp = "2024-01-01T12:34:59.999999999Z".parse().unwrap();
        assert_eq!(timestamp.clock_time(), "12:34:59");
        assert_eq!(
            timestamp.rfc3339(TimestampPrecision::Seconds).to_string(),
            "2024-01-01T12:34:59Z"
        );
    }

    #[test]
    fn ordering_keeps_the_leap_second_between_ordinary_seconds() {
        let parse = |input: &str| input.parse::<UtcTimestamp>().unwrap();
        let before = parse("2016-12-31T23:59:59.999999999Z");
        let leap = parse("2017-01-01T00:59:60.100+01:00");
        let after = parse("2017-01-01T00:00:00Z");
        assert!(before < leap && leap < after);
        assert_eq!(leap.clock_time(), "23:59:60");
        assert_eq!(leap.weekday(), UtcWeekday::Sat);
        assert_eq!(after.weekday(), UtcWeekday::Sun);
    }
}
