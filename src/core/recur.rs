//! Recurrence of calendar items.

use super::model::{Item, ItemKind, Recur};
use super::tz;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};
use std::collections::HashSet;

pub fn occurs_on(item: &Item, d: NaiveDate) -> bool {
    if d < item.date {
        return false;
    }
    if let Some(until) = item.recur_until
        && d > until
    {
        return false;
    }
    match item.recur {
        Recur::None => d == item.date,
        Recur::Daily => true,
        Recur::Weekly => d.weekday() == item.date.weekday(),
        Recur::Monthly => d.day() == item.date.day(),
        Recur::Yearly => d.day() == item.date.day() && d.month() == item.date.month(),
    }
}

/// An item as it falls on one local day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Occ<'a> {
    pub item: &'a Item,
    /// Local time of day, converted from the item's zone if it has one.
    pub time: Option<NaiveTime>,
}

/// Items occurring on the local day `d`: holidays, then timed ones in time
/// order, then the rest.
pub fn occurrences_on(items: &[Item], d: NaiveDate) -> Vec<Occ<'_>> {
    occurrences_in(items, d, &Local)
}

/// As `occurrences_on`, with "local" being `zone`. A timed item written in
/// another zone can land on the day before or after its own date.
pub fn occurrences_in<'a, Z: TimeZone>(items: &'a [Item], d: NaiveDate, zone: &Z) -> Vec<Occ<'a>> {
    let mut v: Vec<Occ> = Vec::new();
    for item in items {
        match (item.time, item.tz.as_deref()) {
            (Some(t), Some(z)) => {
                for shift in -1..=1 {
                    let there = d + Duration::days(shift);
                    if !occurs_on(item, there) {
                        continue;
                    }
                    let here = tz::convert(there.and_time(t), z, zone);
                    if here.date() == d {
                        v.push(Occ {
                            item,
                            time: Some(here.time()),
                        });
                    }
                }
            }
            _ if occurs_on(item, d) => v.push(Occ {
                item,
                time: item.time,
            }),
            _ => {}
        }
    }
    v.sort_by_key(|o| {
        (
            o.item.kind != ItemKind::Holiday,
            o.time.is_none(),
            o.time,
            o.item.id,
        )
    });
    v
}

pub fn holidays_between(items: &[Item], from: NaiveDate, to: NaiveDate) -> HashSet<NaiveDate> {
    let hols: Vec<&Item> = items
        .iter()
        .filter(|i| i.kind == ItemKind::Holiday)
        .collect();
    let mut out = HashSet::new();
    if hols.is_empty() {
        return out;
    }
    let mut d = from;
    while d <= to {
        if hols.iter().any(|i| occurs_on(i, d)) {
            out.insert(d);
        }
        d = d.succ_opt().unwrap();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn item(recur: Recur, date: NaiveDate) -> Item {
        Item {
            recur,
            ..Item::new(ItemKind::Appointment, "x", date)
        }
    }

    #[test]
    fn single() {
        let i = item(Recur::None, d(2026, 11, 16));
        assert!(occurs_on(&i, d(2026, 11, 16)));
        assert!(!occurs_on(&i, d(2026, 11, 17)));
    }

    #[test]
    fn weekly_and_until() {
        let mut i = item(Recur::Weekly, d(2026, 11, 16));
        assert!(occurs_on(&i, d(2026, 11, 23)));
        assert!(!occurs_on(&i, d(2026, 11, 24)));
        assert!(
            !occurs_on(&i, d(2026, 11, 9)),
            "never before the start date"
        );
        i.recur_until = Some(d(2026, 11, 20));
        assert!(!occurs_on(&i, d(2026, 11, 23)));
    }

    #[test]
    fn monthly_skips_short_months() {
        let i = item(Recur::Monthly, d(2026, 1, 31));
        assert!(occurs_on(&i, d(2026, 3, 31)));
        assert!(!occurs_on(&i, d(2026, 2, 28)));
    }

    #[test]
    fn yearly_leap_day() {
        let i = item(Recur::Yearly, d(2024, 2, 29));
        assert!(occurs_on(&i, d(2028, 2, 29)));
        assert!(!occurs_on(&i, d(2027, 2, 28)));
    }

    #[test]
    fn zoned_items_land_on_the_local_day() {
        let berlin = chrono_tz::Europe::Berlin;
        // Monday 9 PM Eastern, weekly: Tuesday 3 AM in Berlin.
        let mut club = item(Recur::Weekly, d(2026, 11, 16));
        club.time = NaiveTime::from_hms_opt(21, 0, 0);
        club.tz = Some("America/New_York".into());
        let mut plain = item(Recur::None, d(2026, 11, 17));
        plain.id = 2;
        let mut early = item(Recur::None, d(2026, 11, 17));
        early.time = NaiveTime::from_hms_opt(1, 0, 0);
        early.id = 3;
        let items = [club, plain, early];

        assert!(occurrences_in(&items, d(2026, 11, 16), &berlin).is_empty());
        let tue = occurrences_in(&items, d(2026, 11, 17), &berlin);
        let got: Vec<_> = tue.iter().map(|o| (o.item.id, o.time)).collect();
        assert_eq!(
            got,
            [
                (3, NaiveTime::from_hms_opt(1, 0, 0)),
                (0, NaiveTime::from_hms_opt(3, 0, 0)),
                (2, None)
            ],
            "sorted by local time, untimed last"
        );
        assert_eq!(occurrences_in(&items, d(2026, 11, 24), &berlin).len(), 1);
        // In its own zone it stays on Monday.
        let ny = chrono_tz::America::New_York;
        assert_eq!(occurrences_in(&items, d(2026, 11, 16), &ny).len(), 1);
    }

    #[test]
    fn holiday_set() {
        let mut h = item(Recur::Yearly, d(2020, 12, 25));
        h.kind = ItemKind::Holiday;
        let set = holidays_between(&[h], d(2026, 12, 1), d(2027, 12, 31));
        assert_eq!(set.len(), 2);
        assert!(set.contains(&d(2026, 12, 25)));
    }
}
