//! Aggregations behind the analytics view.

use super::db::{Db, Result};
use super::model::split_tags;
use chrono::NaiveDate;
use rusqlite::params;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Day {
    pub focus_s: i64,
    /// Focus phases that ran to the end.
    pub completed: u32,
}

/// Focus per calendar day. Days without sessions are absent.
pub fn by_day(db: &Db, from: NaiveDate, to: NaiveDate) -> Result<BTreeMap<NaiveDate, Day>> {
    let mut st = db.conn.prepare(
        "SELECT date(started_at),SUM(duration_s),SUM(completed) FROM sessions
         WHERE kind='focus' AND date(started_at) BETWEEN ?1 AND ?2 GROUP BY 1",
    )?;
    st.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, NaiveDate>(0)?,
            Day {
                focus_s: r.get(1)?,
                completed: r.get(2)?,
            },
        ))
    })?
    .collect()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub focus_s: i64,
    pub completed: u32,
    /// Focus phases that were skipped or abandoned.
    pub unfinished: u32,
    pub interruptions: u32,
    pub active_days: u32,
    pub goal_days: u32,
    pub cards_done: u32,
    pub best_day: Option<(NaiveDate, i64)>,
}

pub fn summary(db: &Db, from: NaiveDate, to: NaiveDate, daily_goal: u32) -> Result<Summary> {
    let days = by_day(db, from, to)?;
    let mut s = Summary::default();
    for (d, day) in &days {
        s.focus_s += day.focus_s;
        s.completed += day.completed;
        s.active_days += 1;
        if daily_goal > 0 && day.completed >= daily_goal {
            s.goal_days += 1;
        }
        if s.best_day.is_none_or(|(_, b)| day.focus_s > b) {
            s.best_day = Some((*d, day.focus_s));
        }
    }
    // A focus phase split by a card switch has several rows; count only the
    // row that carries the skip or abandon.
    s.unfinished = db.conn.query_row(
        "SELECT COUNT(DISTINCT s.id) FROM sessions s JOIN interruptions i ON i.session_id=s.id
         WHERE s.kind='focus' AND s.completed=0 AND i.kind IN ('skip','abandon')
         AND date(s.started_at) BETWEEN ?1 AND ?2",
        params![from, to],
        |r| r.get(0),
    )?;
    s.interruptions = db.conn.query_row(
        "SELECT COUNT(*) FROM interruptions i JOIN sessions s ON s.id=i.session_id
         WHERE i.kind IN ('pause','abandon','suspend') AND date(s.started_at) BETWEEN ?1 AND ?2",
        params![from, to],
        |r| r.get(0),
    )?;
    s.cards_done = db.conn.query_row(
        "SELECT COUNT(*) FROM cards WHERE completed_at IS NOT NULL
         AND date(completed_at) BETWEEN ?1 AND ?2",
        params![from, to],
        |r| r.get(0),
    )?;
    Ok(s)
}

fn grouped(db: &Db, col: &str, from: NaiveDate, to: NaiveDate) -> Result<Vec<(String, i64)>> {
    let mut st = db.conn.prepare(&format!(
        "SELECT COALESCE({col},''),SUM(duration_s) FROM sessions
         WHERE kind='focus' AND date(started_at) BETWEEN ?1 AND ?2 GROUP BY 1 ORDER BY 2 DESC"
    ))?;
    st.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect()
}

/// Focus seconds per card title; time without a card is labelled "No task".
pub fn by_card(db: &Db, from: NaiveDate, to: NaiveDate) -> Result<Vec<(String, i64)>> {
    Ok(label_empty(grouped(db, "card_title", from, to)?, "No task"))
}

pub fn by_board(db: &Db, from: NaiveDate, to: NaiveDate) -> Result<Vec<(String, i64)>> {
    Ok(label_empty(
        grouped(db, "board_name", from, to)?,
        "No board",
    ))
}

/// Focus seconds per tag. A session with several tags counts towards each.
pub fn by_tag(db: &Db, from: NaiveDate, to: NaiveDate) -> Result<Vec<(String, i64)>> {
    let mut acc: HashMap<String, i64> = HashMap::new();
    for (tags, secs) in grouped(db, "tags", from, to)? {
        for t in split_tags(&tags) {
            *acc.entry(t).or_default() += secs;
        }
    }
    let mut v: Vec<_> = acc.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(v)
}

fn label_empty(mut v: Vec<(String, i64)>, label: &str) -> Vec<(String, i64)> {
    for (name, _) in &mut v {
        if name.is_empty() {
            *name = label.to_string();
        }
    }
    v
}

/// Focus seconds by the hour of day the session started in.
pub fn by_hour(db: &Db, from: NaiveDate, to: NaiveDate) -> Result<[i64; 24]> {
    let mut st = db.conn.prepare(
        "SELECT CAST(strftime('%H',started_at) AS INTEGER),SUM(duration_s) FROM sessions
         WHERE kind='focus' AND date(started_at) BETWEEN ?1 AND ?2 GROUP BY 1",
    )?;
    let mut out = [0i64; 24];
    for row in st.query_map(params![from, to], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (h, s) = row?;
        out[h.clamp(0, 23) as usize] = s;
    }
    Ok(out)
}

/// Most common interruption reasons, blank reasons grouped as "No reason".
pub fn reasons(db: &Db, from: NaiveDate, to: NaiveDate) -> Result<Vec<(String, i64)>> {
    let mut st = db.conn.prepare(
        "SELECT lower(trim(i.reason)),COUNT(*) FROM interruptions i
         JOIN sessions s ON s.id=i.session_id
         WHERE i.kind IN ('pause','abandon') AND date(s.started_at) BETWEEN ?1 AND ?2
         GROUP BY 1 ORDER BY 2 DESC LIMIT 8",
    )?;
    let v: Vec<(String, i64)> = st
        .query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_>>()?;
    Ok(label_empty(v, "No reason"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    pub title: String,
    pub estimate: f64,
    /// Actual effort in pomodoros of the configured length.
    pub actual: f64,
}

/// Finished cards that had an estimate, most recently finished first.
pub fn estimates(db: &Db, from: NaiveDate, to: NaiveDate, focus_s: i64) -> Result<Vec<Estimate>> {
    let mut st = db.conn.prepare(
        "SELECT c.title,c.estimate,
         (SELECT COALESCE(SUM(duration_s),0) FROM sessions s WHERE s.card_id=c.id AND s.kind='focus')
         FROM cards c WHERE c.estimate>0 AND c.completed_at IS NOT NULL
         AND date(c.completed_at) BETWEEN ?1 AND ?2 ORDER BY c.completed_at DESC LIMIT 12",
    )?;
    st.query_map(params![from, to], |r| {
        Ok(Estimate {
            title: r.get(0)?,
            estimate: r.get(1)?,
            actual: r.get::<_, i64>(2)? as f64 / focus_s.max(1) as f64,
        })
    })?
    .collect()
}

/// Focus minutes per day for a habit link: a tag, a board, or both.
pub fn focus_minutes_by_day(
    db: &Db,
    tag: Option<&str>,
    board: Option<&str>,
) -> Result<HashMap<NaiveDate, f64>> {
    let mut st = db.conn.prepare(
        "SELECT date(started_at),tags,COALESCE(board_name,''),duration_s FROM sessions WHERE kind='focus'",
    )?;
    let mut out: HashMap<NaiveDate, f64> = HashMap::new();
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, NaiveDate>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
        ))
    })?;
    for row in rows {
        let (d, tags, b, secs) = row?;
        if tag.is_some_and(|t| !split_tags(&tags).iter().any(|x| x == t)) {
            continue;
        }
        if board.is_some_and(|name| name != b) {
            continue;
        }
        *out.entry(d).or_default() += secs as f64 / 60.0;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::*;
    use chrono::{Duration, NaiveDateTime};

    fn at(day: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap()
    }

    fn log(
        db: &Db,
        card: Option<Id>,
        start: NaiveDateTime,
        secs: i64,
        completed: bool,
        why: &[&str],
    ) {
        db.insert_session(&SessionRecord {
            kind: Phase::Focus,
            card_id: card,
            started_at: start,
            ended_at: start + Duration::seconds(secs),
            duration_s: secs,
            planned_s: 1500,
            completed,
            interruptions: why
                .iter()
                .map(|k| Interruption {
                    at: start,
                    kind: (*k).into(),
                    reason: "Phone".into(),
                })
                .collect(),
        })
        .unwrap();
    }

    fn seeded() -> (Db, Id, Id) {
        let db = Db::open_in_memory().unwrap();
        let b = db.add_board("Work").unwrap();
        let lists = db.lists(b).unwrap();
        let a = db.add_card(lists[1].id, "Report", at(1, 8)).unwrap();
        let mut card = db.card(a).unwrap().unwrap();
        card.tags = "writing, deep".into();
        card.estimate = 2.0;
        db.update_card(&card).unwrap();
        let c = db.add_card(lists[1].id, "Email", at(1, 8)).unwrap();
        log(&db, Some(a), at(2, 9), 1500, true, &[]);
        log(&db, Some(a), at(2, 10), 1500, true, &["pause"]);
        log(&db, Some(c), at(2, 14), 600, false, &["pause", "abandon"]);
        log(&db, None, at(3, 9), 1500, true, &[]);
        // A break must never be counted as focus.
        db.insert_session(&SessionRecord {
            kind: Phase::ShortBreak,
            card_id: None,
            started_at: at(2, 11),
            ended_at: at(2, 11),
            duration_s: 300,
            planned_s: 300,
            completed: true,
            interruptions: vec![],
        })
        .unwrap();
        (db, a, lists[2].id)
    }

    fn range() -> (NaiveDate, NaiveDate) {
        (at(1, 0).date(), at(30, 0).date())
    }

    #[test]
    fn days_and_summary() {
        let (db, a, done) = seeded();
        let (f, t) = range();
        let days = by_day(&db, f, t).unwrap();
        assert_eq!(
            days[&at(2, 0).date()],
            Day {
                focus_s: 3600,
                completed: 2
            }
        );
        assert_eq!(days.len(), 2);

        db.move_card(a, done, 0, at(3, 12)).unwrap();
        let s = summary(&db, f, t, 2).unwrap();
        assert_eq!(s.focus_s, 5100);
        assert_eq!(s.completed, 3);
        assert_eq!(s.unfinished, 1);
        assert_eq!(s.interruptions, 3);
        assert_eq!((s.active_days, s.goal_days, s.cards_done), (2, 1, 1));
        assert_eq!(s.best_day, Some((at(2, 0).date(), 3600)));

        let e = estimates(&db, f, t, 1500).unwrap();
        assert_eq!(
            e,
            vec![Estimate {
                title: "Report".into(),
                estimate: 2.0,
                actual: 2.0
            }]
        );
    }

    #[test]
    fn range_is_respected() {
        let (db, ..) = seeded();
        let d3 = at(3, 0).date();
        assert_eq!(summary(&db, d3, d3, 0).unwrap().focus_s, 1500);
    }

    #[test]
    fn groupings() {
        let (db, ..) = seeded();
        let (f, t) = range();
        assert_eq!(
            by_card(&db, f, t).unwrap(),
            vec![
                ("Report".into(), 3000),
                ("No task".into(), 1500),
                ("Email".into(), 600)
            ]
        );
        assert_eq!(by_board(&db, f, t).unwrap()[0], ("Work".into(), 3600));
        assert_eq!(
            by_tag(&db, f, t).unwrap(),
            vec![("deep".into(), 3000), ("writing".into(), 3000)]
        );
        let hours = by_hour(&db, f, t).unwrap();
        assert_eq!(
            (hours[9], hours[10], hours[14], hours[11]),
            (3000, 1500, 600, 0)
        );
        assert_eq!(reasons(&db, f, t).unwrap(), vec![("phone".into(), 3)]);
    }

    #[test]
    fn habit_link_minutes() {
        let (db, ..) = seeded();
        let m = focus_minutes_by_day(&db, Some("writing"), None).unwrap();
        assert_eq!(m[&at(2, 0).date()], 50.0);
        assert_eq!(m.len(), 1);
        let m = focus_minutes_by_day(&db, None, Some("Work")).unwrap();
        assert_eq!(m[&at(2, 0).date()], 60.0);
    }
}
