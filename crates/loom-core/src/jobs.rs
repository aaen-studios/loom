//! Cron schedules for jobs.
//!
//! A small five-field cron parser and next-run calculator. Times are local:
//! the system's UTC offset is applied before fields are matched, so "08:00"
//! means 08:00 on the user's clock (and follows DST, because the offset is
//! looked up at evaluation time).

use crate::db::now_ms;

/// An arbitrary "no more firings" horizon: cron expressions repeat, but a
/// calculation should still terminate on bad input.
const MAX_DAYS_AHEAD: i64 = 366 * 4;

#[derive(Debug, Clone, PartialEq)]
pub struct Cron {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days_of_month: Vec<u32>,
    months: Vec<u32>,
    days_of_week: Vec<u32>,
    dom_any: bool,
    dow_any: bool,
}

impl Cron {
    /// Parses `minute hour day-of-month month day-of-week`.
    pub fn parse(expression: &str) -> Result<Self, String> {
        let fields: Vec<&str> = expression.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(format!(
                "a cron expression has five fields (minute hour day month weekday); got {}",
                fields.len()
            ));
        }

        let minutes = parse_field(fields[0], 0, 59, "minute")?;
        let hours = parse_field(fields[1], 0, 23, "hour")?;
        let days_of_month = parse_field(fields[2], 1, 31, "day of month")?;
        let months = parse_field(fields[3], 1, 12, "month")?;
        let mut days_of_week = parse_field(fields[4], 0, 7, "weekday")?;
        // Both 0 and 7 mean Sunday.
        for day in &mut days_of_week {
            if *day == 7 {
                *day = 0;
            }
        }
        days_of_week.sort_unstable();
        days_of_week.dedup();

        Ok(Self {
            minutes,
            hours,
            days_of_month,
            months,
            days_of_week,
            dom_any: fields[2].trim() == "*",
            dow_any: fields[4].trim() == "*",
        })
    }

    /// The next firing strictly after `after_ms`.
    pub fn next_after(&self, after_ms: i64) -> Option<i64> {
        let offset = local_offset_seconds();
        let after_local = after_ms / 1_000 + offset;
        let after_minute = after_local.div_euclid(60);

        let start_day = after_minute.div_euclid(24 * 60);
        for day in start_day..start_day + MAX_DAYS_AHEAD {
            if !self.matches_day(day) {
                continue;
            }
            for hour in &self.hours {
                for minute in &self.minutes {
                    let candidate_minute = day * 24 * 60 + *hour as i64 * 60 + *minute as i64;
                    if candidate_minute > after_minute {
                        return Some((candidate_minute * 60 - offset) * 1_000);
                    }
                }
            }
        }
        None
    }

    fn matches_day(&self, day: i64) -> bool {
        let (_, month, day_of_month) = crate::fsutil::civil_from_days(day);
        if !self.months.contains(&(month as u32)) {
            return false;
        }

        // Day-of-week: 1970-01-01 was a Thursday (4).
        let weekday = ((day + 4).rem_euclid(7)) as u32;
        let dom_match = self.days_of_month.contains(&(day_of_month as u32));
        let dow_match = self.days_of_week.contains(&weekday);

        match (self.dom_any, self.dow_any) {
            // Both restricted: cron's OR rule.
            (false, false) => dom_match || dow_match,
            (false, true) => dom_match,
            (true, false) => dow_match,
            (true, true) => true,
        }
    }
}

fn parse_field(field: &str, min: u32, max: u32, label: &str) -> Result<Vec<u32>, String> {
    let mut values = Vec::new();
    for part in field.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(format!("empty {label} field"));
        }

        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step: u32 = step
                    .parse()
                    .map_err(|_| format!("bad step \"{step}\" in {label}"))?;
                if step == 0 {
                    return Err(format!("step 0 in {label}"));
                }
                (range, step)
            }
            None => (part, 1),
        };

        let (from, to) = if range == "*" {
            (min, max)
        } else if let Some((from, to)) = range.split_once('-') {
            (parse_number(from, min, max, label)?, parse_number(to, min, max, label)?)
        } else {
            let value = parse_number(range, min, max, label)?;
            // `5/10` means "from 5, every 10".
            (value, if part.contains('/') { max } else { value })
        };

        if from > to {
            return Err(format!("backwards range \"{range}\" in {label}"));
        }
        let mut value = from;
        while value <= to {
            values.push(value);
            value += step;
        }
    }

    values.sort_unstable();
    values.dedup();
    if values.is_empty() {
        return Err(format!("no values in {label}"));
    }
    Ok(values)
}

fn parse_number(text: &str, min: u32, max: u32, label: &str) -> Result<u32, String> {
    let value: u32 = text
        .trim()
        .parse()
        .map_err(|_| format!("bad {label} value \"{text}\""))?;
    if value < min || value > max {
        return Err(format!("{label} value {value} is outside {min}-{max}"));
    }
    Ok(value)
}

/// A friendly "next runs" list for the job editor.
pub fn upcoming(expression: &str, count: usize) -> Result<Vec<i64>, String> {
    let cron = Cron::parse(expression)?;
    let mut runs = Vec::new();
    let mut after = now_ms();
    for _ in 0..count.clamp(1, 10) {
        match cron.next_after(after) {
            Some(next) => {
                runs.push(next);
                after = next;
            }
            None => break,
        }
    }
    Ok(runs)
}

/// `Wed 2026-09-16 08:00 local` — how the UI and the model see a firing time.
pub fn format_local(ms: i64) -> String {
    let seconds = ms / 1_000 + local_offset_seconds();
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    let (year, month, day) = crate::fsutil::civil_from_days(days);
    let weekday = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
        [((days + 4).rem_euclid(7)) as usize];
    format!(
        "{weekday} {year:04}-{month:02}-{day:02} {:02}:{:02}",
        time / 3_600,
        (time % 3_600) / 60
    )
}

/// The system's UTC offset in seconds (local = UTC + offset).
#[cfg(windows)]
fn local_offset_seconds() -> i64 {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    unsafe {
        let mut info = TIME_ZONE_INFORMATION::default();
        // 0 = TIME_ZONE_ID_UNKNOWN, 1 = STANDARD, 2 = DAYLIGHT.
        match GetTimeZoneInformation(&mut info) {
            2 => -(info.Bias as i64 + info.DaylightBias as i64) * 60,
            _ => -(info.Bias as i64) * 60,
        }
    }
}

#[cfg(not(windows))]
fn local_offset_seconds() -> i64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: i64, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
        let days = crate::fsutil::days_from_civil(year, month as i64, day as i64);
        (days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60) * 1_000
    }

    #[test]
    fn every_minute_is_one_minute_away() {
        let cron = Cron::parse("* * * * *").unwrap();
        let start = at(2026, 9, 16, 10, 30);
        assert_eq!(cron.next_after(start), Some(start + 60_000));
    }

    #[test]
    fn daily_time_is_respected() {
        let cron = Cron::parse("0 8 * * *").unwrap();
        let start = at(2026, 9, 16, 10, 30);
        let next = cron.next_after(start).unwrap();
        assert_eq!(next, at(2026, 9, 17, 8, 0));
    }

    #[test]
    fn weekdays_and_ranges_work() {
        // 09:00 on weekdays. 2026-09-18 is a Friday, so the next is Monday the 21st.
        let cron = Cron::parse("0 9 * * 1-5").unwrap();
        let start = at(2026, 9, 18, 12, 0);
        assert_eq!(cron.next_after(start), Some(at(2026, 9, 21, 9, 0)));
    }

    #[test]
    fn steps_expand() {
        let cron = Cron::parse("*/15 9-10 * * *").unwrap();
        let start = at(2026, 9, 16, 9, 5);
        assert_eq!(cron.next_after(start), Some(at(2026, 9, 16, 9, 15)));
        assert_eq!(cron.next_after(at(2026, 9, 16, 9, 45)), Some(at(2026, 9, 16, 9, 45) + 15 * 60_000));
    }

    #[test]
    fn bad_expressions_are_rejected() {
        assert!(Cron::parse("* * * *").is_err());
        assert!(Cron::parse("61 * * * *").is_err());
        assert!(Cron::parse("* 24 * * *").is_err());
        assert!(Cron::parse("* * 0 * *").is_err());
    }
}
