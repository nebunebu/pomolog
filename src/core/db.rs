use super::model::*;
use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashMap;
use std::path::Path;

pub type Result<T> = rusqlite::Result<T>;

const MIGRATIONS: &[&str] = &[include_str!("migrations/0001_init.sql")];

pub struct Db {
    pub conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        let mut db = Db { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let version = self.schema_version()?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<usize> {
        let v: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        Ok(v.max(0) as usize)
    }

    // ---- meta ----

    pub fn meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, value],
        )?;
        Ok(())
    }

    // ---- boards and lists ----

    pub fn boards(&self) -> Result<Vec<Board>> {
        let mut st = self
            .conn
            .prepare("SELECT id,name FROM boards ORDER BY position,id")?;
        st.query_map([], |r| {
            Ok(Board {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect()
    }

    /// Creates a board with the three standard lists.
    pub fn add_board(&self, name: &str) -> Result<Id> {
        let pos: i64 =
            self.conn
                .query_row("SELECT COALESCE(MAX(position),0)+1 FROM boards", [], |r| {
                    r.get(0)
                })?;
        self.conn.execute(
            "INSERT INTO boards(name,position) VALUES(?1,?2)",
            params![name, pos],
        )?;
        let id = self.conn.last_insert_rowid();
        for (i, (n, k)) in [
            ("To do", "normal"),
            ("In progress", "progress"),
            ("Done", "done"),
        ]
        .iter()
        .enumerate()
        {
            self.conn.execute(
                "INSERT INTO lists(board_id,name,kind,position) VALUES(?1,?2,?3,?4)",
                params![id, n, k, (i as i64 + 1) * 1000],
            )?;
        }
        Ok(id)
    }

    pub fn rename_board(&self, id: Id, name: &str) -> Result<()> {
        self.conn
            .execute("UPDATE boards SET name=?2 WHERE id=?1", params![id, name])?;
        Ok(())
    }

    pub fn delete_board(&self, id: Id) -> Result<()> {
        self.conn.execute("DELETE FROM boards WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn lists(&self, board: Id) -> Result<Vec<List>> {
        let mut st = self.conn.prepare(
            "SELECT id,board_id,name,kind FROM lists WHERE board_id=?1 ORDER BY position,id",
        )?;
        st.query_map([board], |r| {
            Ok(List {
                id: r.get(0)?,
                board_id: r.get(1)?,
                name: r.get(2)?,
                kind: ListKind::parse(&r.get::<_, String>(3)?),
            })
        })?
        .collect()
    }

    /// All lists of the board that `list` belongs to.
    pub fn sibling_lists(&self, list: Id) -> Result<Vec<List>> {
        let board: Option<Id> = self
            .conn
            .query_row("SELECT board_id FROM lists WHERE id=?1", [list], |r| {
                r.get(0)
            })
            .optional()?;
        match board {
            Some(b) => self.lists(b),
            None => Ok(Vec::new()),
        }
    }

    /// Adds a custom list just before "In progress".
    pub fn add_list(&self, board: Id, name: &str) -> Result<Id> {
        let progress: i64 = self.conn.query_row(
            "SELECT position FROM lists WHERE board_id=?1 AND kind='progress'",
            [board],
            |r| r.get(0),
        )?;
        let before: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position),0) FROM lists WHERE board_id=?1 AND position<?2",
            params![board, progress],
            |r| r.get(0),
        )?;
        let mut pos = (before + progress) / 2;
        if pos == before {
            // No room left between neighbours: push the tail along.
            self.conn.execute(
                "UPDATE lists SET position=position+1000 WHERE board_id=?1 AND position>=?2",
                params![board, progress],
            )?;
            pos = before + 500;
        }
        self.conn.execute(
            "INSERT INTO lists(board_id,name,kind,position) VALUES(?1,?2,'normal',?3)",
            params![board, name, pos],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_list(&self, id: Id, name: &str) -> Result<()> {
        self.conn
            .execute("UPDATE lists SET name=?2 WHERE id=?1", params![id, name])?;
        Ok(())
    }

    /// Only custom lists can be deleted. Returns false for the fixed ones.
    pub fn delete_list(&self, id: Id) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM lists WHERE id=?1 AND kind='normal'", [id])?;
        Ok(n > 0)
    }

    // ---- cards ----

    const CARD_COLS: &'static str = "c.id,c.list_id,c.title,c.notes,c.estimate,c.due,c.tags,c.completed_at,
        (SELECT COALESCE(SUM(duration_s),0) FROM sessions s WHERE s.card_id=c.id AND s.kind='focus')";

    fn card_from(r: &Row) -> Result<Card> {
        Ok(Card {
            id: r.get(0)?,
            list_id: r.get(1)?,
            title: r.get(2)?,
            notes: r.get(3)?,
            estimate: r.get(4)?,
            due: r.get(5)?,
            tags: r.get(6)?,
            completed_at: r.get(7)?,
            spent_s: r.get(8)?,
        })
    }

    /// Cards of a board in list and position order.
    pub fn cards(&self, board: Id) -> Result<Vec<Card>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {} FROM cards c JOIN lists l ON l.id=c.list_id
             WHERE l.board_id=?1 ORDER BY l.position,c.position,c.id",
            Self::CARD_COLS
        ))?;
        st.query_map([board], Self::card_from)?.collect()
    }

    pub fn card(&self, id: Id) -> Result<Option<Card>> {
        self.conn
            .query_row(
                &format!("SELECT {} FROM cards c WHERE c.id=?1", Self::CARD_COLS),
                [id],
                Self::card_from,
            )
            .optional()
    }

    /// Cards with a due date, across all boards, with their board name and
    /// whether they are finished.
    pub fn due_cards(&self) -> Result<Vec<(Card, String, bool)>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {},b.name,l.kind FROM cards c JOIN lists l ON l.id=c.list_id
             JOIN boards b ON b.id=l.board_id WHERE c.due IS NOT NULL ORDER BY c.due,c.id",
            Self::CARD_COLS
        ))?;
        st.query_map([], |r| {
            Ok((
                Self::card_from(r)?,
                r.get(9)?,
                r.get::<_, String>(10)? == "done",
            ))
        })?
        .collect()
    }

    /// Cards in any "In progress" list, with their board name.
    pub fn in_progress(&self) -> Result<Vec<(Card, String)>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {},b.name FROM cards c JOIN lists l ON l.id=c.list_id
             JOIN boards b ON b.id=l.board_id WHERE l.kind='progress'
             ORDER BY b.position,c.position,c.id",
            Self::CARD_COLS
        ))?;
        st.query_map([], |r| Ok((Self::card_from(r)?, r.get(9)?)))?
            .collect()
    }

    pub fn add_card(&self, list: Id, title: &str, now: NaiveDateTime) -> Result<Id> {
        let pos: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position),0)+1 FROM cards WHERE list_id=?1",
            [list],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO cards(list_id,title,position,created_at) VALUES(?1,?2,?3,?4)",
            params![list, title, pos, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Saves the editable fields; does not move the card.
    pub fn update_card(&self, c: &Card) -> Result<()> {
        self.conn.execute(
            "UPDATE cards SET title=?2,notes=?3,estimate=?4,due=?5,tags=?6 WHERE id=?1",
            params![
                c.id,
                c.title,
                c.notes,
                c.estimate,
                c.due,
                split_tags(&c.tags).join(",")
            ],
        )?;
        Ok(())
    }

    pub fn delete_card(&self, id: Id) -> Result<()> {
        self.conn.execute("DELETE FROM cards WHERE id=?1", [id])?;
        Ok(())
    }

    /// Moves a card to `index` within `list`, renumbering that list. Entering
    /// a Done list stamps the completion time; leaving it clears it.
    pub fn move_card(&self, card: Id, list: Id, index: usize, now: NaiveDateTime) -> Result<()> {
        let mut ids: Vec<Id> = {
            let mut st = self
                .conn
                .prepare("SELECT id FROM cards WHERE list_id=?1 AND id<>?2 ORDER BY position,id")?;
            st.query_map(params![list, card], |r| r.get(0))?
                .collect::<Result<_>>()?
        };
        ids.insert(index.min(ids.len()), card);
        let kind: String =
            self.conn
                .query_row("SELECT kind FROM lists WHERE id=?1", [list], |r| r.get(0))?;
        for (i, id) in ids.iter().enumerate() {
            self.conn.execute(
                "UPDATE cards SET list_id=?2, position=?3 WHERE id=?1",
                params![id, list, i as i64 + 1],
            )?;
        }
        if kind == "done" {
            self.conn.execute(
                "UPDATE cards SET completed_at=COALESCE(completed_at,?2) WHERE id=?1",
                params![card, now],
            )?;
        } else {
            self.conn
                .execute("UPDATE cards SET completed_at=NULL WHERE id=?1", [card])?;
        }
        Ok(())
    }

    /// The list of the given kind on a board (first match for Normal).
    pub fn list_of_kind(&self, board: Id, kind: ListKind) -> Result<Option<Id>> {
        self.conn
            .query_row(
                "SELECT id FROM lists WHERE board_id=?1 AND kind=?2 ORDER BY position LIMIT 1",
                params![board, kind.as_str()],
                |r| r.get(0),
            )
            .optional()
    }

    // ---- sessions ----

    pub fn insert_session(&self, s: &SessionRecord) -> Result<Id> {
        let snap: Option<(String, String, String)> = match s.card_id {
            Some(id) => self
                .conn
                .query_row(
                    "SELECT c.title,b.name,c.tags FROM cards c JOIN lists l ON l.id=c.list_id
                     JOIN boards b ON b.id=l.board_id WHERE c.id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?,
            None => None,
        };
        let card_id = snap.as_ref().and(s.card_id);
        let (title, board, tags) = match snap {
            Some((t, b, g)) => (Some(t), Some(b), g),
            None => (None, None, String::new()),
        };
        self.conn.execute(
            "INSERT INTO sessions(kind,card_id,started_at,ended_at,duration_s,planned_s,completed,
             card_title,board_name,tags) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                s.kind.as_str(),
                card_id,
                s.started_at,
                s.ended_at,
                s.duration_s,
                s.planned_s,
                s.completed,
                title,
                board,
                tags
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        for i in &s.interruptions {
            self.conn.execute(
                "INSERT INTO interruptions(session_id,at,kind,reason) VALUES(?1,?2,?3,?4)",
                params![id, i.at, i.kind, i.reason],
            )?;
        }
        Ok(id)
    }

    pub fn delete_session(&self, id: Id) -> Result<()> {
        self.conn
            .execute("DELETE FROM sessions WHERE id=?1", [id])?;
        Ok(())
    }

    /// Sessions that started on a day in the inclusive range, newest first.
    pub fn sessions_between(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<SessionRow>> {
        let mut st = self.conn.prepare(
            "SELECT s.id,s.kind,s.card_id,s.card_title,s.started_at,s.ended_at,s.duration_s,s.completed,
             (SELECT COUNT(*) FROM interruptions i WHERE i.session_id=s.id)
             FROM sessions s WHERE date(s.started_at) BETWEEN ?1 AND ?2 ORDER BY s.started_at DESC",
        )?;
        st.query_map(params![from, to], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                kind: Phase::parse(&r.get::<_, String>(1)?),
                card_id: r.get(2)?,
                card_title: r.get(3)?,
                started_at: r.get(4)?,
                ended_at: r.get(5)?,
                duration_s: r.get(6)?,
                completed: r.get(7)?,
                interruptions: r.get(8)?,
            })
        })?
        .collect()
    }

    // ---- calendar items ----

    pub fn items(&self) -> Result<Vec<Item>> {
        let mut st = self.conn.prepare(
            "SELECT id,kind,title,date,time,duration_min,recur,recur_until,done,source,notes
             FROM items ORDER BY date,time,id",
        )?;
        st.query_map([], |r| {
            Ok(Item {
                id: r.get(0)?,
                kind: ItemKind::parse(&r.get::<_, String>(1)?),
                title: r.get(2)?,
                date: r.get(3)?,
                time: r.get(4)?,
                duration_min: r.get(5)?,
                recur: Recur::parse(&r.get::<_, String>(6)?),
                recur_until: r.get(7)?,
                done: r.get(8)?,
                source: r.get(9)?,
                notes: r.get(10)?,
            })
        })?
        .collect()
    }

    /// Inserts when `id` is 0, otherwise updates. Returns the id.
    pub fn save_item(&self, i: &Item) -> Result<Id> {
        if i.id == 0 {
            self.conn.execute(
                "INSERT INTO items(kind,title,date,time,duration_min,recur,recur_until,done,source,notes)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    i.kind.as_str(),
                    i.title,
                    i.date,
                    i.time,
                    i.duration_min,
                    i.recur.as_str(),
                    i.recur_until,
                    i.done,
                    i.source,
                    i.notes
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE items SET kind=?2,title=?3,date=?4,time=?5,duration_min=?6,recur=?7,
                 recur_until=?8,done=?9,source=?10,notes=?11 WHERE id=?1",
                params![
                    i.id,
                    i.kind.as_str(),
                    i.title,
                    i.date,
                    i.time,
                    i.duration_min,
                    i.recur.as_str(),
                    i.recur_until,
                    i.done,
                    i.source,
                    i.notes
                ],
            )?;
            Ok(i.id)
        }
    }

    pub fn delete_item(&self, id: Id) -> Result<()> {
        self.conn.execute("DELETE FROM items WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn item_source_exists(&self, source: &str) -> Result<bool> {
        self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM items WHERE source=?1)",
            [source],
            |r| r.get(0),
        )
    }

    // ---- habits ----

    pub fn habits(&self) -> Result<Vec<Habit>> {
        let mut st = self.conn.prepare(
            "SELECT id,name,kind,target,unit,schedule,link_tag,link_board,rest_on_holidays,
             skips_per_month,created,archived FROM habits ORDER BY archived,position,id",
        )?;
        st.query_map([], |r| {
            Ok(Habit {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: HabitKind::parse(&r.get::<_, String>(2)?),
                target: r.get(3)?,
                unit: r.get(4)?,
                schedule: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(Schedule::Daily),
                link_tag: r.get(6)?,
                link_board: r.get(7)?,
                rest_on_holidays: r.get(8)?,
                skips_per_month: r.get(9)?,
                created: r.get(10)?,
                archived: r.get(11)?,
            })
        })?
        .collect()
    }

    pub fn save_habit(&self, h: &Habit) -> Result<Id> {
        let schedule = serde_json::to_string(&h.schedule).unwrap_or_default();
        let tag = h
            .link_tag
            .as_deref()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty());
        if h.id == 0 {
            self.conn.execute(
                "INSERT INTO habits(name,kind,target,unit,schedule,link_tag,link_board,
                 rest_on_holidays,skips_per_month,created,archived,position)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,
                 (SELECT COALESCE(MAX(position),0)+1 FROM habits))",
                params![
                    h.name,
                    h.kind.as_str(),
                    h.target,
                    h.unit,
                    schedule,
                    tag,
                    h.link_board,
                    h.rest_on_holidays,
                    h.skips_per_month,
                    h.created,
                    h.archived
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        } else {
            self.conn.execute(
                "UPDATE habits SET name=?2,kind=?3,target=?4,unit=?5,schedule=?6,link_tag=?7,
                 link_board=?8,rest_on_holidays=?9,skips_per_month=?10,created=?11,archived=?12
                 WHERE id=?1",
                params![
                    h.id,
                    h.name,
                    h.kind.as_str(),
                    h.target,
                    h.unit,
                    schedule,
                    tag,
                    h.link_board,
                    h.rest_on_holidays,
                    h.skips_per_month,
                    h.created,
                    h.archived
                ],
            )?;
            Ok(h.id)
        }
    }

    pub fn delete_habit(&self, id: Id) -> Result<()> {
        self.conn.execute("DELETE FROM habits WHERE id=?1", [id])?;
        Ok(())
    }

    /// All stored entries, keyed by habit.
    pub fn habit_entries(&self) -> Result<HashMap<Id, HashMap<NaiveDate, HabitEntry>>> {
        let mut st = self
            .conn
            .prepare("SELECT habit_id,date,value,skipped FROM habit_entries")?;
        let mut out: HashMap<Id, HashMap<NaiveDate, HabitEntry>> = HashMap::new();
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, Id>(0)?,
                r.get::<_, NaiveDate>(1)?,
                HabitEntry {
                    value: r.get(2)?,
                    skipped: r.get(3)?,
                },
            ))
        })?;
        for row in rows {
            let (h, d, e) = row?;
            out.entry(h).or_default().insert(d, e);
        }
        Ok(out)
    }

    /// Stores an entry; an empty one (no value, not skipped) removes the row.
    pub fn set_habit_entry(&self, habit: Id, date: NaiveDate, e: HabitEntry) -> Result<()> {
        if e.value <= 0.0 && !e.skipped {
            self.conn.execute(
                "DELETE FROM habit_entries WHERE habit_id=?1 AND date=?2",
                params![habit, date],
            )?;
        } else {
            self.conn.execute(
                "INSERT INTO habit_entries(habit_id,date,value,skipped) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(habit_id,date) DO UPDATE SET value=?3, skipped=?4",
                params![habit, date, e.value, e.skipped],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 16)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
    }

    #[test]
    fn migrates_empty_database() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len());
        assert!(db.boards().unwrap().is_empty());
    }

    #[test]
    fn board_has_fixed_lists_and_custom_go_before_progress() {
        let db = Db::open_in_memory().unwrap();
        let b = db.add_board("Work").unwrap();
        for n in ["Backlog", "Next", "Blocked"] {
            db.add_list(b, n).unwrap();
        }
        let lists = db.lists(b).unwrap();
        let names: Vec<&str> = lists.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            ["To do", "Backlog", "Next", "Blocked", "In progress", "Done"]
        );
        assert!(!db.delete_list(lists[4].id).unwrap(), "fixed lists stay");
        assert!(db.delete_list(lists[1].id).unwrap());
    }

    #[test]
    fn many_custom_lists_keep_order() {
        let db = Db::open_in_memory().unwrap();
        let b = db.add_board("Work").unwrap();
        for i in 0..15 {
            db.add_list(b, &format!("L{i}")).unwrap();
        }
        let lists = db.lists(b).unwrap();
        assert_eq!(lists.len(), 18);
        assert_eq!(lists[16].kind, ListKind::Progress);
        assert_eq!(lists[15].name, "L14");
    }

    #[test]
    fn move_card_orders_and_stamps_done() {
        let db = Db::open_in_memory().unwrap();
        let b = db.add_board("Work").unwrap();
        let lists = db.lists(b).unwrap();
        let (todo, done) = (lists[0].id, lists[2].id);
        let a = db.add_card(todo, "a", now()).unwrap();
        let c = db.add_card(todo, "c", now()).unwrap();
        let x = db.add_card(todo, "x", now()).unwrap();
        db.move_card(x, todo, 0, now()).unwrap();
        let order: Vec<Id> = db.cards(b).unwrap().iter().map(|k| k.id).collect();
        assert_eq!(order, [x, a, c]);

        db.move_card(a, done, 0, now()).unwrap();
        assert!(db.card(a).unwrap().unwrap().completed_at.is_some());
        db.move_card(a, todo, 5, now()).unwrap();
        let card = db.card(a).unwrap().unwrap();
        assert!(card.completed_at.is_none());
        let order: Vec<Id> = db.cards(b).unwrap().iter().map(|k| k.id).collect();
        assert_eq!(order, [x, c, a]);
    }

    #[test]
    fn sessions_feed_card_time_and_survive_card_deletion() {
        let db = Db::open_in_memory().unwrap();
        let b = db.add_board("Work").unwrap();
        let list = db.lists(b).unwrap()[1].id;
        let c = db.add_card(list, "Report", now()).unwrap();
        db.insert_session(&SessionRecord {
            kind: Phase::Focus,
            card_id: Some(c),
            started_at: now(),
            ended_at: now() + chrono::Duration::minutes(25),
            duration_s: 1500,
            planned_s: 1500,
            completed: true,
            interruptions: vec![Interruption {
                at: now(),
                kind: "pause".into(),
                reason: "tea".into(),
            }],
        })
        .unwrap();
        assert_eq!(db.card(c).unwrap().unwrap().spent_s, 1500);
        assert_eq!(db.in_progress().unwrap().len(), 1);
        db.delete_card(c).unwrap();
        let rows = db.sessions_between(now().date(), now().date()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].card_id, None);
        assert_eq!(rows[0].card_title.as_deref(), Some("Report"));
        assert_eq!(rows[0].interruptions, 1);
    }

    #[test]
    fn session_for_missing_card_is_stored_unlinked() {
        let db = Db::open_in_memory().unwrap();
        db.insert_session(&SessionRecord {
            kind: Phase::Focus,
            card_id: Some(999),
            started_at: now(),
            ended_at: now(),
            duration_s: 60,
            planned_s: 60,
            completed: true,
            interruptions: vec![],
        })
        .unwrap();
        assert_eq!(
            db.sessions_between(now().date(), now().date()).unwrap()[0].card_id,
            None
        );
    }

    #[test]
    fn items_and_habits_round_trip() {
        let db = Db::open_in_memory().unwrap();
        let mut it = Item::new(ItemKind::Appointment, "Dentist", now().date());
        it.time = chrono::NaiveTime::from_hms_opt(14, 30, 0);
        it.recur = Recur::Yearly;
        it.id = db.save_item(&it).unwrap();
        assert_eq!(db.items().unwrap(), vec![it.clone()]);
        it.title = "Dentist!".into();
        db.save_item(&it).unwrap();
        assert_eq!(db.items().unwrap()[0].title, "Dentist!");

        let mut h = Habit::new("Read", now().date());
        h.schedule = Schedule::Weekdays(vec![0, 2]);
        h.link_tag = Some(" Writing ".into());
        h.id = db.save_habit(&h).unwrap();
        let got = &db.habits().unwrap()[0];
        assert_eq!(got.schedule, h.schedule);
        assert_eq!(got.link_tag.as_deref(), Some("writing"));

        db.set_habit_entry(
            h.id,
            now().date(),
            HabitEntry {
                value: 1.0,
                skipped: false,
            },
        )
        .unwrap();
        assert_eq!(db.habit_entries().unwrap()[&h.id].len(), 1);
        db.set_habit_entry(h.id, now().date(), HabitEntry::default())
            .unwrap();
        assert!(!db.habit_entries().unwrap().contains_key(&h.id));
    }
}
