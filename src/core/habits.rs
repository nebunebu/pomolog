//! Don't-break-the-chain maths.
//!
//! Rules:
//! - A scheduled day that is done extends the chain.
//! - A day that is not scheduled (wrong weekday, or a holiday when the habit
//!   rests on holidays) never breaks it.
//! - A skipped day keeps the chain alive without adding to it.
//! - Today never breaks the chain until it is over.
//! - For "N times a week" habits the chain is counted in weeks.

use super::model::{Habit, HabitEntry, Schedule};
use chrono::{Datelike, Duration, NaiveDate};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayState {
    /// Before the habit existed, or in the future.
    Outside,
    Done,
    /// Scheduled, in the past, not done.
    Missed,
    /// Explicitly skipped.
    Skipped,
    /// Not scheduled on this day.
    Rest,
    /// Scheduled today and not done yet.
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Streak {
    pub current: u32,
    pub longest: u32,
    /// True when the chain is counted in weeks rather than days.
    pub weeks: bool,
    /// Today (or this week) still needs doing to keep the chain.
    pub at_risk: bool,
}

impl Streak {
    pub fn unit(&self, n: u32) -> &'static str {
        match (self.weeks, n == 1) {
            (true, true) => "week",
            (true, false) => "weeks",
            (false, true) => "day",
            (false, false) => "days",
        }
    }
}

pub struct Ctx<'a> {
    pub entries: &'a HashMap<NaiveDate, HabitEntry>,
    pub holidays: &'a HashSet<NaiveDate>,
    pub today: NaiveDate,
}

pub fn is_done(h: &Habit, e: Option<&HabitEntry>) -> bool {
    e.is_some_and(|e| e.value >= h.target.max(f64::MIN_POSITIVE))
}

fn scheduled(h: &Habit, d: NaiveDate, holidays: &HashSet<NaiveDate>) -> bool {
    if h.rest_on_holidays && holidays.contains(&d) {
        return false;
    }
    match &h.schedule {
        Schedule::Daily | Schedule::PerWeek(_) => true,
        Schedule::Weekdays(days) => days.contains(&(d.weekday().num_days_from_monday() as u8)),
    }
}

pub fn is_scheduled(h: &Habit, d: NaiveDate, holidays: &HashSet<NaiveDate>) -> bool {
    d >= h.created && scheduled(h, d, holidays)
}

pub fn day_state(h: &Habit, d: NaiveDate, cx: &Ctx) -> DayState {
    if d < h.created || d > cx.today {
        return DayState::Outside;
    }
    let e = cx.entries.get(&d);
    if is_done(h, e) {
        return DayState::Done;
    }
    if e.is_some_and(|e| e.skipped) {
        return DayState::Skipped;
    }
    if !scheduled(h, d, cx.holidays) {
        return DayState::Rest;
    }
    if let Schedule::PerWeek(n) = h.schedule {
        // Individual days are optional; the open days of a past week that
        // fell short show as missed.
        let (mon, sun) = week_of(d);
        if sun >= cx.today {
            return if week_met(h, mon, n, cx) {
                DayState::Rest
            } else {
                DayState::Pending
            };
        }
        return if week_met(h, mon, n, cx) {
            DayState::Rest
        } else {
            DayState::Missed
        };
    }
    if d == cx.today {
        DayState::Pending
    } else {
        DayState::Missed
    }
}

fn week_of(d: NaiveDate) -> (NaiveDate, NaiveDate) {
    let mon = d - Duration::days(d.weekday().num_days_from_monday() as i64);
    (mon, mon + Duration::days(6))
}

fn week_counts(h: &Habit, mon: NaiveDate, cx: &Ctx) -> (u32, u32, u32) {
    let (mut done, mut skipped, mut rest) = (0, 0, 0);
    for i in 0..7 {
        let d = mon + Duration::days(i);
        let e = cx.entries.get(&d);
        if is_done(h, e) {
            done += 1;
        } else if e.is_some_and(|e| e.skipped) {
            skipped += 1;
        } else if d < h.created || !scheduled(h, d, cx.holidays) {
            rest += 1;
        }
    }
    (done, skipped, rest)
}

fn week_met(h: &Habit, mon: NaiveDate, n: u8, cx: &Ctx) -> bool {
    let (done, skipped, rest) = week_counts(h, mon, cx);
    // Skips lower the bar; so do days that could not be used at all.
    let available = 7 - rest;
    let need = (n as u32).min(available).saturating_sub(skipped);
    done >= need
}

pub fn streak(h: &Habit, cx: &Ctx) -> Streak {
    match h.schedule {
        Schedule::PerWeek(n) => weekly_streak(h, n, cx),
        _ => daily_streak(h, cx),
    }
}

fn daily_streak(h: &Habit, cx: &Ctx) -> Streak {
    let mut s = Streak::default();
    let mut run = 0u32;
    let mut d = h.created.min(cx.today);
    while d <= cx.today {
        match day_state(h, d, cx) {
            DayState::Done => {
                run += 1;
                s.longest = s.longest.max(run);
            }
            DayState::Missed => run = 0,
            DayState::Pending => s.at_risk = true,
            DayState::Skipped | DayState::Rest | DayState::Outside => {}
        }
        d = d.succ_opt().unwrap();
    }
    s.current = run;
    s
}

fn weekly_streak(h: &Habit, n: u8, cx: &Ctx) -> Streak {
    let mut s = Streak {
        weeks: true,
        ..Default::default()
    };
    let mut run = 0u32;
    let (mut mon, _) = week_of(h.created.min(cx.today));
    let (this_mon, _) = week_of(cx.today);
    while mon <= this_mon {
        let met = week_met(h, mon, n, cx);
        if met {
            run += 1;
            s.longest = s.longest.max(run);
        } else if mon == this_mon {
            s.at_risk = true;
        } else {
            run = 0;
        }
        mon += Duration::days(7);
    }
    s.current = run;
    s
}

/// Skips already used in the calendar month containing `d`.
pub fn skips_used(entries: &HashMap<NaiveDate, HabitEntry>, d: NaiveDate) -> u32 {
    entries
        .iter()
        .filter(|(k, e)| e.skipped && k.year() == d.year() && k.month() == d.month())
        .count() as u32
}

/// Share of scheduled days that were done in the range, 0.0 to 1.0.
pub fn completion_rate(h: &Habit, from: NaiveDate, to: NaiveDate, cx: &Ctx) -> Option<f32> {
    let (mut done, mut due) = (0u32, 0u32);
    let mut d = from.max(h.created);
    while d <= to.min(cx.today) {
        match day_state(h, d, cx) {
            DayState::Done => {
                done += 1;
                due += 1;
            }
            DayState::Missed => due += 1,
            _ => {}
        }
        d = d.succ_opt().unwrap();
    }
    (due > 0).then(|| done as f32 / due as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        // November 2026: the 2nd is a Monday.
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    fn done(days: &[u32]) -> HashMap<NaiveDate, HabitEntry> {
        days.iter()
            .map(|&x| {
                (
                    d(x),
                    HabitEntry {
                        value: 1.0,
                        skipped: false,
                    },
                )
            })
            .collect()
    }

    fn run(h: &Habit, e: &HashMap<NaiveDate, HabitEntry>, hol: &[u32], today: u32) -> Streak {
        let holidays: HashSet<NaiveDate> = hol.iter().map(|&x| d(x)).collect();
        streak(
            h,
            &Ctx {
                entries: e,
                holidays: &holidays,
                today: d(today),
            },
        )
    }

    fn habit(schedule: Schedule) -> Habit {
        Habit {
            schedule,
            ..Habit::new("h", d(2))
        }
    }

    #[test]
    fn daily_chain_counts_and_breaks() {
        let h = habit(Schedule::Daily);
        let s = run(&h, &done(&[2, 3, 4, 6, 7]), &[], 7);
        assert_eq!((s.current, s.longest), (2, 3));
        assert!(!s.at_risk);
    }

    #[test]
    fn today_pending_does_not_break() {
        let h = habit(Schedule::Daily);
        let s = run(&h, &done(&[2, 3, 4]), &[], 5);
        assert_eq!(s.current, 3);
        assert!(s.at_risk);
    }

    #[test]
    fn unscheduled_days_do_not_break() {
        // Monday, Wednesday, Friday.
        let h = habit(Schedule::Weekdays(vec![0, 2, 4]));
        let s = run(&h, &done(&[2, 4, 6, 9]), &[], 10);
        assert_eq!((s.current, s.longest), (4, 4));
        let s = run(&h, &done(&[2, 6, 9]), &[], 10);
        assert_eq!(
            (s.current, s.longest),
            (2, 2),
            "missing Wednesday the 4th breaks it"
        );
    }

    #[test]
    fn skip_keeps_chain_without_adding() {
        let h = habit(Schedule::Daily);
        let mut e = done(&[2, 3, 5]);
        e.insert(
            d(4),
            HabitEntry {
                value: 0.0,
                skipped: true,
            },
        );
        let s = run(&h, &e, &[], 5);
        assert_eq!(s.current, 3);
        assert_eq!(skips_used(&e, d(20)), 1);
    }

    #[test]
    fn holiday_rest_days() {
        let mut h = habit(Schedule::Daily);
        let e = done(&[2, 3, 5]);
        assert_eq!(
            run(&h, &e, &[4], 5).current,
            1,
            "holiday ignored unless opted in"
        );
        h.rest_on_holidays = true;
        assert_eq!(run(&h, &e, &[4], 5).current, 3);
    }

    #[test]
    fn quantity_needs_target() {
        let mut h = habit(Schedule::Daily);
        h.target = 20.0;
        let mut e = HashMap::new();
        e.insert(
            d(2),
            HabitEntry {
                value: 20.0,
                skipped: false,
            },
        );
        e.insert(
            d(3),
            HabitEntry {
                value: 12.0,
                skipped: false,
            },
        );
        let s = run(&h, &e, &[], 4);
        assert_eq!(s.current, 0);
        assert_eq!(s.longest, 1);
    }

    #[test]
    fn per_week_counts_weeks() {
        let h = habit(Schedule::PerWeek(3));
        // Week of the 2nd: 3 done. Week of the 9th: 3 done. Week of the 16th: 1 so far.
        let s = run(&h, &done(&[2, 4, 7, 9, 10, 15, 16]), &[], 17);
        assert!(s.weeks);
        assert_eq!((s.current, s.longest), (2, 2));
        assert!(s.at_risk);
        // A failed middle week resets.
        let s = run(&h, &done(&[2, 4, 7, 9, 16, 17, 18]), &[], 18);
        assert_eq!((s.current, s.longest), (1, 1));
    }

    #[test]
    fn per_week_day_states() {
        let h = habit(Schedule::PerWeek(2));
        let e = done(&[2]);
        let cx = Ctx {
            entries: &e,
            holidays: &HashSet::new(),
            today: d(12),
        };
        assert_eq!(day_state(&h, d(2), &cx), DayState::Done);
        assert_eq!(
            day_state(&h, d(5), &cx),
            DayState::Missed,
            "week fell short"
        );
        assert_eq!(day_state(&h, d(11), &cx), DayState::Pending);
        assert_eq!(day_state(&h, d(13), &cx), DayState::Outside);
    }

    #[test]
    fn rate() {
        let h = habit(Schedule::Daily);
        let e = done(&[2, 3]);
        let cx = Ctx {
            entries: &e,
            holidays: &HashSet::new(),
            today: d(6),
        };
        // Done 2, missed 4 and 5, today pending is not counted.
        assert_eq!(completion_rate(&h, d(1), d(30), &cx), Some(0.5));
    }
}
