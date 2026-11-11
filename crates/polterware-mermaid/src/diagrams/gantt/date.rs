//! The date and duration arithmetic of `ganttDb.js`, the part upstream delegates to dayjs
//! with the `customParseFormat`, `isoWeek`, and `advancedFormat` plugins, written with plain
//! integer arithmetic.
//!
//! [`Date`] is a wall-clock date and time with no zone: month is 1-based, and the fields are
//! the calendar fields dayjs reads (`getFullYear`, `getMonth`, and so on). Upstream works in
//! the browser's local zone; a pure library has none, so
//!
//! - a parsed date without a zone is that wall clock (no daylight-saving shifts),
//! - `x` epoch milliseconds and an explicit `Z`/`±hh:mm` offset are read as UTC,
//! - a date-only text in the `new Date(text)` fallback is that wall clock (a browser would
//!   read it as UTC midnight and show it in the local zone), and
//! - "today" (`new Date()`) is today UTC.
//!
//! [`Date::format`] and [`Date::parse_strict`] implement dayjs's format mini-language with
//! the tokens gantt uses, including its quirks: the strict parse succeeds only when
//! formatting the result reproduces the input, which is why a `x`/`X` format never parses
//! strictly; `format` emits literal text for `Y` alone and for `x`/`X`; and `ss`/`mm` parse
//! one or two digits but format two. Week tokens (`w`, `ww`) and zone names (`z`) are not
//! implemented: a format using them parses as invalid.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// The largest magnitude a JavaScript `Date` holds, in milliseconds (`8.64e15`).
const MAX_TIME: i64 = 8_640_000_000_000_000;
const MILLISECONDS_A_SECOND: i64 = 1_000;
const MILLISECONDS_A_MINUTE: i64 = 60_000;
const MILLISECONDS_A_HOUR: i64 = 3_600_000;
const MILLISECONDS_A_DAY: i64 = 86_400_000;

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const WEEKDAYS_MIN: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
const WEEKDAYS_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const MONTHS_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A duration unit, the dayjs units `parseDuration` can answer (`'Y'` is not one: only
/// `y` years, `M` months, `w` weeks, `d` days, `h` hours, `m` minutes, `s` seconds, and
/// `ms` milliseconds are).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Year,
    Month,
    Week,
    Day,
    Hour,
    Minute,
    Second,
    Millisecond,
}

/// A wall-clock date and time, `month` 1-based and every other field already normalized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
}

impl Date {
    /// A calendar date and time, `None` when a field is out of range.
    pub fn new(
        year: i64,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        millisecond: u32,
    ) -> Option<Self> {
        if !(1..=12).contains(&month)
            || day < 1
            || day > days_in_month(year, month)
            || hour > 23
            || minute > 59
            || second > 59
            || millisecond > 999
        {
            return None;
        }
        Some(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            millisecond,
        })
    }

    /// Midnight of this date.
    pub(crate) fn midnight(self) -> Self {
        Self {
            hour: 0,
            minute: 0,
            second: 0,
            millisecond: 0,
            ..self
        }
    }

    /// Today's date at midnight, UTC (upstream's `new Date()` with `setHours(0, 0, 0, 0)`).
    pub(crate) fn today() -> Self {
        let milliseconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => i64::try_from(elapsed.as_millis()).unwrap_or(MAX_TIME),
            Err(error) => -i64::try_from(error.duration().as_millis()).unwrap_or(MAX_TIME),
        };
        Self::from_epoch_milliseconds(milliseconds)
            .unwrap_or(Self {
                year: 1970,
                month: 1,
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
                millisecond: 0,
            })
            .midnight()
    }

    /// Milliseconds since the Unix epoch, read as UTC.
    pub(crate) fn epoch_milliseconds(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
            .saturating_mul(MILLISECONDS_A_DAY)
            .saturating_add(i64::from(self.hour) * MILLISECONDS_A_HOUR)
            .saturating_add(i64::from(self.minute) * MILLISECONDS_A_MINUTE)
            .saturating_add(i64::from(self.second) * MILLISECONDS_A_SECOND)
            .saturating_add(i64::from(self.millisecond))
    }

    /// The UTC wall clock of `milliseconds`, `None` outside JavaScript's `Date` range.
    pub(crate) fn from_epoch_milliseconds(milliseconds: i64) -> Option<Self> {
        if !(-MAX_TIME..=MAX_TIME).contains(&milliseconds) {
            return None;
        }
        let days = milliseconds.div_euclid(MILLISECONDS_A_DAY);
        let rest = milliseconds.rem_euclid(MILLISECONDS_A_DAY);
        let (year, month, day) = civil_from_days(days);
        Some(Self {
            year,
            month,
            day,
            hour: (rest / MILLISECONDS_A_HOUR) as u32,
            minute: ((rest % MILLISECONDS_A_HOUR) / MILLISECONDS_A_MINUTE) as u32,
            second: ((rest % MILLISECONDS_A_MINUTE) / MILLISECONDS_A_SECOND) as u32,
            millisecond: (rest % MILLISECONDS_A_SECOND) as u32,
        })
    }

    /// dayjs's `add(value, unit)`, `None` where dayjs would get an invalid date.
    pub(crate) fn add(self, value: f64, unit: Unit) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        match unit {
            // `this.set('month'|'year', ...)`: the day clamps to the target month's length.
            Unit::Month => self.shift_months(js_trunc(value)),
            Unit::Year => self.shift_years(js_trunc(value)),
            // `instanceFactorySet(n)`: the day of the month moves by `Math.round(n * value)`.
            Unit::Day => self.shift_days(js_round(value)),
            Unit::Week => self.shift_days(js_round(value * 7.0)),
            Unit::Hour => self.shift_milliseconds(value * MILLISECONDS_A_HOUR as f64),
            Unit::Minute => self.shift_milliseconds(value * MILLISECONDS_A_MINUTE as f64),
            Unit::Second => self.shift_milliseconds(value * MILLISECONDS_A_SECOND as f64),
            Unit::Millisecond => self.shift_milliseconds(value),
        }
    }

    fn shift_milliseconds(self, delta: f64) -> Option<Self> {
        let total = self.epoch_milliseconds() as f64 + delta;
        if !total.is_finite() || total.abs() >= MAX_TIME as f64 {
            return None;
        }
        Self::from_epoch_milliseconds(total.trunc() as i64)
    }

    fn shift_days(self, days: i64) -> Option<Self> {
        Self::from_epoch_milliseconds(
            self.epoch_milliseconds()
                .checked_add(days.checked_mul(MILLISECONDS_A_DAY)?)?,
        )
    }

    fn shift_months(self, months: i64) -> Option<Self> {
        let total = self
            .year
            .checked_mul(12)?
            .checked_add(i64::from(self.month) - 1)?
            .checked_add(months)?;
        let year = total.div_euclid(12);
        let month = (total.rem_euclid(12) + 1) as u32;
        let day = self.day.min(days_in_month(year, month));
        Self::new(
            year,
            month,
            day,
            self.hour,
            self.minute,
            self.second,
            self.millisecond,
        )
    }

    fn shift_years(self, years: i64) -> Option<Self> {
        let year = self.year.checked_add(years)?;
        let day = self.day.min(days_in_month(year, self.month));
        Self::new(
            year,
            self.month,
            day,
            self.hour,
            self.minute,
            self.second,
            self.millisecond,
        )
    }

    /// The ISO weekday: Monday is 1 and Sunday is 7.
    pub(crate) fn iso_weekday(self) -> u32 {
        let sunday_based = self.weekday_index();
        if sunday_based == 0 { 7 } else { sunday_based }
    }

    /// The weekday index, Sunday-based (`Date.getDay`).
    pub(crate) fn weekday_index(self) -> u32 {
        (days_from_civil(self.year, self.month, self.day) + 4).rem_euclid(7) as u32
    }

    /// The English weekday name, capitalized (`format('dddd')`).
    pub(crate) fn weekday_name(self) -> &'static str {
        WEEKDAYS[self.weekday_index() as usize]
    }

    /// dayjs's `format`, for the tokens `REGEX_FORMAT` knows.
    pub(crate) fn format(self, format: &str) -> String {
        let chars: Vec<char> = format.chars().collect();
        let mut out = String::with_capacity(format.len());
        let mut index = 0;
        while index < chars.len() {
            let c = chars[index];
            if c == '['
                && let Some(end) = chars[index + 1..].iter().position(|&c| c == ']')
            {
                out.extend(&chars[index + 1..index + 1 + end]);
                index += end + 2;
                continue;
            }
            if matches!(c, 'Y' | 'M' | 'D' | 'd' | 'H' | 'h' | 'm' | 's' | 'Z' | 'S') {
                let mut run = 1;
                while index + run < chars.len() && chars[index + run] == c {
                    run += 1;
                }
                self.format_run(c, run, &mut out);
                index += run;
                continue;
            }
            match c {
                'a' => out.push_str(if self.hour < 12 { "am" } else { "pm" }),
                'A' => out.push_str(if self.hour < 12 { "AM" } else { "PM" }),
                other => out.push(other),
            }
            index += 1;
        }
        out
    }

    fn format_run(self, token: char, run: usize, out: &mut String) {
        let mut left = run;
        while left > 0 {
            match token {
                'Y' => {
                    if left >= 4 {
                        out.push_str(&pad(self.year, 4));
                        left -= 4;
                    } else if left >= 2 {
                        out.push_str(&last_two_digits(self.year));
                        left -= 2;
                    } else {
                        out.push('Y');
                        left -= 1;
                    }
                }
                'M' => {
                    let take = left.min(4);
                    match take {
                        4 => out.push_str(MONTHS[(self.month - 1) as usize]),
                        3 => out.push_str(MONTHS_SHORT[(self.month - 1) as usize]),
                        2 => out.push_str(&format!("{:02}", self.month)),
                        _ => out.push_str(&self.month.to_string()),
                    }
                    left -= take;
                }
                'D' => {
                    let take = left.min(2);
                    if take == 2 {
                        out.push_str(&format!("{:02}", self.day));
                    } else {
                        out.push_str(&self.day.to_string());
                    }
                    left -= take;
                }
                'd' => {
                    let take = left.min(4);
                    let weekday = self.weekday_index() as usize;
                    match take {
                        4 => out.push_str(WEEKDAYS[weekday]),
                        3 => out.push_str(WEEKDAYS_SHORT[weekday]),
                        2 => out.push_str(WEEKDAYS_MIN[weekday]),
                        _ => out.push_str(&weekday.to_string()),
                    }
                    left -= take;
                }
                'H' => {
                    let take = left.min(2);
                    if take == 2 {
                        out.push_str(&format!("{:02}", self.hour));
                    } else {
                        out.push_str(&self.hour.to_string());
                    }
                    left -= take;
                }
                'h' => {
                    let take = left.min(2);
                    let hour = self.hour % 12;
                    let hour = if hour == 0 { 12 } else { hour };
                    if take == 2 {
                        out.push_str(&format!("{hour:02}"));
                    } else {
                        out.push_str(&hour.to_string());
                    }
                    left -= take;
                }
                'm' => {
                    let take = left.min(2);
                    if take == 2 {
                        out.push_str(&format!("{:02}", self.minute));
                    } else {
                        out.push_str(&self.minute.to_string());
                    }
                    left -= take;
                }
                's' => {
                    let take = left.min(2);
                    if take == 2 {
                        out.push_str(&format!("{:02}", self.second));
                    } else {
                        out.push_str(&self.second.to_string());
                    }
                    left -= take;
                }
                'S' => {
                    if left >= 3 {
                        out.push_str(&format!("{:03}", self.millisecond));
                        left -= 3;
                    } else {
                        out.push('S');
                        left -= 1;
                    }
                }
                'Z' => {
                    let take = left.min(2);
                    out.push_str(if take == 2 { "+0000" } else { "+00:00" });
                    left -= take;
                }
                _ => left = 0,
            }
        }
    }

    /// `dayjs(text, format, true)`: the strict custom-format parse, `None` when invalid.
    ///
    /// Upstream's validity rule is the round trip: after building a date from the parsed
    /// fields, dayjs formats it again with the same format and rejects it when the text
    /// differs. That rule is applied here too, which gives the same answers as dayjs's
    /// loose matching of the tokens inside the text.
    pub(crate) fn parse_strict(text: &str, format: &str) -> Option<Self> {
        // `x`/`X` are not format tokens in dayjs's formatter, so the round trip never holds.
        if format.is_empty() || format == "x" || format == "X" {
            return None;
        }
        let tokens = format_tokens(format);
        let mut fields = ParsedFields::default();
        let mut position = 0;
        for token in &tokens {
            match token {
                FormatElement::Literal(literal) => {
                    if !text[position..].starts_with(literal.as_str()) {
                        return None;
                    }
                    position += literal.len();
                }
                FormatElement::Field(field) => {
                    let used = fields.consume(*field, &text[position..])?;
                    position += used;
                }
            }
        }
        let date = fields.build()?;
        (date.format(format) == text).then_some(date)
    }

    /// JavaScript's `new Date(text)` fallback, for the forms it accepts in practice.
    pub(crate) fn parse_js(text: &str) -> Option<Self> {
        let text = js_trim(text);
        if text.is_empty() {
            return None;
        }
        let (date_part, time_part) = match text.find(['T', 't', ' ']) {
            Some(index) => (&text[..index], Some(&text[index + 1..])),
            None => (text, None),
        };
        let pieces: Vec<&str> = date_part.split(['-', '/']).collect();
        if pieces.len() > 3
            || pieces
                .iter()
                .any(|piece| piece.is_empty() || !piece.chars().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        if pieces.len() == 1 && time_part.is_none() {
            return Self::year_only(pieces[0]);
        }
        let year: i64 = pieces[0].parse().ok()?;
        let month: u32 = match pieces.get(1) {
            Some(piece) => piece.parse().ok()?,
            None => 1,
        };
        let day: u32 = match pieces.get(2) {
            Some(piece) => piece.parse().ok()?,
            None => 1,
        };
        let (hour, minute, second, millisecond, adjustment) = match time_part {
            Some(text) => parse_time(text)?,
            None => (0, 0, 0, 0, 0),
        };
        let date = Self::new(year, month, day, hour, minute, second, millisecond)?;
        if adjustment == 0 {
            Some(date)
        } else {
            Self::from_epoch_milliseconds(
                date.epoch_milliseconds()
                    .checked_add(adjustment * MILLISECONDS_A_MINUTE)?,
            )
        }
    }

    /// A lone number is a year; one or two digits follow the legacy 1900/2000 split.
    fn year_only(text: &str) -> Option<Self> {
        let digits = text.trim_start_matches(['+']);
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let value: i64 = digits.parse().ok()?;
        let year = if digits.len() <= 2 {
            if value > 68 {
                1900 + value
            } else {
                2000 + value
            }
        } else {
            value
        };
        Self::new(year, 1, 1, 0, 0, 0, 0)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.millisecond
        )
    }
}

/// `parseDuration` of `ganttDb.js`: the value and unit, `NaN` and milliseconds when the text
/// is not a duration.
pub(crate) fn parse_duration(text: &str) -> (f64, Unit) {
    let text = js_trim(text);
    let bytes = text.as_bytes();
    const INVALID: (f64, Unit) = (f64::NAN, Unit::Millisecond);
    let mut digits = 0;
    while digits < bytes.len() && bytes[digits].is_ascii_digit() {
        digits += 1;
    }
    if digits == 0 {
        return INVALID;
    }
    let mut value_end = digits;
    let mut unit_start = digits;
    if bytes.get(digits) == Some(&b'.') {
        let mut fraction = digits + 1;
        while fraction < bytes.len() && bytes[fraction].is_ascii_digit() {
            fraction += 1;
        }
        if fraction > digits + 1 {
            value_end = fraction;
            unit_start = fraction;
        }
    }
    let unit = match &text[unit_start..] {
        "y" => Unit::Year,
        "M" => Unit::Month,
        "w" => Unit::Week,
        "d" => Unit::Day,
        "h" => Unit::Hour,
        "m" => Unit::Minute,
        "s" => Unit::Second,
        "ms" => Unit::Millisecond,
        _ => return INVALID,
    };
    match text[..value_end].parse::<f64>() {
        Ok(value) if value.is_finite() => (value, unit),
        _ => INVALID,
    }
}

/// The pieces a strict parse fills, `None` for the fields the text did not name.
#[derive(Default)]
struct ParsedFields {
    year: Option<i64>,
    month: Option<u32>,
    day: Option<u32>,
    hour: Option<u32>,
    minute: Option<u32>,
    second: Option<u32>,
    millisecond: Option<u32>,
    afternoon: Option<bool>,
    /// The `Date.UTC` adjustment an explicit zone asks for, in minutes.
    zone: Option<i64>,
    /// The ISO week a `w`/`ww` token asked for; not implemented.
    week: Option<i64>,
}

impl ParsedFields {
    /// Consumes one field token from the front of `text`, answering the bytes used.
    fn consume(&mut self, token: FormatToken, text: &str) -> Option<usize> {
        let number = |count: Option<usize>| digits(text, count);
        match token {
            FormatToken::Year4 => {
                let (value, used) = number(Some(4))?;
                self.year = Some(value);
                Some(used)
            }
            FormatToken::Year2 => {
                let (value, used) = number(Some(2))?;
                self.year = Some(if value > 68 {
                    1900 + value
                } else {
                    2000 + value
                });
                Some(used)
            }
            FormatToken::Year => {
                let (value, used) = signed_digits(text)?;
                self.year = Some(value);
                Some(used)
            }
            FormatToken::Month => {
                let (value, used) = number(Some(2))?;
                self.month = Some(value as u32);
                Some(used)
            }
            FormatToken::Month2 => {
                let (value, used) = number(Some(2)).filter(|(_, used)| *used == 2)?;
                self.month = Some(value as u32);
                Some(used)
            }
            FormatToken::MonthShort | FormatToken::MonthLong => {
                let used = word(text);
                let word = &text[..used];
                let names: &[&str] = if token == FormatToken::MonthShort {
                    &MONTHS_SHORT
                } else {
                    &MONTHS
                };
                let index = names.iter().position(|name| *name == word)?;
                self.month = Some(index as u32 + 1);
                Some(used)
            }
            FormatToken::Day => {
                let (value, used) = number(Some(2))?;
                self.day = Some(value as u32);
                Some(used)
            }
            FormatToken::Day2 | FormatToken::DayOrdinal => {
                let (value, used) = if token == FormatToken::Day2 {
                    number(Some(2)).filter(|(_, used)| *used == 2)?
                } else {
                    number(Some(2))?
                };
                self.day = Some(value as u32);
                Some(used)
            }
            FormatToken::Hour | FormatToken::Hour2 => {
                let (value, used) = number(Some(2))?;
                self.hour = Some(value as u32);
                Some(used)
            }
            FormatToken::Hour12 | FormatToken::Hour12Two => {
                let (value, used) = number(Some(2))?;
                self.hour = Some(value as u32);
                Some(used)
            }
            FormatToken::Minute | FormatToken::Minute2 => {
                let (value, used) = number(Some(2))?;
                self.minute = Some(value as u32);
                Some(used)
            }
            FormatToken::Second | FormatToken::Second2 => {
                let (value, used) = number(Some(2))?;
                self.second = Some(value as u32);
                Some(used)
            }
            FormatToken::Millisecond1 => {
                let (value, used) = number(Some(1))?;
                self.millisecond = Some(value as u32 * 100);
                Some(used)
            }
            FormatToken::Millisecond2 => {
                let (value, used) = number(Some(2)).filter(|(_, used)| *used == 2)?;
                self.millisecond = Some(value as u32 * 10);
                Some(used)
            }
            FormatToken::Millisecond3 => {
                let (value, used) = number(Some(3)).filter(|(_, used)| *used == 3)?;
                self.millisecond = Some(value as u32);
                Some(used)
            }
            FormatToken::MeridiemUpper => {
                let used = word(text);
                self.afternoon = Some(match &text[..used] {
                    "PM" => true,
                    "AM" => false,
                    _ => return None,
                });
                Some(used)
            }
            FormatToken::MeridiemLower => {
                let used = word(text);
                self.afternoon = Some(match &text[..used] {
                    "pm" => true,
                    "am" => false,
                    _ => return None,
                });
                Some(used)
            }
            FormatToken::Quarter => {
                let (value, used) = number(Some(1))?;
                self.month = Some((value as u32 - 1) * 3 + 1);
                Some(used)
            }
            FormatToken::Week | FormatToken::Week2 => {
                let (value, used) = if token == FormatToken::Week2 {
                    number(Some(2)).filter(|(_, used)| *used == 2)?
                } else {
                    number(Some(2))?
                };
                self.week = Some(value);
                Some(used)
            }
            FormatToken::Zone | FormatToken::Zone2 => {
                let used = zone(text)?;
                self.zone = Some(used.1);
                Some(used.0)
            }
            FormatToken::ZoneName => None,
        }
    }

    /// Dayjs's defaults, then the date: missing fields come from today (or from the first of
    /// the month when the text named a year or a month), and the time starts at midnight.
    fn build(self) -> Option<Date> {
        if self.week.is_some() {
            return None;
        }
        let today = Date::today();
        let year_named = self.year.is_some_and(|year| year != 0);
        let month_named = self.month.is_some_and(|month| month != 0);
        let day = self.day.unwrap_or(if year_named || month_named {
            1
        } else {
            today.day
        });
        let year = match self.year {
            Some(year) if year_named => year,
            _ => today.year,
        };
        let month = if year_named && !month_named {
            1
        } else if month_named {
            self.month?
        } else {
            today.month
        };
        let mut hour = self.hour;
        if let Some(afternoon) = self.afternoon {
            if afternoon {
                if let Some(hours) = hour
                    && hours < 12
                {
                    hour = Some(hours + 12);
                }
            } else if hour == Some(12) {
                hour = Some(0);
            }
        }
        let hour = hour.unwrap_or(0);
        let minute = self.minute.unwrap_or(0);
        let second = self.second.unwrap_or(0);
        let millisecond = self.millisecond.unwrap_or(0);
        if year.abs() > 275_760 {
            return None;
        }
        // `new Date(year, month, day, ...)`: years 0 through 99 mean 1900 through 1999.
        let year = if (0..=99).contains(&year) {
            year + 1900
        } else {
            year
        };
        let date = normalized(year, month, day, hour, minute, second, millisecond)?;
        Some(match self.zone {
            Some(offset) => Date::from_epoch_milliseconds(
                date.epoch_milliseconds()
                    .checked_add(offset.checked_mul(MILLISECONDS_A_MINUTE)?)?,
            )?,
            None => date,
        })
    }
}

/// `new Date(year, month, day, hour, minute, second, millisecond)`: out-of-range fields carry
/// into the next unit.
fn normalized(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: u32,
) -> Option<Date> {
    let total_months = year.checked_mul(12)?.checked_add(i64::from(month) - 1)?;
    let year = total_months.div_euclid(12);
    let month = (total_months.rem_euclid(12) + 1) as u32;
    let days = days_from_civil(year, month, 1).checked_add(i64::from(day) - 1)?;
    let milliseconds = days
        .checked_mul(MILLISECONDS_A_DAY)?
        .checked_add(i64::from(hour) * MILLISECONDS_A_HOUR)?
        .checked_add(i64::from(minute) * MILLISECONDS_A_MINUTE)?
        .checked_add(i64::from(second) * MILLISECONDS_A_SECOND)?
        .checked_add(i64::from(millisecond))?;
    Date::from_epoch_milliseconds(milliseconds)
}

/// One element of a dayjs parse format: literal text or a field token.
#[derive(Clone, Debug, PartialEq, Eq)]
enum FormatElement {
    Literal(String),
    Field(FormatToken),
}

/// One token of a dayjs parse format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatToken {
    Year4,
    Year2,
    Year,
    Month,
    Month2,
    MonthShort,
    MonthLong,
    Day,
    Day2,
    DayOrdinal,
    Hour,
    Hour2,
    Hour12,
    Hour12Two,
    Minute,
    Minute2,
    Second,
    Second2,
    Millisecond1,
    Millisecond2,
    Millisecond3,
    MeridiemUpper,
    MeridiemLower,
    Quarter,
    Week,
    Week2,
    Zone,
    Zone2,
    ZoneName,
}

/// The `formattingTokens` regex of dayjs's `customParseFormat`: bracketed literals, runs of
/// `[-_:/.,()\s]`, and the signed-integer, day, and time tokens. Characters the regex does
/// not know are kept as literals here; the strict round trip rejects them anyway.
fn format_tokens(format: &str) -> Vec<FormatElement> {
    let chars: Vec<char> = format.chars().collect();
    let mut elements = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c == '['
            && let Some(end) = chars[index + 1..].iter().position(|&c| c == ']')
        {
            elements.push(FormatElement::Literal(
                chars[index + 1..index + 1 + end].iter().collect(),
            ));
            index += end + 2;
            continue;
        }
        if is_format_literal(c) {
            let start = index;
            while index < chars.len() && is_format_literal(chars[index]) {
                index += 1;
            }
            elements.push(FormatElement::Literal(chars[start..index].iter().collect()));
            continue;
        }
        let run = |wanted: char| {
            let mut count = 0;
            while index + count < chars.len() && chars[index + count] == wanted {
                count += 1;
            }
            count
        };
        let (token, used) = match c {
            'A' => (FormatToken::MeridiemUpper, 1),
            'a' => (FormatToken::MeridiemLower, 1),
            'Q' => (FormatToken::Quarter, 1),
            'Y' if run('Y') >= 4 => (FormatToken::Year4, 4),
            'Y' if run('Y') >= 2 => (FormatToken::Year2, 2),
            'Y' => (FormatToken::Year, 1),
            'w' if run('w') >= 2 => (FormatToken::Week2, 2),
            'w' => (FormatToken::Week, 1),
            'M' if run('M') >= 4 => (FormatToken::MonthLong, 4),
            'M' if run('M') == 3 => (FormatToken::MonthShort, 3),
            'M' if run('M') == 2 => (FormatToken::Month2, 2),
            'M' => (FormatToken::Month, 1),
            'D' if chars.get(index + 1) == Some(&'o') => (FormatToken::DayOrdinal, 2),
            'D' if run('D') >= 2 => (FormatToken::Day2, 2),
            'D' => (FormatToken::Day, 1),
            'h' if run('h') >= 2 => (FormatToken::Hour12Two, 2),
            'h' => (FormatToken::Hour12, 1),
            'H' if run('H') >= 2 => (FormatToken::Hour2, 2),
            'H' => (FormatToken::Hour, 1),
            'm' if run('m') >= 2 => (FormatToken::Minute2, 2),
            'm' => (FormatToken::Minute, 1),
            's' if run('s') >= 2 => (FormatToken::Second2, 2),
            's' => (FormatToken::Second, 1),
            'S' if run('S') >= 3 => (FormatToken::Millisecond3, 3),
            'S' if run('S') == 2 => (FormatToken::Millisecond2, 2),
            'S' => (FormatToken::Millisecond1, 1),
            'z' => (FormatToken::ZoneName, 1),
            'Z' if run('Z') >= 2 => (FormatToken::Zone2, 2),
            'Z' => (FormatToken::Zone, 1),
            _ => {
                elements.push(FormatElement::Literal(c.to_string()));
                index += 1;
                continue;
            }
        };
        elements.push(FormatElement::Field(token));
        index += used;
    }
    elements
}

fn is_format_literal(c: char) -> bool {
    matches!(c, '-' | '_' | ':' | '/' | '.' | ',' | '(' | ')' | ' ')
        || crate::preprocess::is_js_whitespace(c)
}

/// `\d{n}` or `\d{1,n}`, answering the value and the bytes used.
fn digits(text: &str, count: Option<usize>) -> Option<(i64, usize)> {
    let mut used = 0;
    for c in text.chars() {
        if !c.is_ascii_digit() || count.is_some_and(|limit| used >= limit) {
            break;
        }
        used += 1;
    }
    if used == 0 {
        return None;
    }
    Some((text[..used].parse().ok()?, used))
}

/// `[+-]?\d+`, as the `Y` token parses a signed year.
fn signed_digits(text: &str) -> Option<(i64, usize)> {
    let sign = text
        .chars()
        .next()
        .filter(|c| *c == '+' || *c == '-')
        .map_or(0, char::len_utf8);
    let (value, digits) = digits(&text[sign..], None)?;
    Some((
        if text.starts_with('-') { -value } else { value },
        sign + digits,
    ))
}

/// `\d*[^-_:/,()\s\d]+`: a word, possibly preceded by digits.
fn word(text: &str) -> usize {
    let mut used = 0;
    for c in text.chars() {
        if c.is_ascii_digit() {
            used += c.len_utf8();
        } else {
            break;
        }
    }
    let mut end = used;
    for c in text[used..].chars() {
        if c.is_ascii_digit() || is_format_literal(c) || c.is_whitespace() {
            break;
        }
        end += c.len_utf8();
    }
    end
}

/// `[+-]\d\d:?(\d\d)?|Z`, answering the bytes used and the `Date.UTC` adjustment in minutes.
fn zone(text: &str) -> Option<(usize, i64)> {
    if text.starts_with('Z') {
        return Some((1, 0));
    }
    let sign = match text.chars().next()? {
        '+' => -1,
        '-' => 1,
        _ => return None,
    };
    let body = &text[1..];
    let (hours, used_hours) = digits(body, Some(2)).filter(|(_, used)| *used == 2)?;
    let rest = &body[used_hours..];
    let (minutes, used_minutes) = if let Some(after_colon) = rest.strip_prefix(':') {
        digits(after_colon, Some(2)).unwrap_or((0, 0))
    } else {
        digits(rest, Some(2)).unwrap_or((0, 0))
    };
    let used = 1 + used_hours + usize::from(rest.starts_with(':')) + used_minutes;
    Some((used, sign * (hours * 60 + minutes)))
}

/// `hh[:mm[:ss[.SSS]]]` with an optional zone, answering the time and the `Date.UTC`
/// adjustment in minutes.
#[allow(clippy::type_complexity)]
fn parse_time(text: &str) -> Option<(u32, u32, u32, u32, i64)> {
    let (clock, offset_text) = match text.find(['Z', '+']) {
        Some(index) => (&text[..index], Some(&text[index..])),
        None => {
            // A `-` after the clock introduces a negative offset only when it follows it.
            match text.rfind('-') {
                Some(index) if index > 0 && text[..index].contains(':') => {
                    (&text[..index], Some(&text[index..]))
                }
                _ => (text, None),
            }
        }
    };
    let pieces: Vec<&str> = clock.split(':').collect();
    if pieces.is_empty() || pieces.len() > 3 {
        return None;
    }
    let hour: u32 = pieces[0].parse().ok()?;
    let minute: u32 = pieces.get(1).map_or(Some(0), |piece| piece.parse().ok())?;
    let (second, millisecond) = match pieces.get(2) {
        Some(piece) => {
            let (second, fraction) = match piece.split_once('.') {
                Some((second, fraction)) => (second, Some(fraction)),
                None => (*piece, None),
            };
            let second: u32 = second.parse().ok()?;
            let millisecond = match fraction {
                Some(fraction) => {
                    let mut digits = fraction.chars().take(3).collect::<String>();
                    while digits.len() < 3 {
                        digits.push('0');
                    }
                    digits.parse().ok()?
                }
                None => 0,
            };
            (second, millisecond)
        }
        None => (0, 0),
    };
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let offset = match offset_text {
        Some(offset_text) => {
            let (used, offset) = zone(offset_text)?;
            if used != offset_text.len() {
                return None;
            }
            offset
        }
        None => 0,
    };
    Some((hour, minute, second, millisecond, offset))
}

/// `Utils.s`: `padStart` on a number's string.
fn pad(value: i64, length: usize) -> String {
    let text = value.to_string();
    if text.len() >= length {
        return text;
    }
    format!("{}{text}", "0".repeat(length - text.len()))
}

/// `String(year).slice(-2)`, the `YY` token: it does not pad a short year.
fn last_two_digits(year: i64) -> String {
    let text = year.to_string();
    let count = text.chars().count();
    if count <= 2 {
        text
    } else {
        text.chars().skip(count - 2).collect()
    }
}

/// JavaScript's `Math.round`: halves go toward positive infinity.
fn js_round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

/// JavaScript's `Math.trunc`, for the integer argument of a dayjs setter.
fn js_trunc(value: f64) -> i64 {
    value.trunc() as i64
}

/// JavaScript's `String.prototype.trim`.
fn js_trim(text: &str) -> &str {
    text.trim_matches(crate::preprocess::is_js_whitespace)
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Howard Hinnant's `days_from_civil`: days since 1970-01-01 in the proleptic Gregorian
/// calendar.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_index = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Howard Hinnant's `civil_from_days`: the inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i64, month: u32, day: u32) -> Date {
        Date::new(year, month, day, 0, 0, 0, 0).expect("valid date")
    }

    #[test]
    fn civil_days_round_trip() {
        for (year, month, day) in [
            (1970, 1, 1),
            (2014, 1, 1),
            (2019, 2, 28),
            (2020, 2, 29),
            (2024, 12, 31),
            (1900, 3, 1),
            (1600, 2, 29),
            (2400, 1, 1),
        ] {
            let days = days_from_civil(year, month, day);
            assert_eq!(civil_from_days(days), (year, month, day));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 1, 1), 10_957);
    }

    #[test]
    fn strict_parse_reproduces_the_text() {
        assert_eq!(
            Date::parse_strict("2014-01-01", "YYYY-MM-DD"),
            Some(date(2014, 1, 1))
        );
        // February 30 rolls into March, so the round trip rejects it.
        assert_eq!(Date::parse_strict("2014-02-30", "YYYY-MM-DD"), None);
        // A three-digit year is not `YYYY`.
        assert_eq!(Date::parse_strict("202-12-01", "YYYY-MM-DD"), None);
        // `x` is not a format token, so its round trip never holds.
        assert_eq!(Date::parse_strict("0", "x"), None);
        // Seconds-only formats start from today.
        let today = Date::today();
        assert_eq!(
            Date::parse_strict("20", "ss"),
            Date::new(today.year, today.month, today.day, 0, 0, 20, 0)
        );
    }

    #[test]
    fn format_matches_dayjs() {
        let date = Date::new(2014, 1, 6, 13, 5, 4, 30).expect("valid date");
        assert_eq!(date.format("YYYY-MM-DD"), "2014-01-06");
        assert_eq!(date.format("YYYYMMDD"), "20140106");
        assert_eq!(date.format("dddd"), "Monday");
        assert_eq!(date.format("ss"), "04");
        assert_eq!(date.format("yyyy-mm-dd"), "yyyy-05-Mo");
        assert_eq!(date.format("x"), "x");
        assert_eq!(date.format("[week] dddd"), "week Monday");
        assert_eq!(date.format("YY-M-D H:m:s.SSS A"), "14-1-6 13:5:4.030 PM");
    }

    #[test]
    fn adds_like_dayjs() {
        let january_31 = date(2024, 1, 31);
        assert_eq!(
            january_31.add(1.0, Unit::Month),
            Some(date(2024, 2, 29)),
            "months clamp the day"
        );
        assert_eq!(
            january_31.add(1.0, Unit::Day),
            Some(date(2024, 2, 1)),
            "days carry"
        );
        let start = date(1970, 1, 1);
        assert_eq!(
            start.add(0.005, Unit::Second),
            Date::new(1970, 1, 1, 0, 0, 0, 5)
        );
        assert_eq!(start.add(2.0, Unit::Week), Some(date(1970, 1, 15)));
    }

    #[test]
    fn parses_javascript_date_text() {
        assert_eq!(Date::parse_js("202-12-01"), Some(date(202, 12, 1)));
        assert_eq!(
            Date::parse_js("202304").map(|date| date.year),
            Some(202_304)
        );
        assert_eq!(Date::parse_js("0").map(|date| date.year), Some(2000));
        assert_eq!(Date::parse_js("nonsense"), None);
    }
}
