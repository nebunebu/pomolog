use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

pub type Id = i64;

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub id: Id,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Normal,
    Progress,
    Done,
}

impl ListKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ListKind::Normal => "normal",
            ListKind::Progress => "progress",
            ListKind::Done => "done",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "progress" => ListKind::Progress,
            "done" => ListKind::Done,
            _ => ListKind::Normal,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub id: Id,
    pub board_id: Id,
    pub name: String,
    pub kind: ListKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: Id,
    pub list_id: Id,
    pub title: String,
    pub notes: String,
    /// Estimated effort in pomodoros.
    pub estimate: f64,
    pub due: Option<NaiveDate>,
    /// Comma separated, lower case.
    pub tags: String,
    pub completed_at: Option<NaiveDateTime>,
    /// Focus seconds logged against this card.
    pub spent_s: i64,
}

impl Card {
    pub fn tag_list(&self) -> Vec<String> {
        split_tags(&self.tags)
    }
}

pub fn split_tags(s: &str) -> Vec<String> {
    s.split(',')
        .map(|t| t.trim().trim_start_matches('#').to_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    Focus,
    ShortBreak,
    LongBreak,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Focus => "focus",
            Phase::ShortBreak => "short",
            Phase::LongBreak => "long",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "short" => Phase::ShortBreak,
            "long" => Phase::LongBreak,
            _ => Phase::Focus,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Phase::Focus => "Focus",
            Phase::ShortBreak => "Short break",
            Phase::LongBreak => "Long break",
        }
    }
    pub fn is_break(self) -> bool {
        self != Phase::Focus
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Interruption {
    pub at: NaiveDateTime,
    /// "pause", "skip", "abandon" or "suspend".
    pub kind: String,
    pub reason: String,
}

/// One logged stretch of timer activity. A focus period that was split by
/// switching cards produces several records; only the last is `completed`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub kind: Phase,
    pub card_id: Option<Id>,
    pub started_at: NaiveDateTime,
    pub ended_at: NaiveDateTime,
    pub duration_s: i64,
    pub planned_s: i64,
    pub completed: bool,
    pub interruptions: Vec<Interruption>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub id: Id,
    pub kind: Phase,
    pub card_id: Option<Id>,
    pub card_title: Option<String>,
    pub started_at: NaiveDateTime,
    pub ended_at: NaiveDateTime,
    pub duration_s: i64,
    pub completed: bool,
    pub interruptions: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Holiday,
    Appointment,
    Deadline,
    /// A dated note: a title and a body, no time.
    Note,
}

impl ItemKind {
    pub const ALL: [ItemKind; 4] = [
        ItemKind::Holiday,
        ItemKind::Appointment,
        ItemKind::Deadline,
        ItemKind::Note,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Holiday => "holiday",
            ItemKind::Appointment => "appointment",
            ItemKind::Deadline => "deadline",
            ItemKind::Note => "note",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "holiday" => ItemKind::Holiday,
            "deadline" => ItemKind::Deadline,
            "note" => ItemKind::Note,
            _ => ItemKind::Appointment,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Holiday => "Holiday",
            ItemKind::Appointment => "Appointment",
            ItemKind::Deadline => "Deadline",
            ItemKind::Note => "Note",
        }
    }
    /// Only appointments and deadlines have a time of day.
    pub fn timed(self) -> bool {
        matches!(self, ItemKind::Appointment | ItemKind::Deadline)
    }
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Recur {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Recur {
    pub const ALL: [Recur; 5] = [
        Recur::None,
        Recur::Daily,
        Recur::Weekly,
        Recur::Monthly,
        Recur::Yearly,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Recur::None => "none",
            Recur::Daily => "daily",
            Recur::Weekly => "weekly",
            Recur::Monthly => "monthly",
            Recur::Yearly => "yearly",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "daily" => Recur::Daily,
            "weekly" => Recur::Weekly,
            "monthly" => Recur::Monthly,
            "yearly" => Recur::Yearly,
            _ => Recur::None,
        }
    }
}

impl std::fmt::Display for Recur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Recur::None => "Does not repeat",
            Recur::Daily => "Every day",
            Recur::Weekly => "Every week",
            Recur::Monthly => "Every month",
            Recur::Yearly => "Every year",
        })
    }
}

/// The calendar every database has; it cannot be deleted.
pub const DEFAULT_CALENDAR: Id = 1;

/// A named group of calendar items that can be shown or hidden together.
#[derive(Debug, Clone, PartialEq)]
pub struct Calendar {
    pub id: Id,
    pub name: String,
    /// `#rrggbb`; without one, items use the theme's colour for their kind.
    pub color: Option<String>,
    /// The zone new items in this calendar are written in.
    pub tz: Option<String>,
    pub visible: bool,
}

impl Calendar {
    pub fn new(name: impl Into<String>) -> Self {
        Calendar {
            id: 0,
            name: name.into(),
            color: None,
            tz: None,
            visible: true,
        }
    }
}

impl std::fmt::Display for Calendar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// A dated calendar entry that is not a kanban card.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: Id,
    pub kind: ItemKind,
    pub title: String,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub duration_min: Option<i64>,
    pub recur: Recur,
    pub recur_until: Option<NaiveDate>,
    pub done: bool,
    /// Identifier of the imported event, used to avoid duplicates.
    pub source: Option<String>,
    pub notes: String,
    pub calendar_id: Id,
    /// IANA zone the date and time are written in; `None` is local time.
    pub tz: Option<String>,
}

impl Item {
    pub fn new(kind: ItemKind, title: impl Into<String>, date: NaiveDate) -> Self {
        Item {
            id: 0,
            kind,
            title: title.into(),
            date,
            time: None,
            duration_min: None,
            recur: Recur::None,
            recur_until: None,
            done: false,
            source: None,
            notes: String::new(),
            calendar_id: DEFAULT_CALENDAR,
            tz: None,
        }
    }

    /// Drops the fields its kind has no use for: holidays and notes have no
    /// time, a duration needs a time, a deadline is one date with a done flag.
    pub fn normalise(&mut self) {
        if !self.kind.timed() {
            self.time = None;
        }
        if self.time.is_none() || self.kind != ItemKind::Appointment {
            self.duration_min = None;
        }
        if self.time.is_none() {
            self.tz = None;
        }
        if self.kind == ItemKind::Deadline {
            self.recur = Recur::None;
        } else {
            self.done = false;
        }
        if self.recur == Recur::None {
            self.recur_until = None;
        }
    }
}

/// A calendar item as text: what the command line, the socket and a bulk
/// import give. Everything is optional so the same shape describes a change
/// to an existing item.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemText {
    pub title: String,
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(alias = "repeat", skip_serializing_if = "Option::is_none")]
    pub recur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// "none", "-" or nothing at all clears an optional field.
fn clears(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "" | "none" | "-")
}

impl ItemText {
    /// Fills in what this row leaves out from `defaults`.
    pub fn or(mut self, defaults: &ItemText) -> ItemText {
        self.time = self.time.or_else(|| defaults.time.clone());
        self.duration = self.duration.or(defaults.duration);
        self.kind = self.kind.or_else(|| defaults.kind.clone());
        self.recur = self.recur.or_else(|| defaults.recur.clone());
        self.until = self.until.or_else(|| defaults.until.clone());
        self.calendar = self.calendar.or_else(|| defaults.calendar.clone());
        self.tz = self.tz.or_else(|| defaults.tz.clone());
        self
    }

    /// A new item, or what could not be read. A note without a date is for
    /// today; everything else needs one.
    pub fn build(&self, today: NaiveDate, calendars: &[Calendar]) -> Result<Item, String> {
        if self.title.trim().is_empty() {
            return Err("the title is empty".into());
        }
        let mut item = Item::new(ItemKind::Appointment, "", today);
        if let Some(c) = calendars.first() {
            item.calendar_id = c.id;
        }
        let is_note = self
            .kind
            .as_deref()
            .is_some_and(|k| k.trim().eq_ignore_ascii_case("note"));
        if self.date.trim().is_empty() && !is_note {
            return Err("a date is needed (try 2026-12-01, tomorrow, fri, Jan 5)".into());
        }
        self.apply(&mut item, today, calendars)?;
        Ok(item)
    }

    /// Changes the fields that are given and leaves the rest of `item` alone.
    pub fn apply(
        &self,
        item: &mut Item,
        today: NaiveDate,
        calendars: &[Calendar],
    ) -> Result<(), String> {
        use crate::core::{dates, tz};
        if !self.title.trim().is_empty() {
            item.title = self.title.trim().to_string();
        }
        if !self.date.trim().is_empty() {
            item.date = dates::parse_date(&self.date, today)
                .ok_or_else(|| format!("could not read the date \"{}\"", self.date))?;
        }
        if let Some(k) = &self.kind {
            let k = k.trim().to_ascii_lowercase();
            item.kind = match k.as_str() {
                "holiday" | "appointment" | "deadline" | "note" => ItemKind::parse(&k),
                _ => {
                    return Err(format!(
                        "unknown kind \"{k}\" (holiday, appointment, deadline, note)"
                    ));
                }
            };
        }
        let mut zone_set = false;
        if let Some(name) = &self.calendar {
            let cal = calendars
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| {
                    let names: Vec<&str> = calendars.iter().map(|c| c.name.as_str()).collect();
                    format!(
                        "there is no calendar called \"{name}\" (calendars: {})",
                        names.join(", ")
                    )
                })?;
            item.calendar_id = cal.id;
        }
        if let Some(t) = &self.time {
            if clears(t) {
                item.time = None;
            } else {
                let (time, zone) =
                    tz::parse_time(t).ok_or_else(|| format!("could not read the time \"{t}\""))?;
                item.time = Some(time);
                if let Some(z) = zone {
                    item.tz = Some(z.name().to_string());
                    zone_set = true;
                }
            }
        }
        if let Some(z) = &self.tz {
            item.tz = if clears(z) || z.trim().eq_ignore_ascii_case("local") {
                None
            } else {
                Some(tz::canonical(z).ok_or_else(|| {
                    format!("unknown time zone \"{z}\" (try America/New_York, Eastern, UTC)")
                })?)
            };
            zone_set = true;
        }
        // A new item takes the zone of its calendar unless it names its own.
        if !zone_set && item.id == 0 && item.tz.is_none() {
            item.tz = calendars
                .iter()
                .find(|c| c.id == item.calendar_id)
                .and_then(|c| c.tz.clone());
        }
        if let Some(d) = self.duration {
            if d == 0 {
                item.duration_min = None;
            } else if !(1..=24 * 60).contains(&d) {
                return Err("the duration is a number of minutes, up to a day".into());
            } else {
                item.duration_min = Some(d);
            }
        }
        if let Some(r) = &self.recur {
            let r = r.trim().to_ascii_lowercase();
            item.recur = match r.as_str() {
                "none" => Recur::None,
                "daily" | "weekly" | "monthly" | "yearly" => Recur::parse(&r),
                _ => {
                    return Err(format!(
                        "unknown repeat \"{r}\" (none, daily, weekly, monthly, yearly)"
                    ));
                }
            };
        }
        if let Some(u) = &self.until {
            item.recur_until = if clears(u) {
                None
            } else {
                Some(
                    dates::parse_date(u, today)
                        .ok_or_else(|| format!("could not read the end date \"{u}\""))?,
                )
            };
        }
        if let Some(n) = &self.notes {
            item.notes = n.trim().to_string();
        }
        if let Some(s) = &self.source {
            item.source = (!s.trim().is_empty()).then(|| format!("cli:{}", s.trim()));
        }
        item.normalise();
        if item.recur_until.is_some_and(|u| u < item.date) {
            return Err("the end date is before the start".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HabitKind {
    /// Done or not done.
    Check,
    /// A number to reach, e.g. 20 pages.
    Quantity,
    /// Completed automatically by focus minutes on a tag or board.
    Focus,
}

impl HabitKind {
    pub const ALL: [HabitKind; 3] = [HabitKind::Check, HabitKind::Quantity, HabitKind::Focus];
    pub fn as_str(self) -> &'static str {
        match self {
            HabitKind::Check => "check",
            HabitKind::Quantity => "quantity",
            HabitKind::Focus => "focus",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "quantity" => HabitKind::Quantity,
            "focus" => HabitKind::Focus,
            _ => HabitKind::Check,
        }
    }
}

impl std::fmt::Display for HabitKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            HabitKind::Check => "Yes / no",
            HabitKind::Quantity => "Quantity",
            HabitKind::Focus => "Focus time",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Schedule {
    Daily,
    /// Days of the week, 0 = Monday.
    Weekdays(Vec<u8>),
    /// Any N days in a Monday-to-Sunday week.
    PerWeek(u8),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Habit {
    pub id: Id,
    pub name: String,
    pub kind: HabitKind,
    /// 1 for Check, a quantity for Quantity, minutes for Focus.
    pub target: f64,
    pub unit: String,
    pub schedule: Schedule,
    pub link_tag: Option<String>,
    pub link_board: Option<Id>,
    pub rest_on_holidays: bool,
    pub skips_per_month: u32,
    pub created: NaiveDate,
    pub archived: bool,
}

impl Habit {
    pub fn new(name: impl Into<String>, created: NaiveDate) -> Self {
        Habit {
            id: 0,
            name: name.into(),
            kind: HabitKind::Check,
            target: 1.0,
            unit: String::new(),
            schedule: Schedule::Daily,
            link_tag: None,
            link_board: None,
            rest_on_holidays: false,
            skips_per_month: 2,
            created,
            archived: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct HabitEntry {
    pub value: f64,
    pub skipped: bool,
}
