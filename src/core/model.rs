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
}

impl ItemKind {
    pub const ALL: [ItemKind; 3] = [ItemKind::Holiday, ItemKind::Appointment, ItemKind::Deadline];
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Holiday => "holiday",
            ItemKind::Appointment => "appointment",
            ItemKind::Deadline => "deadline",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "holiday" => ItemKind::Holiday,
            "deadline" => ItemKind::Deadline,
            _ => ItemKind::Appointment,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Holiday => "Holiday",
            ItemKind::Appointment => "Appointment",
            ItemKind::Deadline => "Deadline",
        }
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
        }
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
