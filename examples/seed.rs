//! Fills a database with a couple of months of plausible data, for trying
//! the app out or taking screenshots.
//!
//!     POMOLOG_DATA_DIR=/tmp/demo cargo run --example seed
//!     POMOLOG_DATA_DIR=/tmp/demo cargo run
//!
//! It refuses to touch a database that already has sessions in it.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Weekday};
use pomolog::config;
use pomolog::core::db::Db;
use pomolog::core::ics;
use pomolog::core::model::*;

/// A small deterministic generator, so every run gives the same data.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = config::db_path();
    let db = Db::open(&path)?;
    let today = Local::now().date_naive();
    if !db
        .sessions_between(today - Duration::days(4000), today)?
        .is_empty()
    {
        return Err(format!("{} already has sessions in it; not seeding", path.display()).into());
    }
    let mut rng = Lcg(42);
    let at = |d: NaiveDate, h: u32, m: u32| d.and_hms_opt(h, m, 0).unwrap();

    // ---- boards and cards ----
    for b in db.boards()? {
        db.delete_board(b.id)?;
    }
    let work = db.add_board("Work")?;
    let home = db.add_board("Personal")?;
    let w = db.lists(work)?;
    let (todo, doing, done) = (w[0].id, w[1].id, w[2].id);
    let backlog = db.add_list(work, "Backlog")?;
    let h = db.lists(home)?;

    let card =
        |list: Id, title: &str, tags: &str, est: f64, due: Option<i64>| -> rusqlite::Result<Id> {
            let id = db.add_card(list, title, at(today - Duration::days(30), 9, 0))?;
            let mut c = db.card(id)?.unwrap();
            c.tags = tags.into();
            c.estimate = est;
            c.due = due.map(|d| today + Duration::days(d));
            db.update_card(&c)?;
            Ok(id)
        };
    let report = card(
        doing,
        "Write the quarterly report",
        "writing, deep",
        6.0,
        Some(2),
    )?;
    let review = card(doing, "Review the storage design", "review", 3.0, None)?;
    let bug = card(doing, "Fix the flaky sync test", "code", 2.0, Some(-1))?;
    card(todo, "Plan next sprint", "planning", 2.0, Some(4))?;
    card(todo, "Reply to the vendor", "email", 1.0, Some(0))?;
    card(todo, "Draft the conference talk", "writing", 8.0, Some(12))?;
    card(backlog, "Try the new profiler", "code", 0.0, None)?;
    card(backlog, "Clean up the wiki", "", 3.0, None)?;
    let shipped = [
        card(todo, "Migrate the CI runners", "code, infra", 4.0, None)?,
        card(todo, "Interview notes", "writing", 1.0, None)?,
        card(todo, "Benchmark the parser", "code", 3.0, None)?,
    ];
    let taxes = card(h[1].id, "Sort out the tax papers", "admin", 3.0, Some(6))?;
    card(h[0].id, "Book the dentist", "", 0.0, Some(1))?;
    card(h[0].id, "Fix the bike light", "", 1.0, None)?;

    // ---- sessions over the last 70 days ----
    let pool = [
        report, report, review, bug, shipped[0], shipped[1], shipped[2], taxes,
    ];
    let reasons = [
        "phone",
        "colleague",
        "slack",
        "coffee",
        "",
        "email",
        "phone",
    ];
    for back in (0..70).rev() {
        let day = today - Duration::days(back);
        let weekend = matches!(day.weekday(), Weekday::Sat | Weekday::Sun);
        // Fewer sessions at weekends, the odd day off, a slow build-up.
        let mut n = if weekend {
            rng.below(3)
        } else {
            3 + rng.below(7)
        };
        if rng.chance(8) {
            n = 0;
        }
        if back == 0 {
            n = 3;
        }
        let mut start =
            at(day, if weekend { 10 } else { 8 }, 30) + Duration::minutes(rng.below(50) as i64);
        for i in 0..n {
            let abandoned = rng.chance(9);
            let secs = if abandoned {
                240 + rng.below(700) as i64
            } else {
                1500
            };
            let mut interruptions = Vec::new();
            if rng.chance(22) {
                interruptions.push(Interruption {
                    at: start + Duration::minutes(8),
                    kind: "pause".into(),
                    reason: reasons[rng.below(reasons.len() as u64) as usize].into(),
                });
            }
            if abandoned {
                interruptions.push(Interruption {
                    at: start + Duration::seconds(secs),
                    kind: "abandon".into(),
                    reason: reasons[rng.below(reasons.len() as u64) as usize].into(),
                });
            }
            let card_id = if rng.chance(12) {
                None
            } else {
                Some(pool[rng.below(pool.len() as u64) as usize])
            };
            db.insert_session(&SessionRecord {
                kind: Phase::Focus,
                card_id,
                started_at: start,
                ended_at: start + Duration::seconds(secs),
                duration_s: secs,
                planned_s: 1500,
                completed: !abandoned,
                interruptions,
            })?;
            start += Duration::seconds(secs);
            let long = (i + 1) % 4 == 0;
            let brk = if long { 900 } else { 300 };
            db.insert_session(&SessionRecord {
                kind: if long {
                    Phase::LongBreak
                } else {
                    Phase::ShortBreak
                },
                card_id: None,
                started_at: start,
                ended_at: start + Duration::seconds(brk),
                duration_s: brk,
                planned_s: brk,
                completed: true,
                interruptions: Vec::new(),
            })?;
            start += Duration::seconds(brk) + Duration::minutes(rng.below(25) as i64);
            // A lunch gap.
            if i == 3 {
                start += Duration::minutes(50);
            }
        }
    }
    for (i, id) in shipped.iter().enumerate() {
        db.move_card(
            *id,
            done,
            0,
            at(today - Duration::days(3 + i as i64 * 6), 16, 0),
        )?;
    }

    // ---- calendar ----
    let year = today.year();
    let holidays = format!(
        "BEGIN:VCALENDAR\r\n\
         BEGIN:VEVENT\r\nUID:newyear\r\nDTSTART;VALUE=DATE:{y}0101\r\nSUMMARY:New Year's Day\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:labour\r\nDTSTART;VALUE=DATE:{y}0501\r\nSUMMARY:Labour Day\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:xmas\r\nDTSTART;VALUE=DATE:{y}1225\r\nSUMMARY:Christmas Day\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:boxing\r\nDTSTART;VALUE=DATE:{y}1226\r\nSUMMARY:Boxing Day\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\n\
         END:VCALENDAR\r\n",
        y = year - 1
    );
    ics::import(&db, &holidays, ItemKind::Holiday, 1)?;
    db.save_item(&Item::new(
        ItemKind::Holiday,
        "Day off",
        today + Duration::days(5),
    ))?;
    db.save_item(&Item::new(
        ItemKind::Holiday,
        "Bridge day",
        today - Duration::days(9),
    ))?;

    let appt = |title: &str, date: NaiveDate, h: u32, m: u32, mins: i64, recur: Recur| Item {
        time: NaiveTime::from_hms_opt(h, m, 0),
        duration_min: Some(mins),
        recur,
        ..Item::new(ItemKind::Appointment, title, date)
    };
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64 + 28);
    db.save_item(&appt("Team standup", monday, 9, 30, 15, Recur::Weekly))?;
    db.save_item(&appt(
        "1:1 with Sam",
        monday + Duration::days(3),
        14,
        0,
        30,
        Recur::Weekly,
    ))?;
    db.save_item(&appt(
        "Dentist",
        today + Duration::days(3),
        11,
        15,
        45,
        Recur::None,
    ))?;
    db.save_item(&appt("Lunch with Noor", today, 12, 30, 60, Recur::None))?;
    db.save_item(&appt(
        "Rent",
        today.with_day(1).unwrap() - Duration::days(62),
        8,
        0,
        5,
        Recur::Monthly,
    ))?;

    db.save_item(&Item::new(
        ItemKind::Deadline,
        "Submit expenses",
        today + Duration::days(1),
    ))?;
    db.save_item(&Item::new(
        ItemKind::Deadline,
        "Renew the passport",
        today - Duration::days(2),
    ))?;
    db.save_item(&Item {
        done: true,
        ..Item::new(
            ItemKind::Deadline,
            "Send the invoice",
            today - Duration::days(4),
        )
    })?;

    // A second calendar, with an item written in another time zone, and a note.
    let friends = db.save_calendar(&Calendar {
        color: Some("#8e6fe0".into()),
        ..Calendar::new("Friends")
    })?;
    db.save_item(&Item {
        time: NaiveTime::from_hms_opt(21, 0, 0),
        duration_min: Some(90),
        recur: Recur::Weekly,
        calendar_id: friends,
        tz: Some("America/New_York".into()),
        ..Item::new(ItemKind::Appointment, "Book club", today)
    })?;
    db.save_item(&Item {
        notes: "Gate code 4411.\nAsk for Sam at the desk.".into(),
        ..Item::new(
            ItemKind::Note,
            "Visiting the studio",
            today + Duration::days(1),
        )
    })?;

    // ---- habits ----
    let since = today - Duration::days(60);
    let mut read = Habit::new("Read", since);
    read.rest_on_holidays = true;
    let read = db.save_habit(&read)?;

    let mut pages = Habit::new("Write morning pages", since);
    pages.kind = HabitKind::Quantity;
    pages.target = 3.0;
    pages.unit = "pages".into();
    pages.schedule = Schedule::Weekdays(vec![0, 1, 2, 3, 4]);
    let pages = db.save_habit(&pages)?;

    let mut gym = Habit::new("Exercise", since);
    gym.schedule = Schedule::PerWeek(3);
    let gym = db.save_habit(&gym)?;

    let mut deep = Habit::new("Deep writing", since);
    deep.kind = HabitKind::Focus;
    deep.target = 50.0;
    deep.link_tag = Some("writing".into());
    deep.schedule = Schedule::Weekdays(vec![0, 1, 2, 3, 4]);
    db.save_habit(&deep)?;

    for back in (1..=60).rev() {
        let day = today - Duration::days(back);
        let wd = day.weekday().num_days_from_monday();
        // Reading: a long chain with one skip and one old break.
        if back == 41 {
            // missed
        } else if back == 17 {
            db.set_habit_entry(
                read,
                day,
                HabitEntry {
                    value: 0.0,
                    skipped: true,
                },
            )?;
        } else {
            db.set_habit_entry(
                read,
                day,
                HabitEntry {
                    value: 1.0,
                    skipped: false,
                },
            )?;
        }
        if wd < 5 {
            let v = if back % 11 == 0 {
                1.0
            } else {
                3.0 + rng.below(2) as f64
            };
            db.set_habit_entry(
                pages,
                day,
                HabitEntry {
                    value: v,
                    skipped: false,
                },
            )?;
        }
        if matches!(wd, 0 | 2 | 5) && !(22..29).contains(&back) || (back == 24) {
            db.set_habit_entry(
                gym,
                day,
                HabitEntry {
                    value: 1.0,
                    skipped: false,
                },
            )?;
        }
    }
    db.set_habit_entry(
        pages,
        today,
        HabitEntry {
            value: 2.0,
            skipped: false,
        },
    )?;

    println!("seeded {}", path.display());
    Ok(())
}
