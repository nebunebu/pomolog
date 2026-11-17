//! The command-line commands that work on data. They write straight to the
//! database, whether or not the app is running: a running app notices the
//! change by itself, so it never has to know a command to honour it.

use crate::core::db::Db;
use crate::core::intake::{self, Added, Intake};
use crate::core::model::{Calendar, DEFAULT_CALENDAR, Id, Item, ItemKind, ItemText, Recur};
use crate::core::{dates, ics, recur, tz};
use chrono::{Duration, NaiveDate, NaiveDateTime};
use serde_json::json;

type Out = Result<String, String>;

fn db_err(e: rusqlite::Error) -> String {
    format!("database error: {e}")
}

fn calendar_name(calendars: &[Calendar], id: Id) -> &str {
    calendars
        .iter()
        .find(|c| c.id == id)
        .map_or("?", |c| c.name.as_str())
}

/// `Tue 5 Jan 2027 21:00 (America/New_York)`.
fn when(item: &Item, twelve_hour: bool) -> String {
    let mut s = item.date.format("%a %-d %b %Y").to_string();
    if let Some(t) = item.time {
        s = format!("{s} {}", dates::fmt_time(t, twelve_hour));
    }
    if let Some(z) = &item.tz {
        s = format!("{s} ({z})");
    }
    s
}

pub fn add_card(
    db: &Db,
    title: &str,
    due: Option<&str>,
    board: Option<&str>,
    tags: &[String],
    now: NaiveDateTime,
) -> Out {
    let due = match due {
        Some(text) => Some(dates::parse_date(text, now.date()).ok_or_else(|| {
            format!(
                "could not read the due date \"{text}\" (try 2026-12-01, tomorrow, fri, +3d, Jan 5)"
            )
        })?),
        None => None,
    };
    let (_, place) = intake::add_card(db, title, due, board, tags, now)?;
    Ok(format!("added to {place}"))
}

pub fn add_event(
    db: &Db,
    text: &ItemText,
    allow_duplicate: bool,
    today: NaiveDate,
    twelve_hour: bool,
) -> Out {
    Ok(match intake::add_item(db, text, today, allow_duplicate)? {
        Added::New(i) => format!(
            "added \"{}\" on {} [#{}]",
            i.title,
            when(&i, twelve_hour),
            i.id
        ),
        Added::Updated(i) => {
            format!(
                "updated \"{}\": {} [#{}]",
                i.title,
                when(&i, twelve_hour),
                i.id
            )
        }
        Added::Duplicate(i) => format!(
            "\"{}\" on {} is already there [#{}]; nothing added (--allow-duplicate adds it anyway)",
            i.title,
            when(&i, twelve_hour),
            i.id
        ),
    })
}

/// What a free line of text describes, added. See `intake::parse`.
pub fn intake_line(db: &Db, text: &str, now: NaiveDateTime, twelve_hour: bool) -> Out {
    match intake::parse(text, now.date())? {
        Intake::Card {
            title,
            due,
            board,
            tags,
        } => {
            let (_, place) = intake::add_card(db, &title, due, board.as_deref(), &tags, now)?;
            Ok(match due {
                Some(d) => format!("added card \"{title}\" to {place}, due {d}"),
                None => format!("added card \"{title}\" to {place}"),
            })
        }
        Intake::Item(t) => add_event(db, &t, false, now.date(), twelve_hour),
    }
}

/// Items between two dates (today and the next 30 days by default), one line
/// per occurrence, in local time.
pub fn list_events(
    db: &Db,
    from: Option<&str>,
    to: Option<&str>,
    calendar: Option<&str>,
    as_json: bool,
    today: NaiveDate,
    twelve_hour: bool,
) -> Out {
    let date = |text: Option<&str>, fallback: NaiveDate| match text {
        Some(t) => {
            dates::parse_date(t, today).ok_or_else(|| format!("could not read the date \"{t}\""))
        }
        None => Ok(fallback),
    };
    let from = date(from, today)?;
    let to = date(to, from + Duration::days(30))?;
    if to < from {
        return Err("the end date is before the start".into());
    }
    if (to - from).num_days() > 3660 {
        return Err("that is more than ten years; narrow the range".into());
    }
    let calendars = db.calendars().map_err(db_err)?;
    let only = match calendar {
        Some(name) => Some(
            calendars
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| format!("there is no calendar called \"{name}\""))?
                .id,
        ),
        None => None,
    };
    let items: Vec<Item> = db
        .items()
        .map_err(db_err)?
        .into_iter()
        .filter(|i| only.is_none_or(|c| i.calendar_id == c))
        .collect();

    let mut lines = Vec::new();
    let mut rows = Vec::new();
    let mut d = from;
    while d <= to {
        for o in recur::occurrences_on(&items, d) {
            let i = o.item;
            let cal = calendar_name(&calendars, i.calendar_id);
            if as_json {
                rows.push(json!({
                    "id": i.id,
                    "date": d.to_string(),
                    "time": o.time.map(|t| t.format("%H:%M").to_string()),
                    "duration": i.duration_min,
                    "title": i.title,
                    "kind": i.kind.as_str(),
                    "calendar": cal,
                    "tz": i.tz,
                    "repeat": i.recur.as_str(),
                    "done": i.done,
                    "notes": i.notes,
                    "source": i.source,
                }));
                continue;
            }
            let time = match (o.time, i.duration_min) {
                (Some(t), Some(m)) => format!(
                    "{}–{}",
                    dates::fmt_time(t, twelve_hour),
                    dates::fmt_time(t + Duration::minutes(m), twelve_hour)
                ),
                (Some(t), None) => dates::fmt_time(t, twelve_hour),
                (None, _) => "all day".into(),
            };
            let mut tail = format!("#{} {} · {cal}", i.id, i.kind.as_str());
            if i.recur != Recur::None {
                tail = format!("{tail} · {}", i.recur.as_str());
            }
            if i.kind == ItemKind::Deadline && i.done {
                tail = format!("{tail} · done");
            }
            lines.push(format!(
                "{}  {time:<13}  {}  [{tail}]",
                d.format("%Y-%m-%d %a"),
                i.title
            ));
        }
        d += Duration::days(1);
    }
    if as_json {
        return serde_json::to_string_pretty(&rows).map_err(|e| e.to_string());
    }
    if lines.is_empty() {
        return Ok(format!("nothing between {from} and {to}"));
    }
    Ok(lines.join("\n"))
}

pub fn edit_event(db: &Db, id: Id, changes: &ItemText, today: NaiveDate, twelve_hour: bool) -> Out {
    let mut item = db
        .item(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("there is no item #{id} (see `pomolog event list`)"))?;
    let before = item.clone();
    let calendars = db.calendars().map_err(db_err)?;
    changes.apply(&mut item, today, &calendars)?;
    if item == before {
        return Ok(format!("#{id} is unchanged"));
    }
    db.save_item(&item).map_err(db_err)?;
    Ok(format!(
        "updated \"{}\": {} [#{id}]",
        item.title,
        when(&item, twelve_hour)
    ))
}

pub fn remove_event(db: &Db, id: Id) -> Out {
    let item = db
        .item(id)
        .map_err(db_err)?
        .ok_or_else(|| format!("there is no item #{id} (see `pomolog event list`)"))?;
    db.delete_item(id).map_err(db_err)?;
    let every = if item.recur == Recur::None {
        ""
    } else {
        " and all its repeats"
    };
    Ok(format!("removed \"{}\"{every} [#{id}]", item.title))
}

/// Adds many items at once. `text` is an `.ics` calendar, or one JSON object
/// per line (or a JSON array of them) with the fields of `pomolog event`:
/// `{"title": "Club", "date": "Jan 5", "time": "9pm"}`. `defaults` fills in
/// what a row leaves out. Running the same import again adds nothing.
pub fn import_events(
    db: &Db,
    text: &str,
    defaults: &ItemText,
    allow_duplicate: bool,
    today: NaiveDate,
) -> Out {
    if text.to_ascii_uppercase().contains("BEGIN:VCALENDAR") {
        let calendars = db.calendars().map_err(db_err)?;
        let probe = ItemText {
            title: "x".into(),
            date: "today".into(),
            kind: defaults.kind.clone(),
            calendar: defaults.calendar.clone(),
            ..Default::default()
        }
        .build(today, &calendars)?;
        let (added, skipped) =
            ics::import(db, text, probe.kind, probe.calendar_id).map_err(db_err)?;
        return Ok(format!(
            "imported {added} event(s); {skipped} were already there"
        ));
    }
    let trimmed = text.trim_start();
    let rows: Vec<(usize, Result<ItemText, String>)> = if trimmed.starts_with('[') {
        serde_json::from_str::<Vec<ItemText>>(trimmed)
            .map_err(|e| format!("could not read the JSON: {e}"))?
            .into_iter()
            .enumerate()
            .map(|(i, t)| (i + 1, Ok(t)))
            .collect()
    } else {
        text.lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
            .map(|(i, l)| {
                (
                    i + 1,
                    serde_json::from_str::<ItemText>(l).map_err(|e| e.to_string()),
                )
            })
            .collect()
    };
    if rows.is_empty() {
        return Err("there is nothing to import".into());
    }
    let (mut added, mut updated, mut skipped) = (0, 0, 0);
    let mut errors = Vec::new();
    for (n, row) in rows {
        let res = row.and_then(|t| intake::add_item(db, &t.or(defaults), today, allow_duplicate));
        match res {
            Ok(Added::New(_)) => added += 1,
            Ok(Added::Updated(_)) => updated += 1,
            Ok(Added::Duplicate(_)) => skipped += 1,
            Err(e) => errors.push(format!("  entry {n}: {e}")),
        }
    }
    let summary = format!("added {added}, updated {updated}, {skipped} already there");
    if errors.is_empty() {
        Ok(summary)
    } else {
        Err(format!(
            "{summary}; {} could not be read:\n{}",
            errors.len(),
            errors.join("\n")
        ))
    }
}

pub fn list_calendars(db: &Db) -> Out {
    let items = db.items().map_err(db_err)?;
    let lines: Vec<String> = db
        .calendars()
        .map_err(db_err)?
        .iter()
        .map(|c| {
            let n = items.iter().filter(|i| i.calendar_id == c.id).count();
            let mut line = format!("{}  ({n} item(s))", c.name);
            if let Some(z) = &c.tz {
                line = format!("{line}  {z}");
            }
            if let Some(col) = &c.color {
                line = format!("{line}  {col}");
            }
            if !c.visible {
                line = format!("{line}  hidden");
            }
            line
        })
        .collect();
    Ok(lines.join("\n"))
}

pub fn add_calendar(db: &Db, name: &str, color: Option<&str>, zone: Option<&str>) -> Out {
    let name = name.trim();
    if name.is_empty() {
        return Err("the name is empty".into());
    }
    let calendars = db.calendars().map_err(db_err)?;
    if calendars.iter().any(|c| c.name.eq_ignore_ascii_case(name)) {
        return Err(format!("there is already a calendar called \"{name}\""));
    }
    let color = match color {
        Some(c) => Some(
            crate::theme::Rgb::parse(c)
                .ok_or_else(|| format!("could not read the colour \"{c}\" (try #4a8fe7)"))?
                .to_hex(),
        ),
        None => None,
    };
    let zone = match zone {
        Some(z) => Some(tz::canonical(z).ok_or_else(|| {
            format!("unknown time zone \"{z}\" (try America/New_York, Eastern, UTC)")
        })?),
        None => None,
    };
    db.save_calendar(&Calendar {
        color,
        tz: zone,
        ..Calendar::new(name)
    })
    .map_err(db_err)?;
    Ok(format!("added the calendar \"{name}\""))
}

/// Removes a calendar. One that still has items needs `force`, since they go
/// with it.
pub fn remove_calendar(db: &mut Db, name: &str, force: bool) -> Out {
    let calendars = db.calendars().map_err(db_err)?;
    let cal = calendars
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| format!("there is no calendar called \"{name}\""))?;
    if cal.id == DEFAULT_CALENDAR {
        return Err(format!(
            "\"{}\" is the default calendar and stays",
            cal.name
        ));
    }
    let n = db
        .items()
        .map_err(db_err)?
        .iter()
        .filter(|i| i.calendar_id == cal.id)
        .count();
    if n > 0 && !force {
        return Err(format!(
            "\"{}\" still has {n} item(s); --force removes them with it",
            cal.name
        ));
    }
    db.delete_calendar(cal.id).map_err(db_err)?;
    Ok(format!(
        "removed the calendar \"{}\" and {n} item(s)",
        cal.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
    }

    fn text(title: &str, date: &str, time: Option<&str>) -> ItemText {
        ItemText {
            title: title.into(),
            date: date.into(),
            time: time.map(Into::into),
            ..Default::default()
        }
    }

    #[test]
    fn add_list_edit_remove() {
        let db = Db::open_in_memory().unwrap();
        let out = add_event(
            &db,
            &text("Club", "Nov 20", Some("9pm")),
            false,
            today(),
            false,
        );
        assert_eq!(out.unwrap(), "added \"Club\" on Fri 20 Nov 2026 21:00 [#1]");
        let again = add_event(
            &db,
            &text("Club", "Nov 20", Some("9pm")),
            false,
            today(),
            false,
        );
        assert!(again.unwrap().contains("already there"));
        assert_eq!(db.items().unwrap().len(), 1);

        let list = list_events(&db, None, None, None, false, today(), true).unwrap();
        assert_eq!(
            list,
            "2026-11-20 Fri  9:00 PM        Club  [#1 appointment · Personal]"
        );
        let rows: serde_json::Value = serde_json::from_str(
            &list_events(&db, None, None, None, true, today(), false).unwrap(),
        )
        .unwrap();
        assert_eq!(rows[0]["time"], "21:00");
        assert_eq!(rows[0]["calendar"], "Personal");
        assert!(
            list_events(
                &db,
                Some("Dec 1"),
                Some("Dec 2"),
                None,
                false,
                today(),
                false
            )
            .unwrap()
            .starts_with("nothing between")
        );
        assert!(list_events(&db, None, None, Some("nope"), false, today(), false).is_err());

        let changes = ItemText {
            time: Some("8pm".into()),
            duration: Some(90),
            recur: Some("weekly".into()),
            ..Default::default()
        };
        let out = edit_event(&db, 1, &changes, today(), false).unwrap();
        assert_eq!(out, "updated \"Club\": Fri 20 Nov 2026 20:00 [#1]");
        let item = db.item(1).unwrap().unwrap();
        assert_eq!((item.duration_min, item.recur), (Some(90), Recur::Weekly));
        assert_eq!(item.title, "Club", "fields that were not given stay");
        assert_eq!(
            edit_event(&db, 1, &ItemText::default(), today(), false).unwrap(),
            "#1 is unchanged"
        );
        assert!(edit_event(&db, 9, &changes, today(), false).is_err());

        assert!(remove_event(&db, 1).unwrap().contains("all its repeats"));
        assert!(remove_event(&db, 1).is_err());
    }

    #[test]
    fn bulk_import_is_repeatable() {
        let db = Db::open_in_memory().unwrap();
        let lines = r#"
# book club
{"title": "Club", "date": "Jan 5"}
{"title": "Club", "date": "Feb 2", "time": "8pm", "duration": 60}
{"title": "Dentist", "date": "1/12", "source": "dent-1"}
"#;
        let defaults = ItemText {
            time: Some("9:00 PM Eastern".into()),
            duration: Some(120),
            ..Default::default()
        };
        let out = import_events(&db, lines, &defaults, false, today()).unwrap();
        assert_eq!(out, "added 3, updated 0, 0 already there");
        let items = db.items().unwrap();
        assert_eq!(items[0].tz.as_deref(), Some("America/New_York"));
        assert_eq!(items[0].duration_min, Some(120), "the default");
        assert_eq!(items[2].duration_min, Some(60), "its own");
        let out = import_events(&db, lines, &defaults, false, today()).unwrap();
        assert_eq!(out, "added 0, updated 1, 2 already there");
        assert_eq!(db.items().unwrap().len(), 3);

        let array = r#"[{"title": "A", "date": "tomorrow"}, {"title": "", "date": "x"}]"#;
        let err = import_events(&db, array, &ItemText::default(), false, today()).unwrap_err();
        assert!(
            err.starts_with("added 1") && err.contains("entry 2"),
            "{err}"
        );
        assert!(import_events(&db, "\n", &defaults, false, today()).is_err());

        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nDTSTART:20261225\nSUMMARY:Xmas\nEND:VEVENT\nEND:VCALENDAR\n";
        let hol = ItemText {
            kind: Some("holiday".into()),
            ..Default::default()
        };
        let out = import_events(&db, ics, &hol, false, today()).unwrap();
        assert_eq!(out, "imported 1 event(s); 0 were already there");
        let items = db.items().unwrap();
        let xmas = items.iter().find(|i| i.title == "Xmas").unwrap();
        assert_eq!(xmas.kind, ItemKind::Holiday);
    }

    #[test]
    fn cards_notes_and_free_text() {
        let db = Db::open_in_memory().unwrap();
        let now = today().and_hms_opt(9, 0, 0).unwrap();
        assert_eq!(
            add_card(&db, "Report", Some("Jan 5"), None, &[], now).unwrap(),
            "added to Tasks / To do"
        );
        assert!(add_card(&db, "Report", Some("someday"), None, &[], now).is_err());
        assert!(
            intake_line(&db, "Dentist fri 2pm", now, false)
                .unwrap()
                .starts_with("added \"Dentist\" on Fri 20 Nov 2026 14:00")
        );
        assert_eq!(
            intake_line(&db, "Pay rent due fri", now, false).unwrap(),
            "added card \"Pay rent\" to Tasks / To do, due 2026-11-20"
        );
        intake_line(&db, "note: gate code 4411", now, false).unwrap();
        let note = db
            .items()
            .unwrap()
            .into_iter()
            .find(|i| i.kind == ItemKind::Note);
        assert_eq!(note.unwrap().date, today());
    }

    #[test]
    fn calendars() {
        let mut db = Db::open_in_memory().unwrap();
        add_calendar(&db, "Work", Some("#4a8fe7"), Some("eastern")).unwrap();
        assert!(add_calendar(&db, "work", None, None).is_err(), "same name");
        assert!(add_calendar(&db, "X", Some("blueish"), None).is_err());
        assert!(add_calendar(&db, "X", None, Some("Narnia")).is_err());
        let mut t = text("Standup", "tomorrow", Some("9:30"));
        t.calendar = Some("Work".into());
        add_event(&db, &t, false, today(), false).unwrap();
        let list = list_calendars(&db).unwrap();
        assert!(list.contains("Personal  (0 item(s))"));
        assert!(list.contains("Work  (1 item(s))  America/New_York"));
        assert!(remove_calendar(&mut db, "Personal", true).is_err());
        assert!(
            remove_calendar(&mut db, "Work", false).is_err(),
            "it has items"
        );
        remove_calendar(&mut db, "Work", true).unwrap();
        assert!(db.items().unwrap().is_empty());
    }
}
