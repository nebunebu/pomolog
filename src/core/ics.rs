//! A small iCalendar reader: enough for public holiday files and exported
//! appointments. Times are taken as local wall-clock time; UTC times ("Z")
//! are converted to local. Only plain FREQ rules are kept as recurrence.

use super::db::{Db, Result};
use super::model::{Item, ItemKind, Recur};
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub duration_min: Option<i64>,
    pub recur: Recur,
    pub until: Option<NaiveDate>,
}

pub fn parse(text: &str) -> Vec<Event> {
    // Unfold: a line starting with a space or tab continues the previous one.
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        let raw = raw.trim_end_matches('\r');
        if let Some(rest) = raw.strip_prefix([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
            }
        } else {
            lines.push(raw.to_string());
        }
    }

    let mut out = Vec::new();
    let mut cur: Option<Vec<(String, String, String)>> = None;
    for line in lines {
        let upper = line.to_ascii_uppercase();
        if upper == "BEGIN:VEVENT" {
            cur = Some(Vec::new());
        } else if upper == "END:VEVENT" {
            if let Some(props) = cur.take()
                && let Some(ev) = build(&props)
            {
                out.push(ev);
            }
        } else if let Some(props) = cur.as_mut()
            && let Some((head, value)) = line.split_once(':')
        {
            let (name, params) = head.split_once(';').unwrap_or((head, ""));
            props.push((
                name.to_ascii_uppercase(),
                params.to_ascii_uppercase(),
                value.to_string(),
            ));
        }
    }
    out
}

fn get<'a>(props: &'a [(String, String, String)], name: &str) -> Option<(&'a str, &'a str)> {
    props
        .iter()
        .find(|p| p.0 == name)
        .map(|p| (p.1.as_str(), p.2.as_str()))
}

fn parse_when(value: &str) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let v = value.trim();
    if v.len() == 8 {
        return Some((NaiveDate::parse_from_str(v, "%Y%m%d").ok()?, None));
    }
    let utc = v.ends_with('Z');
    let dt = NaiveDateTime::parse_from_str(v.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()?;
    let dt = if utc {
        Utc.from_utc_datetime(&dt)
            .with_timezone(&Local)
            .naive_local()
    } else {
        dt
    };
    Some((dt.date(), Some(dt.time())))
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ")
        .replace("\\N", " ")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn build(props: &[(String, String, String)]) -> Option<Event> {
    let (_, start) = get(props, "DTSTART")?;
    let (date, time) = parse_when(start)?;
    let summary = unescape(get(props, "SUMMARY").map_or("", |p| p.1))
        .trim()
        .to_string();
    if summary.is_empty() {
        return None;
    }
    let duration_min = match (time, get(props, "DTEND").and_then(|p| parse_when(p.1))) {
        (Some(t), Some((ed, Some(et)))) => {
            let mins = (ed.and_time(et) - date.and_time(t)).num_minutes();
            (mins > 0).then_some(mins)
        }
        _ => None,
    };
    let (mut recur, mut until) = (Recur::None, None);
    if let Some((_, rule)) = get(props, "RRULE") {
        let mut simple = true;
        for part in rule.split(';') {
            match part.split_once('=') {
                Some(("FREQ", f)) => {
                    recur = match f {
                        "DAILY" => Recur::Daily,
                        "WEEKLY" => Recur::Weekly,
                        "MONTHLY" => Recur::Monthly,
                        "YEARLY" => Recur::Yearly,
                        _ => Recur::None,
                    }
                }
                Some(("UNTIL", u)) => until = parse_when(u).map(|w| w.0),
                Some(("INTERVAL", "1")) | Some(("WKST", _)) => {}
                // Anything else (BYDAY lists, counts, intervals) is beyond
                // what the calendar models, so keep only the first occurrence.
                _ => simple = false,
            }
        }
        if !simple {
            recur = Recur::None;
            until = None;
        }
    }
    let uid = get(props, "UID").map_or_else(|| format!("{summary}@{date}"), |p| p.1.to_string());
    Some(Event {
        uid,
        summary,
        date,
        time,
        duration_min,
        recur,
        until,
    })
}

/// Imports events as items of `kind`, skipping ones imported before.
/// Returns (added, skipped).
pub fn import(db: &Db, text: &str, kind: ItemKind) -> Result<(usize, usize)> {
    let (mut added, mut skipped) = (0, 0);
    for ev in parse(text) {
        let source = format!("ics:{}:{}", ev.uid, ev.date);
        if db.item_source_exists(&source)? {
            skipped += 1;
            continue;
        }
        let holiday = kind == ItemKind::Holiday;
        db.save_item(&Item {
            id: 0,
            kind,
            title: ev.summary,
            date: ev.date,
            time: if holiday { None } else { ev.time },
            duration_min: if holiday { None } else { ev.duration_min },
            recur: ev.recur,
            recur_until: ev.until,
            done: false,
            source: Some(source),
            notes: String::new(),
        })?;
        added += 1;
    }
    Ok((added, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:xmas@example\r\n\
DTSTART;VALUE=DATE:20261225\r\nDTEND;VALUE=DATE:20261226\r\nSUMMARY:Christmas\r\n  Day\r\n\
RRULE:FREQ=YEARLY\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:dent\r\n\
DTSTART;TZID=Europe/Berlin:20261120T143000\r\nDTEND;TZID=Europe/Berlin:20261120T151500\r\n\
SUMMARY:Dentist\\, check-up\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:odd\r\nDTSTART:20261102T090000\r\n\
SUMMARY:Standup\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=10\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nDTSTART:20261102\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn parses_events() {
        let ev = parse(SAMPLE);
        assert_eq!(ev.len(), 3, "an event without a summary is dropped");
        assert_eq!(ev[0].summary, "Christmas Day", "folded lines are joined");
        assert_eq!(ev[0].date, NaiveDate::from_ymd_opt(2026, 12, 25).unwrap());
        assert_eq!((ev[0].time, ev[0].recur), (None, Recur::Yearly));
        assert_eq!(ev[1].summary, "Dentist, check-up");
        assert_eq!(ev[1].time, NaiveTime::from_hms_opt(14, 30, 0));
        assert_eq!(ev[1].duration_min, Some(45));
        assert_eq!(
            ev[2].recur,
            Recur::None,
            "complex rules fall back to a single event"
        );
    }

    #[test]
    fn import_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(import(&db, SAMPLE, ItemKind::Holiday).unwrap(), (3, 0));
        assert_eq!(import(&db, SAMPLE, ItemKind::Holiday).unwrap(), (0, 3));
        let items = db.items().unwrap();
        assert!(
            items
                .iter()
                .all(|i| i.kind == ItemKind::Holiday && i.time.is_none())
        );
    }
}
