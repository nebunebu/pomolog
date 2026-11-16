//! Recurrence of calendar items.

use super::model::{Item, ItemKind, Recur};
use chrono::{Datelike, NaiveDate};
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

/// Items occurring on `d`, timed ones first in time order.
pub fn on_day(items: &[Item], d: NaiveDate) -> Vec<&Item> {
    let mut v: Vec<&Item> = items.iter().filter(|i| occurs_on(i, d)).collect();
    v.sort_by_key(|i| (i.kind != ItemKind::Holiday, i.time.is_none(), i.time, i.id));
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
    fn holiday_set() {
        let mut h = item(Recur::Yearly, d(2020, 12, 25));
        h.kind = ItemKind::Holiday;
        let set = holidays_between(&[h], d(2026, 12, 1), d(2027, 12, 31));
        assert_eq!(set.len(), 2);
        assert!(set.contains(&d(2026, 12, 25)));
    }
}
