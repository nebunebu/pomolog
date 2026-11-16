//! Friendly date and duration parsing and formatting.

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Weekday};

/// Accepts `2026-11-20`, `today`, `tomorrow`, a weekday name (the next such
/// day, never today), and `+3d` / `+2w`.
pub fn parse_date(input: &str, today: NaiveDate) -> Option<NaiveDate> {
    let s = input.trim().to_lowercase();
    if s.is_empty() || !s.is_ascii() {
        return None;
    }
    if let Ok(d) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Some(d);
    }
    match s.as_str() {
        "today" | "tod" => return Some(today),
        "tomorrow" | "tom" => return today.succ_opt(),
        _ => {}
    }
    if let Some(rest) = s.strip_prefix('+') {
        let (num, unit) = rest.split_at(rest.len().saturating_sub(1));
        let n: i64 = num.parse().ok()?;
        return match unit {
            "d" => today.checked_add_signed(Duration::days(n)),
            "w" => today.checked_add_signed(Duration::weeks(n)),
            _ => None,
        };
    }
    if s.len() >= 3
        && let Ok(wd) = s.parse::<Weekday>()
    {
        let ahead = (7 + wd.num_days_from_monday() as i64
            - today.weekday().num_days_from_monday() as i64)
            % 7;
        return today.checked_add_signed(Duration::days(if ahead == 0 { 7 } else { ahead }));
    }
    None
}

/// Accepts `14:30`, `1430`, `9`, `9:5`.
pub fn parse_time(input: &str) -> Option<NaiveTime> {
    let s = input.trim();
    if s.is_empty() || !s.is_ascii() {
        return None;
    }
    let (h, m) = match s.split_once([':', '.']) {
        Some((h, m)) => (h.parse().ok()?, m.parse().ok()?),
        None if s.len() > 2 => (
            s[..s.len() - 2].parse().ok()?,
            s[s.len() - 2..].parse().ok()?,
        ),
        None => (s.parse().ok()?, 0),
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// `1:05:09` or `25:00`.
pub fn clock(secs: i64) -> String {
    let s = secs.max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// `2h 05m`, `45m`, `0m`.
pub fn human(secs: i64) -> String {
    let m = (secs.max(0) + 30) / 60;
    if m >= 60 {
        format!("{}h {:02}m", m / 60, m % 60)
    } else {
        format!("{m}m")
    }
}

/// "Today", "Tomorrow", "Yesterday", "Fri 20 Nov", or with the year when it
/// is not the current one.
pub fn relative(d: NaiveDate, today: NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        _ if d.year() == today.year() => d.format("%a %-d %b").to_string(),
        _ => d.format("%-d %b %Y").to_string(),
    }
}

pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap()
}

pub fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + n;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap()
}

pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    #[test]
    fn dates() {
        let today = d(16); // a Monday
        assert_eq!(
            parse_date("2026-12-01", today),
            NaiveDate::from_ymd_opt(2026, 12, 1)
        );
        assert_eq!(parse_date(" Today ", today), Some(d(16)));
        assert_eq!(parse_date("tomorrow", today), Some(d(17)));
        assert_eq!(parse_date("fri", today), Some(d(20)));
        assert_eq!(parse_date("Friday", today), Some(d(20)));
        assert_eq!(
            parse_date("mon", today),
            Some(d(23)),
            "a weekday name is never today"
        );
        assert_eq!(parse_date("+3d", today), Some(d(19)));
        assert_eq!(parse_date("+1w", today), Some(d(23)));
        for bad in ["", "+", "+d", "+3x", "+é", "2026-13-01", "soon", "fr", "é"] {
            assert_eq!(parse_date(bad, today), None, "{bad:?}");
        }
    }

    #[test]
    fn times() {
        assert_eq!(parse_time("14:30"), NaiveTime::from_hms_opt(14, 30, 0));
        assert_eq!(parse_time("1430"), NaiveTime::from_hms_opt(14, 30, 0));
        assert_eq!(parse_time("9"), NaiveTime::from_hms_opt(9, 0, 0));
        assert_eq!(parse_time("9.05"), NaiveTime::from_hms_opt(9, 5, 0));
        for bad in ["", "25:00", "12:60", "abc", "é9"] {
            assert_eq!(parse_time(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn formatting() {
        assert_eq!(clock(1500), "25:00");
        assert_eq!(clock(3909), "1:05:09");
        assert_eq!(clock(-5), "00:00");
        assert_eq!(human(7500), "2h 05m");
        assert_eq!(human(29), "0m");
        assert_eq!(relative(d(17), d(16)), "Tomorrow");
        assert_eq!(relative(d(20), d(16)), "Fri 20 Nov");
        assert_eq!(
            relative(NaiveDate::from_ymd_opt(2027, 1, 2).unwrap(), d(16)),
            "2 Jan 2027"
        );
    }

    #[test]
    fn months_and_weeks() {
        assert_eq!(
            add_months(d(16), 2),
            NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
        );
        assert_eq!(
            add_months(d(16), -11),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap()
        );
        assert_eq!(week_start(d(22)), d(16));
        assert_eq!(month_start(d(22)), d(1));
    }
}
