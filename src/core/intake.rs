//! One way in for everything: a line of text becomes a card, an appointment,
//! a deadline or a note, and lands in the database. The command line and the
//! command palette both come through here.

use super::db::Db;
use super::model::{Id, Item, ItemText};
use super::{dates, tz};
use chrono::{NaiveDate, NaiveDateTime};

#[derive(Debug, Clone, PartialEq)]
pub enum Intake {
    Card {
        title: String,
        due: Option<NaiveDate>,
        board: Option<String>,
        tags: Vec<String>,
    },
    /// An appointment, deadline or note; `kind` says which.
    Item(ItemText),
}

impl Intake {
    /// What this will create, for showing before it is done.
    pub fn describe(&self) -> String {
        match self {
            Intake::Card { title, due, .. } => match due {
                Some(d) => format!("Card: {title} (due {d})"),
                None => format!("Card: {title}"),
            },
            Intake::Item(t) => {
                let kind = t.kind.as_deref().unwrap_or("appointment");
                let mut when = t.date.clone();
                if let Some(time) = &t.time {
                    when = format!("{when} {time}").trim().to_string();
                }
                let kind = kind[..1].to_uppercase() + &kind[1..];
                if when.is_empty() {
                    format!("{kind}: {}", t.title)
                } else {
                    format!("{kind}: {} ({when})", t.title)
                }
            }
        }
    }
}

fn is_clock(tok: &str) -> bool {
    let t = tok.to_ascii_lowercase();
    (t.contains(':') || t.ends_with("am") || t.ends_with("pm")) && dates::parse_time(tok).is_some()
}

fn minutes(tok: &str) -> Option<i64> {
    let t = tok.to_ascii_lowercase();
    let (num, per) = if let Some(n) = t.strip_suffix("min") {
        (n, 1)
    } else if let Some(n) = t.strip_suffix('m') {
        (n, 1)
    } else {
        (t.strip_suffix('h')?, 60)
    };
    num.parse::<i64>().ok().filter(|n| *n > 0).map(|n| n * per)
}

/// Reads a line such as `Dentist Jan 5 9pm for 45m`, `Report due fri @work
/// #writing` or `note: call the bank`.
///
/// - `note:` in front makes a note (for today unless a date is given);
/// - a time (`14:30`, `9pm`, `at 9`, optionally followed by a zone) makes an
///   appointment, today if there is no date;
/// - `due` or `by` before a date makes a card with that due date;
/// - a date alone makes a deadline;
/// - anything else is a card.
///
/// `@name` picks the board of a card or the calendar of an item, and `#tags`
/// go on a card.
pub fn parse(text: &str, today: NaiveDate) -> Result<Intake, String> {
    let mut rest = text.trim();
    let mut note = false;
    if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("note:") {
        note = true;
        rest = rest[5..].trim();
    }
    let mut toks: Vec<&str> = rest.split_whitespace().collect();

    let mut place = None;
    if let Some(i) = toks.iter().position(|t| t.len() > 1 && t.starts_with('@')) {
        place = Some(toks.remove(i)[1..].to_string());
    }

    // The date: the longest run of words that reads as one.
    let mut date = None;
    let mut due = false;
    'date: for len in (1..=3).rev() {
        for i in 0..toks.len().saturating_sub(len - 1) {
            let phrase = toks[i..i + len].join(" ");
            if dates::parse_date(&phrase, today).is_none() {
                continue;
            }
            let mut start = i;
            if i > 0 {
                match toks[i - 1].to_ascii_lowercase().as_str() {
                    "due" | "by" => {
                        due = true;
                        start -= 1;
                    }
                    "on" => start -= 1,
                    _ => {}
                }
            }
            toks.drain(start..i + len);
            date = Some(phrase);
            break 'date;
        }
    }

    // The time: something that looks like a clock, or a number after "at".
    let mut time = None;
    for i in 0..toks.len() {
        let after_at = i > 0 && toks[i - 1].eq_ignore_ascii_case("at");
        let two = (i + 1 < toks.len()).then(|| format!("{} {}", toks[i], toks[i + 1]));
        let len = if two.as_deref().is_some_and(is_clock) {
            2
        } else if is_clock(toks[i]) || (after_at && dates::parse_time(toks[i]).is_some()) {
            1
        } else {
            continue;
        };
        let mut end = i + len;
        let mut phrase = toks[i..end].join(" ");
        if let Some(zone) = toks.get(end).filter(|z| tz::parse(z).is_some()) {
            phrase = format!("{phrase} {zone}");
            end += 1;
        }
        toks.drain(if after_at { i - 1 } else { i }..end);
        time = Some(phrase);
        break;
    }

    let mut duration = None;
    if time.is_some()
        && let Some(i) = toks.iter().position(|t| t.eq_ignore_ascii_case("for"))
        && let Some(m) = toks.get(i + 1).and_then(|t| minutes(t))
    {
        duration = Some(m);
        toks.drain(i..i + 2);
    }

    let kind = if note {
        Some("note")
    } else if time.is_some() {
        date.get_or_insert_with(|| "today".into());
        Some("appointment")
    } else if date.is_some() && !due {
        Some("deadline")
    } else {
        None
    };
    if let Some(kind) = kind {
        let title = toks.join(" ");
        if title.is_empty() {
            return Err("there is nothing to add besides the date".into());
        }
        return Ok(Intake::Item(ItemText {
            title,
            date: date.unwrap_or_default(),
            time,
            duration,
            kind: Some(kind.into()),
            calendar: place,
            ..Default::default()
        }));
    }
    let tags: Vec<String> = toks
        .iter()
        .filter(|t| t.len() > 1 && t.starts_with('#'))
        .map(|t| t[1..].to_lowercase())
        .collect();
    toks.retain(|t| !(t.len() > 1 && t.starts_with('#')));
    let title = toks.join(" ");
    if title.is_empty() {
        return Err("there is nothing to add".into());
    }
    Ok(Intake::Card {
        title,
        due: date.and_then(|d| dates::parse_date(&d, today)),
        board: place,
        tags,
    })
}

/// Adds a card to the first list of a board: the named one, else the board
/// last opened, else the first. Returns the card and "Board / List".
pub fn add_card(
    db: &Db,
    title: &str,
    due: Option<NaiveDate>,
    board: Option<&str>,
    tags: &[String],
    now: NaiveDateTime,
) -> Result<(Id, String), String> {
    let e = |e: rusqlite::Error| e.to_string();
    let title = title.trim();
    if title.is_empty() {
        return Err("the title is empty".into());
    }
    let mut boards = db.boards().map_err(e)?;
    if boards.is_empty() {
        db.add_board("Tasks").map_err(e)?;
        boards = db.boards().map_err(e)?;
    }
    let chosen = match board {
        Some(name) => boards
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| format!("there is no board called \"{name}\""))?,
        None => {
            let last: Option<Id> = db.meta("board").and_then(|v| v.parse().ok());
            boards
                .iter()
                .find(|b| Some(b.id) == last)
                .unwrap_or(&boards[0])
        }
    };
    let lists = db.lists(chosen.id).map_err(e)?;
    let list = lists.first().ok_or("the board has no lists")?;
    let id = db.add_card(list.id, title, now).map_err(e)?;
    if (due.is_some() || !tags.is_empty())
        && let Some(mut card) = db.card(id).map_err(e)?
    {
        card.due = due;
        card.tags = tags.join(",");
        db.update_card(&card).map_err(e)?;
    }
    Ok((id, format!("{} / {}", chosen.name, list.name)))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Added {
    New(Item),
    /// An item with the same `source` was there and has been brought up to
    /// date.
    Updated(Item),
    /// The same thing is already in the calendar; nothing was written.
    Duplicate(Item),
}

impl Added {
    pub fn item(&self) -> &Item {
        match self {
            Added::New(i) | Added::Updated(i) | Added::Duplicate(i) => i,
        }
    }
}

/// Adds a calendar item from text. Running the same thing twice does not add
/// it twice: with a `source` the earlier item is updated, without one an
/// identical item is left as it is (unless `allow_duplicate`).
pub fn add_item(
    db: &Db,
    text: &ItemText,
    today: NaiveDate,
    allow_duplicate: bool,
) -> Result<Added, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let calendars = db.calendars().map_err(e)?;
    let mut item = text.build(today, &calendars)?;
    if let Some(source) = &item.source
        && let Some(mut old) = db.item_by_source(source).map_err(e)?
    {
        text.apply(&mut old, today, &calendars)?;
        db.save_item(&old).map_err(e)?;
        return Ok(Added::Updated(old));
    }
    if !allow_duplicate
        && item.source.is_none()
        && let Some(id) = db.find_duplicate(&item).map_err(e)?
    {
        item.id = id;
        return Ok(Added::Duplicate(item));
    }
    item.id = db.save_item(&item).map_err(e)?;
    Ok(Added::New(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{Calendar, ItemKind};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() // a Monday
    }

    fn item(text: &str) -> ItemText {
        match parse(text, today()).unwrap() {
            Intake::Item(t) => t,
            other => panic!("{text:?} gave {other:?}"),
        }
    }

    #[test]
    fn a_time_makes_an_appointment() {
        let t = item("Dentist Jan 5 9pm for 45m");
        assert_eq!(
            (t.title.as_str(), t.date.as_str(), t.time.as_deref()),
            ("Dentist", "Jan 5", Some("9pm"))
        );
        assert_eq!(
            (t.kind.as_deref(), t.duration),
            (Some("appointment"), Some(45))
        );

        let t = item("Book club on fri at 9 PM Eastern @Friends");
        assert_eq!(t.title, "Book club");
        assert_eq!(t.time.as_deref(), Some("9 PM Eastern"));
        assert_eq!(t.calendar.as_deref(), Some("Friends"));

        let t = item("Standup 9:30");
        assert_eq!(
            (t.date.as_str(), t.time.as_deref()),
            ("today", Some("9:30"))
        );
        let t = item("Lunch at 12 tomorrow");
        assert_eq!((t.title.as_str(), t.time.as_deref()), ("Lunch", Some("12")));
    }

    #[test]
    fn dates_notes_and_cards() {
        let t = item("Tax return 2027-04-15");
        assert_eq!(
            (t.kind.as_deref(), t.title.as_str()),
            (Some("deadline"), "Tax return")
        );

        let t = item("note: call the bank");
        assert_eq!((t.kind.as_deref(), t.date.as_str()), (Some("note"), ""));
        let t = item("Note: gate code 4411 tomorrow");
        assert_eq!(
            (t.title.as_str(), t.date.as_str()),
            ("gate code 4411", "tomorrow")
        );

        assert_eq!(
            parse("Write the report due fri @work #Writing #q4", today()).unwrap(),
            Intake::Card {
                title: "Write the report".into(),
                due: NaiveDate::from_ymd_opt(2026, 11, 20),
                board: Some("work".into()),
                tags: vec!["writing".into(), "q4".into()],
            }
        );
        // Numbers and stray words are not times or dates.
        assert_eq!(
            parse("Buy 2 apples for 3 people", today()).unwrap(),
            Intake::Card {
                title: "Buy 2 apples for 3 people".into(),
                due: None,
                board: None,
                tags: vec![],
            }
        );
        assert!(parse("  ", today()).is_err());
        assert!(parse("tomorrow", today()).is_err());
        assert_eq!(item("Dentist Jan 5 9pm").date, "Jan 5");
        assert!(
            parse("Dentist Jan 5 9pm", today())
                .unwrap()
                .describe()
                .starts_with("Appointment: Dentist")
        );
    }

    #[test]
    fn cards_land_on_a_board() {
        let db = Db::open_in_memory().unwrap();
        let now = today().and_hms_opt(9, 0, 0).unwrap();
        let (id, place) = add_card(
            &db,
            " Report ",
            today().succ_opt(),
            None,
            &["a".into()],
            now,
        )
        .unwrap();
        assert_eq!(place, "Tasks / To do");
        let card = db.card(id).unwrap().unwrap();
        assert_eq!((card.title.as_str(), card.tags.as_str()), ("Report", "a"));
        assert_eq!(card.due, today().succ_opt());
        assert!(add_card(&db, "x", None, Some("nope"), &[], now).is_err());
        assert!(add_card(&db, " ", None, None, &[], now).is_err());
    }

    #[test]
    fn items_are_not_added_twice() {
        let db = Db::open_in_memory().unwrap();
        let mut work = Calendar::new("Work");
        work.tz = Some("America/New_York".into());
        db.save_calendar(&work).unwrap();
        let text = ItemText {
            title: "Club".into(),
            date: "Jan 5".into(),
            time: Some("9pm".into()),
            ..Default::default()
        };
        assert!(matches!(
            add_item(&db, &text, today(), false),
            Ok(Added::New(_))
        ));
        assert!(matches!(
            add_item(&db, &text, today(), false),
            Ok(Added::Duplicate(_))
        ));
        assert!(matches!(
            add_item(&db, &text, today(), true),
            Ok(Added::New(_))
        ));
        assert_eq!(db.items().unwrap().len(), 2);

        // A source id updates in place, and a calendar gives its zone.
        let mut sourced = ItemText {
            source: Some("club-1".into()),
            calendar: Some("work".into()),
            ..text.clone()
        };
        let Ok(Added::New(first)) = add_item(&db, &sourced, today(), false) else {
            panic!("not added")
        };
        assert_eq!(first.tz.as_deref(), Some("America/New_York"));
        sourced.time = Some("8pm UTC".into());
        let Ok(Added::Updated(second)) = add_item(&db, &sourced, today(), false) else {
            panic!("not updated")
        };
        assert_eq!(second.id, first.id);
        assert_eq!(second.tz.as_deref(), Some("UTC"));
        assert_eq!(db.items().unwrap().len(), 3);

        let bad = ItemText {
            calendar: Some("nope".into()),
            ..text.clone()
        };
        assert!(
            add_item(&db, &bad, today(), false)
                .unwrap_err()
                .contains("Personal")
        );
        let note = ItemText {
            title: "Remember".into(),
            kind: Some("note".into()),
            time: Some("9pm".into()),
            ..Default::default()
        };
        let Ok(Added::New(n)) = add_item(&db, &note, today(), false) else {
            panic!("not added")
        };
        assert_eq!((n.kind, n.date, n.time), (ItemKind::Note, today(), None));
    }
}
