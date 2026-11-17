//! Time zones for calendar items. An item's date and time are wall-clock
//! time in its zone; without a zone they are local time.

use super::dates;
use chrono::{Local, NaiveDateTime, NaiveTime, TimeZone};
use chrono_tz::Tz;

/// An IANA name (`America/New_York`, any case) or a common alias
/// (`Eastern`, `ET`, `Pacific`, `UTC`).
pub fn parse(name: &str) -> Option<Tz> {
    let n = name.trim();
    if n.is_empty() {
        return None;
    }
    let iana = match n.to_ascii_lowercase().as_str() {
        "eastern" | "et" | "est" | "edt" => "America/New_York",
        "central" | "ct" | "cst" | "cdt" => "America/Chicago",
        "mountain" | "mt" | "mst" | "mdt" => "America/Denver",
        "pacific" | "pt" | "pst" | "pdt" => "America/Los_Angeles",
        "utc" | "gmt" | "z" => "UTC",
        _ => n,
    };
    Tz::from_str_insensitive(iana).ok()
}

/// The canonical name to store for what the user typed.
pub fn canonical(name: &str) -> Option<String> {
    parse(name).map(|z| z.name().to_string())
}

/// A time with an optional trailing zone: `9:00 PM Eastern`, `21:00 utc`.
pub fn parse_time(input: &str) -> Option<(NaiveTime, Option<Tz>)> {
    let s = input.trim();
    if let Some(t) = dates::parse_time(s) {
        return Some((t, None));
    }
    let (head, tail) = s.rsplit_once(char::is_whitespace)?;
    Some((dates::parse_time(head)?, Some(parse(tail)?)))
}

/// Wall-clock time in `zone` as wall-clock time in `target`. An unknown zone
/// leaves the time as it is; a time skipped by a clock change moves forward
/// an hour.
pub fn convert<Z: TimeZone>(at: NaiveDateTime, zone: &str, target: &Z) -> NaiveDateTime {
    let Some(z) = parse(zone) else { return at };
    let there = z.from_local_datetime(&at).earliest().or_else(|| {
        z.from_local_datetime(&(at + chrono::Duration::hours(1)))
            .earliest()
    });
    match there {
        Some(dt) => dt.with_timezone(target).naive_local(),
        None => at,
    }
}

pub fn to_local(at: NaiveDateTime, zone: &str) -> NaiveDateTime {
    convert(at, zone, &Local)
}

/// The zone's abbreviation at that moment, e.g. `EST`.
pub fn abbreviation(at: NaiveDateTime, zone: &str) -> String {
    parse(zone)
        .and_then(|z| z.from_local_datetime(&at).earliest())
        .map(|dt| dt.format("%Z").to_string())
        .unwrap_or_else(|| zone.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, d)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn names_and_aliases() {
        assert_eq!(parse("Eastern"), Some(chrono_tz::America::New_York));
        assert_eq!(parse(" et "), Some(chrono_tz::America::New_York));
        assert_eq!(parse("europe/berlin"), Some(chrono_tz::Europe::Berlin));
        assert_eq!(canonical("PT").as_deref(), Some("America/Los_Angeles"));
        assert_eq!(parse("Narnia"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn times_with_a_zone() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(parse_time("9:00 PM"), Some((t(21, 0), None)));
        assert_eq!(
            parse_time("9:00 PM Eastern"),
            Some((t(21, 0), Some(chrono_tz::America::New_York)))
        );
        assert_eq!(
            parse_time("21:00 utc"),
            Some((t(21, 0), Some(chrono_tz::UTC)))
        );
        assert_eq!(parse_time("9pm Narnia"), None);
    }

    #[test]
    fn converts_between_zones() {
        let berlin = chrono_tz::Europe::Berlin;
        // 9 PM Eastern is 3 AM the next day in Berlin.
        assert_eq!(convert(at(16, 21, 0), "Eastern", &berlin), at(17, 3, 0));
        assert_eq!(convert(at(16, 21, 0), "nowhere", &berlin), at(16, 21, 0));
        assert_eq!(abbreviation(at(16, 21, 0), "America/New_York"), "EST");
    }
}
