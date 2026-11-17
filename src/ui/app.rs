//! Application state, messages and the update loop. The views live in the
//! sibling modules as further `impl App` blocks.

use super::style::{self, P};
use crate::bridge::{self, Command, Status};
use crate::config::{self, Config};
use crate::core::db::Db;
use crate::core::habits::{self, Ctx, DayState, Streak};
use crate::core::intake::{self, Added, Intake};
use crate::core::model::*;
use crate::core::stats;
use crate::core::timer::{Effect, Timer};
use crate::core::{dates, recur};
use crate::theme::palette::{self, SystemInput};
use crate::theme::{Theme as Look, portal};
use crate::tray::{self, PomoTray, TrayColors};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime};
use iced::widget::text_editor;
use iced::{Element, Point, Subscription, Task, keyboard, window};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Instant;

pub type El<'a> = Element<'a, Message>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Today,
    Timer,
    Board,
    Calendar,
    Habits,
    Analytics,
    Settings,
}

impl View {
    pub const ALL: [View; 7] = [
        View::Today,
        View::Timer,
        View::Board,
        View::Calendar,
        View::Habits,
        View::Analytics,
        View::Settings,
    ];
    pub fn label(self) -> &'static str {
        match self {
            View::Today => "Today",
            View::Timer => "Timer",
            View::Board => "Board",
            View::Calendar => "Calendar",
            View::Habits => "Habits",
            View::Analytics => "Analytics",
            View::Settings => "Settings",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            View::Today => "today",
            View::Timer => "timer",
            View::Board => "board",
            View::Calendar => "calendar",
            View::Habits => "habits",
            View::Analytics => "analytics",
            View::Settings => "settings",
        }
    }
}

/// An entry in a pick list that carries an id.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub id: Option<Id>,
    pub label: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// Every plain text field in the app, so one message can serve them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Reason,
    ManualMin,
    BoardSearch,
    NewCard,
    Prompt,
    PaletteQuery,
    CardTitle,
    CardEstimate,
    CardDue,
    CardTags,
    ItemTitle,
    ItemDate,
    ItemTime,
    ItemDuration,
    ItemUntil,
    ItemTz,
    CalendarName,
    CalendarTz,
    HabitName,
    HabitTarget,
    HabitUnit,
    HabitTag,
    HabitSkips,
    HabitQty(Id),
    IcsPath,
    ExportPath,
    ImportPath,
    SetFocus,
    SetShort,
    SetLong,
    SetGoal,
    SetRatio,
    SetFont,
    SetLead,
    SetHabitHour,
    SetThemeName,
    Override(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    AutoBreaks,
    AutoFocus,
    Flow,
    Compact,
    Notifications,
    DailySummary,
    Sounds,
    SoundFocusEnd,
    SoundBreakEnd,
    SoundAutoStart,
    SoundWarning,
    Vim,
    HabitRest,
    HabitArchived,
    ItemDone,
    CalWeek,
    CalFilter(usize),
    HabitWeekday(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptAction {
    NewBoard,
    RenameBoard(Id),
    NewList,
    RenameList(Id),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    DeleteBoard(Id),
    DeleteList(Id),
    DeleteHabit(Id),
    DeleteCard(Id),
    DeleteCalendar(Id),
    ImportData,
    WipeData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedKind {
    Daily,
    Weekdays,
    PerWeek,
}

impl SchedKind {
    pub const ALL: [SchedKind; 3] = [SchedKind::Daily, SchedKind::Weekdays, SchedKind::PerWeek];
}

impl std::fmt::Display for SchedKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SchedKind::Daily => "Every day",
            SchedKind::Weekdays => "On chosen weekdays",
            SchedKind::PerWeek => "A number of times a week",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Today,
    Week,
    Month,
    Days30,
    Days90,
    Year,
}

impl Range {
    pub const ALL: [Range; 6] = [
        Range::Today,
        Range::Week,
        Range::Month,
        Range::Days30,
        Range::Days90,
        Range::Year,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Range::Today => "Today",
            Range::Week => "This week",
            Range::Month => "This month",
            Range::Days30 => "30 days",
            Range::Days90 => "90 days",
            Range::Year => "Year",
        }
    }
    pub fn bounds(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let from = match self {
            Range::Today => today,
            Range::Week => dates::week_start(today),
            Range::Month => dates::month_start(today),
            Range::Days30 => today - Duration::days(29),
            Range::Days90 => today - Duration::days(89),
            Range::Year => today - Duration::days(364),
        };
        (from, today)
    }
}

pub struct CardEdit {
    pub id: Id,
    pub title: String,
    pub notes: text_editor::Content,
    pub estimate: String,
    pub due: String,
    pub tags: String,
    pub list: Id,
    /// The lists of the card's board, for moving it.
    pub lists: Vec<Choice>,
}

pub struct ItemEdit {
    pub id: Id,
    pub kind: ItemKind,
    pub title: String,
    pub date: String,
    pub time: String,
    pub duration: String,
    pub recur: Recur,
    pub until: String,
    pub notes: text_editor::Content,
    pub done: bool,
    pub source: Option<String>,
    pub calendar: Id,
    /// Zone the time is written in; empty for local time.
    pub tz: String,
}

#[derive(Debug, Clone)]
pub struct CalendarEdit {
    pub id: Id,
    pub name: String,
    pub color: Option<String>,
    pub tz: String,
}

#[derive(Debug, Clone)]
pub struct HabitEdit {
    pub id: Id,
    pub name: String,
    pub kind: HabitKind,
    pub target: String,
    pub unit: String,
    pub sched: SchedKind,
    pub weekdays: [bool; 7],
    pub per_week: u8,
    pub tag: String,
    pub board: Option<Id>,
    pub rest: bool,
    pub skips: String,
    pub archived: bool,
    pub created: NaiveDate,
}

pub enum Modal {
    None,
    Card(Box<CardEdit>),
    Item(ItemEdit),
    Habit(HabitEdit),
    Prompt {
        title: &'static str,
        value: String,
        action: PromptAction,
    },
    Confirm {
        text: String,
        action: ConfirmAction,
    },
    Palette {
        query: String,
    },
    Calendar(CalendarEdit),
    /// The list of keyboard shortcuts.
    Keys,
}

#[derive(Debug, Clone, Default)]
pub struct Drag {
    pub card: Id,
    pub start: Option<Point>,
    pub pos: Point,
    pub active: bool,
    pub over_list: Option<Id>,
    pub over_card: Option<Id>,
}

/// Something the command palette can run.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Go(View),
    TimerToggle,
    TimerSkip,
    Zen,
    StartCard(Id),
    /// Free text for the intake: a card, an appointment, a deadline or a note.
    Intake(String),
    Theme(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    External(Command),
    WindowOpened(window::Id),
    WindowClosed(window::Id),
    Resized(iced::Size),
    Key(keyboard::Key, keyboard::Modifiers, bool),
    Cursor(Point),
    MouseUp,
    Go(View),
    Field(Field, String),
    Submit(Field),
    Toggle(Toggle, bool),
    CloseModal,
    ConfirmYes,
    PromptOpen(PromptAction),
    Ask(ConfirmAction),
    Run(Action),
    Noop,

    // Timer
    TimerToggle,
    TimerSkip,
    TimerAbandon,
    TimerFinish,
    TimerExtend(i64),
    TimerCard(Choice),
    StartCard(Id),
    Zen(bool),
    SessionDelete(Id),
    ManualAdd,

    // Board
    BoardPick(Choice),
    CardAddTo(Option<Id>),
    CardPress(Id),
    CardHover(Id, bool),
    ListHover(Id, bool),
    CardOpen(Id),
    CardNotes(text_editor::Action),
    CardList(Choice),
    CardSave,

    // Calendar
    CalShift(i32),
    CalToday,
    CalSelect(NaiveDate),
    ItemNew(ItemKind, NaiveDate),
    ItemOpen(Id),
    ItemKindSet(ItemKind),
    ItemRecur(Recur),
    ItemSave,
    ItemDelete(Id),
    ItemDone(Id, bool),
    IcsKind(ItemKind),
    IcsImport,
    IcsCalendar(Calendar),
    ItemCalendar(Calendar),
    ItemNotes(text_editor::Action),
    CalendarNew,
    CalendarOpen(Id),
    CalendarColor(Option<String>),
    CalendarSave,
    CalendarVisible(Id, bool),

    // Habits
    HabitNew,
    HabitOpen(Id),
    HabitKindSet(HabitKind),
    HabitSched(SchedKind),
    HabitPerWeek(u8),
    HabitBoard(Choice),
    HabitSave,
    HabitCheck(Id, NaiveDate, bool),
    HabitSkip(Id, NaiveDate, bool),

    // Analytics
    AnaRange(Range),

    // Settings
    SetTheme(String),
    SetMode(config::Mode),
    SetSource(config::Source),
    SetClock(config::Clock),
    SetRadius(f32),
    SetScale(f32),
    SetVolume(f32),
    TestSound,
    SetEveryN(u32),
    OverrideClear(&'static str),
    ThemeSaveAs,
    Export,
    Import,
}

pub struct Analytics {
    pub range: Range,
    pub summary: stats::Summary,
    pub days: BTreeMap<NaiveDate, stats::Day>,
    pub by_card: Vec<(String, i64)>,
    pub by_board: Vec<(String, i64)>,
    pub by_tag: Vec<(String, i64)>,
    pub hours: [i64; 24],
    pub reasons: Vec<(String, i64)>,
    pub estimates: Vec<stats::Estimate>,
    /// Share of due habit days that were done, if any were due.
    pub habit_rate: Option<f32>,
}

#[derive(Default)]
pub struct SettingsUi {
    pub focus: String,
    pub short: String,
    pub long: String,
    pub goal: String,
    pub ratio: String,
    pub font: String,
    pub lead: String,
    pub habit_hour: String,
    pub theme_name: String,
    pub export_path: String,
    pub import_path: String,
    pub overrides: BTreeMap<String, String>,
    pub themes: Vec<String>,
    pub config_error: Option<String>,
}

pub struct App {
    pub db: Db,
    pub cfg: Config,
    pub look: Look,
    pub p: P,
    pub iced_theme: iced::Theme,
    sys: SystemInput,
    theme_fp: u64,
    last_poll: Instant,
    polls: u32,
    tray: Option<ksni::blocking::Handle<PomoTray>>,
    last_status: Status,
    window: Option<window::Id>,
    /// False in tests: nothing is written or sent outside the database.
    live: bool,
    /// Logical size of the window, for choosing a layout.
    pub size: iced::Size,

    pub view: View,
    pub modal: Modal,
    pub zen: bool,
    pub toast: Option<(String, bool, Instant)>,
    pub now: NaiveDateTime,
    pub today: NaiveDate,
    pub timer: Timer,
    pub reason: String,
    pub manual_min: String,
    notified: HashSet<String>,
    /// The one-minute warning already sounded for the running phase.
    minute_warned: bool,

    // Data, reloaded after changes.
    pub boards: Vec<Board>,
    pub board_id: Option<Id>,
    pub lists: Vec<List>,
    pub cards: Vec<Card>,
    pub in_progress: Vec<(Card, String)>,
    pub items: Vec<Item>,
    pub calendars: Vec<Calendar>,
    /// Calendars switched off in the calendar view.
    pub hidden: HashSet<Id>,
    /// `PRAGMA data_version` when the data was last loaded.
    data_version: i64,
    last_data_check: Instant,
    pub due_cards: Vec<(Card, String, bool)>,
    pub holidays: HashSet<NaiveDate>,
    pub habits: Vec<Habit>,
    /// Stored entries merged with focus time for focus-linked habits.
    pub entries: HashMap<Id, HashMap<NaiveDate, HabitEntry>>,
    stored_entries: HashMap<Id, HashMap<NaiveDate, HabitEntry>>,
    pub streaks: HashMap<Id, Streak>,
    pub day_stats: BTreeMap<NaiveDate, stats::Day>,
    pub today_sessions: Vec<SessionRow>,

    // View state.
    pub search: String,
    pub adding_to: Option<Id>,
    pub new_card: String,
    pub drag: Option<Drag>,
    pub cal_month: NaiveDate,
    pub cal_week: bool,
    pub cal_selected: NaiveDate,
    pub cal_filters: [bool; 5],
    pub ics_path: String,
    pub ics_kind: ItemKind,
    pub ics_calendar: Id,
    /// Keyboard selection on the board: a card, and the list it is in.
    pub board_sel: Option<Id>,
    pub board_list: usize,
    pub habit_sel: Option<Id>,
    /// A `g` was pressed and the next key picks a view.
    pub(super) pending_g: bool,
    pub habit_qty: HashMap<Id, String>,
    pub ana: Analytics,
    pub set: SettingsUi,
}

pub(super) fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

fn expand(path: &str) -> std::path::PathBuf {
    let p = path.trim();
    match p.strip_prefix("~/") {
        Some(rest) => {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest)
        }
        None => std::path::PathBuf::from(p),
    }
}

fn leak_font(name: &str) -> iced::Font {
    iced::Font::with_name(Box::leak(name.to_string().into_boxed_str()))
}

impl App {
    /// The font to start the toolkit with, resolved before the app exists.
    pub fn startup_font() -> Option<iced::Font> {
        let (cfg, _) = Config::load();
        let sys = SystemInput::read(&config::system_config_home());
        let look = palette::resolve(&sys, &cfg.appearance, &cfg.overrides, &config::themes_dir());
        look.font.as_deref().map(leak_font)
    }

    pub fn boot() -> (App, Task<Message>) {
        let (cfg, config_error) = Config::load();
        let (db, db_error) = match Db::open(&config::db_path()) {
            Ok(db) => (db, None),
            Err(e) => (
                Db::open_in_memory().expect("in-memory database"),
                Some(format!(
                    "Could not open the database ({e}). Nothing will be saved this session."
                )),
            ),
        };
        let sys = SystemInput::read(&config::system_config_home());
        let mut app = Self::build(db, cfg, config_error, sys);
        app.live = true;
        if let Some(e) = db_error {
            app.toast(e, true);
        }
        app.tray = tray::spawn(app.tray_colors());
        app.publish_status();

        let (id, open) = window::open(Self::window_settings());
        app.window = Some(id);
        (app, open.map(Message::WindowOpened))
    }

    /// The app without any of its outside connections: no window, tray,
    /// notifications or config writes until `live` is set.
    pub fn build(db: Db, cfg: Config, config_error: Option<String>, sys: SystemInput) -> App {
        let look = palette::resolve(&sys, &cfg.appearance, &cfg.overrides, &config::themes_dir());
        let now = now();
        let today = now.date();
        let mut app = App {
            live: false,
            p: P::new(&look),
            iced_theme: style::iced_theme(&look),
            theme_fp: 0,
            last_poll: Instant::now(),
            polls: 0,
            tray: None,
            last_status: Status::default(),
            window: None,
            size: Self::window_settings().size,
            view: View::Today,
            modal: Modal::None,
            zen: false,
            toast: None,
            now,
            today,
            timer: Timer::new(cfg.timer.to_cfg()),
            reason: String::new(),
            manual_min: String::new(),
            notified: HashSet::new(),
            minute_warned: false,
            boards: Vec::new(),
            board_id: None,
            lists: Vec::new(),
            cards: Vec::new(),
            in_progress: Vec::new(),
            items: Vec::new(),
            calendars: Vec::new(),
            hidden: HashSet::new(),
            data_version: 0,
            last_data_check: Instant::now(),
            due_cards: Vec::new(),
            holidays: HashSet::new(),
            habits: Vec::new(),
            entries: HashMap::new(),
            stored_entries: HashMap::new(),
            streaks: HashMap::new(),
            day_stats: BTreeMap::new(),
            today_sessions: Vec::new(),
            search: String::new(),
            adding_to: None,
            new_card: String::new(),
            drag: None,
            cal_month: dates::month_start(today),
            cal_week: false,
            cal_selected: today,
            cal_filters: [true; 5],
            ics_path: String::new(),
            ics_kind: ItemKind::Holiday,
            ics_calendar: DEFAULT_CALENDAR,
            board_sel: None,
            board_list: 0,
            habit_sel: None,
            pending_g: false,
            habit_qty: HashMap::new(),
            ana: Analytics {
                range: Range::Week,
                summary: Default::default(),
                days: BTreeMap::new(),
                by_card: Vec::new(),
                by_board: Vec::new(),
                by_tag: Vec::new(),
                hours: [0; 24],
                reasons: Vec::new(),
                estimates: Vec::new(),
                habit_rate: None,
            },
            set: SettingsUi {
                config_error,
                ..Default::default()
            },
            db,
            cfg,
            look,
            sys,
        };
        app.theme_fp = app.fingerprint();
        if app.db.boards().map(|b| b.is_empty()).unwrap_or(false) {
            let _ = app.db.add_board("Tasks");
        }
        app.board_id = app.db.meta("board").and_then(|v| v.parse().ok());
        app.sync_settings_ui();
        app.reload_all();
        app.data_version = app.db.data_version().unwrap_or(0);
        app
    }

    fn window_settings() -> window::Settings {
        window::Settings {
            size: iced::Size::new(1180.0, 760.0),
            min_size: Some(iced::Size::new(420.0, 420.0)),
            platform_specific: window::settings::PlatformSpecific {
                application_id: config::APP.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn title(&self, _: window::Id) -> String {
        match &self.timer.active {
            Some(a) if a.open_ended => {
                format!("{} · {}", dates::clock(a.elapsed_s()), config::APP_TITLE)
            }
            Some(a) => format!("{} · {}", dates::clock(a.remaining_s()), config::APP_TITLE),
            None => config::APP_TITLE.to_string(),
        }
    }

    pub fn scale(&self, _: window::Id) -> f32 {
        let s = self.cfg.appearance.scale;
        if s.is_finite() {
            s.clamp(0.5, 2.5)
        } else {
            1.0
        }
    }

    pub fn theme(&self, _: window::Id) -> iced::Theme {
        self.iced_theme.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let tick = if self.timer.is_running() { 250 } else { 1000 };
        let mut subs = vec![
            iced::time::every(std::time::Duration::from_millis(tick)).map(|_| Message::Tick),
            window::close_events().map(Message::WindowClosed),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            iced::event::listen_with(key_events),
            Subscription::run(external_commands),
        ];
        if self.drag.is_some() {
            subs.push(iced::event::listen_with(drag_events));
        }
        Subscription::batch(subs)
    }

    /// Whether the sidebar collapses to icons.
    pub fn rail(&self) -> bool {
        self.size.width < 920.0
    }

    /// Whether the content area has room for side-by-side columns.
    pub fn wide(&self) -> bool {
        self.size.width - if self.rail() { 60.0 } else { 212.0 } >= 800.0
    }

    // ---- theme ----

    fn fingerprint(&self) -> u64 {
        palette::fingerprint(&self.sys, &[config::config_path(), config::themes_dir()])
    }

    fn tray_colors(&self) -> TrayColors {
        let c = &self.look.c;
        TrayColors {
            focus: c.focus,
            short_break: c.short_break,
            long_break: c.long_break,
            idle: c.text_dim,
        }
    }

    /// Re-reads the system theme and rebuilds the palette.
    pub fn retheme(&mut self) {
        self.sys = SystemInput::read(&config::system_config_home());
        self.restyle();
        self.theme_fp = self.fingerprint();
    }

    /// Rebuilds the palette from what is already loaded.
    pub(super) fn restyle(&mut self) {
        self.look = palette::resolve(
            &self.sys,
            &self.cfg.appearance,
            &self.cfg.overrides,
            &config::themes_dir(),
        );
        self.p = P::new(&self.look);
        self.iced_theme = style::iced_theme(&self.look);
        self.set.themes = palette::available(&config::themes_dir());
        let colors = self.tray_colors();
        if let Some(t) = &self.tray {
            t.update(|t| t.colors = colors);
        }
    }

    fn poll_theme(&mut self) {
        if !self.live || self.last_poll.elapsed().as_secs_f32() < 2.0 {
            return;
        }
        self.last_poll = Instant::now();
        self.polls += 1;
        let mut changed = self.fingerprint() != self.theme_fp;
        if !changed && self.polls.is_multiple_of(5) {
            changed = portal::color_scheme() != self.sys.scheme;
        }
        if changed {
            // The config may have been edited by hand or by home-manager.
            let (cfg, err) = Config::load();
            if err.is_none() && cfg != self.cfg {
                self.cfg = cfg;
                self.timer.cfg = self.cfg.timer.to_cfg();
                self.sync_settings_ui();
            }
            self.set.config_error = err;
            self.retheme();
        }
    }

    pub fn sync_settings_ui(&mut self) {
        let num = |v: f64| {
            if v.fract() == 0.0 {
                format!("{v:.0}")
            } else {
                format!("{v}")
            }
        };
        let t = &self.cfg.timer;
        self.set.focus = num(t.focus_min);
        self.set.short = num(t.short_break_min);
        self.set.long = num(t.long_break_min);
        self.set.goal = t.daily_goal.to_string();
        self.set.ratio = num((t.flow_break_ratio * 100.0).round());
        self.set.font = self.cfg.appearance.font.clone();
        self.set.lead = self.cfg.notifications.appointment_lead_min.to_string();
        self.set.habit_hour = self.cfg.notifications.habit_reminder_hour.to_string();
        self.set.overrides = self.cfg.overrides.clone();
        self.set.themes = palette::available(&config::themes_dir());
        if self.set.export_path.is_empty() {
            self.set.export_path = format!("~/pomolog-export-{}.json", self.today);
        }
    }

    pub fn save_cfg(&mut self) {
        if !self.live {
            return;
        }
        match self.cfg.save() {
            Ok(()) => self.theme_fp = self.fingerprint(),
            Err(e) => self.toast(format!("Settings not saved: {e}"), true),
        }
    }

    // ---- data ----

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some((text.into(), error, Instant::now()));
    }

    /// Reports a failed database call instead of dropping it.
    pub fn check<T>(&mut self, r: rusqlite::Result<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.toast(format!("Database error: {e}"), true);
                None
            }
        }
    }

    pub fn reload_all(&mut self) {
        self.reload_board();
        self.reload_calendar();
        self.reload_sessions();
    }

    pub fn reload_board(&mut self) {
        self.boards = self.db.boards().unwrap_or_default();
        if !self.boards.iter().any(|b| Some(b.id) == self.board_id) {
            self.board_id = self.boards.first().map(|b| b.id);
        }
        match self.board_id {
            Some(b) => {
                self.lists = self.db.lists(b).unwrap_or_default();
                self.cards = self.db.cards(b).unwrap_or_default();
            }
            None => {
                self.lists.clear();
                self.cards.clear();
            }
        }
        self.in_progress = self.db.in_progress().unwrap_or_default();
        self.due_cards = self.db.due_cards().unwrap_or_default();
        // The chosen task may have been deleted or finished.
        if let Some(id) = self.timer.card_id
            && !self.in_progress.iter().any(|(c, _)| c.id == id)
            && self.db.card(id).ok().flatten().is_none()
        {
            let fx = self.timer.set_card(now(), None);
            self.apply(fx);
        }
    }

    /// Reloads everything if another process (the command line, usually)
    /// has written to the database since the last look. Returns whether it
    /// had.
    pub fn sync_outside(&mut self) -> bool {
        match self.db.data_version() {
            Ok(v) if v != self.data_version => {
                self.data_version = v;
                self.reload_all();
                true
            }
            _ => false,
        }
    }

    /// A time of day the way the settings ask for it.
    pub fn time_text(&self, t: chrono::NaiveTime) -> String {
        dates::fmt_time(t, self.cfg.appearance.clock == config::Clock::H12)
    }

    /// What is on a local day, leaving out calendars that are switched off.
    pub fn day_items(&self, d: NaiveDate) -> Vec<recur::Occ<'_>> {
        let mut v = recur::occurrences_on(&self.items, d);
        v.retain(|o| !self.hidden.contains(&o.item.calendar_id));
        v
    }

    pub fn reload_calendar(&mut self) {
        self.items = self.db.items().unwrap_or_default();
        self.calendars = self.db.calendars().unwrap_or_default();
        self.hidden = self
            .calendars
            .iter()
            .filter(|c| !c.visible)
            .map(|c| c.id)
            .collect();
        if !self.calendars.iter().any(|c| c.id == self.ics_calendar) {
            self.ics_calendar = DEFAULT_CALENDAR;
        }
        self.holidays = recur::holidays_between(
            &self.items,
            self.today - Duration::days(800),
            self.today + Duration::days(400),
        );
        self.reload_habits();
    }

    pub fn reload_habits(&mut self) {
        self.habits = self.db.habits().unwrap_or_default();
        self.stored_entries = self.db.habit_entries().unwrap_or_default();
        self.entries = self.stored_entries.clone();
        for h in &self.habits {
            if h.kind != HabitKind::Focus {
                continue;
            }
            let board = h
                .link_board
                .and_then(|id| self.boards.iter().find(|b| b.id == id));
            let auto = stats::focus_minutes_by_day(
                &self.db,
                h.link_tag.as_deref(),
                board.map(|b| b.name.as_str()),
            )
            .unwrap_or_default();
            let map = self.entries.entry(h.id).or_default();
            for (d, mins) in auto {
                let e = map.entry(d).or_default();
                e.value = e.value.max(mins);
            }
        }
        self.streaks = self
            .habits
            .iter()
            .map(|h| (h.id, habits::streak(h, &self.habit_ctx(h.id))))
            .collect();
    }

    pub fn reload_sessions(&mut self) {
        self.today_sessions = self
            .db
            .sessions_between(self.today, self.today)
            .unwrap_or_default();
        self.day_stats = stats::by_day(&self.db, self.today - Duration::days(420), self.today)
            .unwrap_or_default();
        self.reload_analytics();
    }

    pub fn reload_analytics(&mut self) {
        let (from, to) = self.ana.range.bounds(self.today);
        let goal = self.cfg.timer.daily_goal;
        let focus_s = self.timer.cfg.focus_s;
        let db = &self.db;
        self.ana.summary = stats::summary(db, from, to, goal).unwrap_or_default();
        self.ana.days = stats::by_day(db, from, to).unwrap_or_default();
        self.ana.by_card = stats::by_card(db, from, to).unwrap_or_default();
        self.ana.by_board = stats::by_board(db, from, to).unwrap_or_default();
        self.ana.by_tag = stats::by_tag(db, from, to).unwrap_or_default();
        self.ana.hours = stats::by_hour(db, from, to).unwrap_or([0; 24]);
        self.ana.reasons = stats::reasons(db, from, to).unwrap_or_default();
        self.ana.estimates = stats::estimates(db, from, to, focus_s).unwrap_or_default();
        let (mut done, mut due) = (0u32, 0u32);
        for h in self.habits.iter().filter(|h| !h.archived) {
            let cx = self.habit_ctx(h.id);
            let mut d = from.max(h.created);
            while d <= to {
                match habits::day_state(h, d, &cx) {
                    DayState::Done => {
                        done += 1;
                        due += 1;
                    }
                    DayState::Missed => due += 1,
                    _ => {}
                }
                d = d.succ_opt().unwrap();
            }
        }
        self.ana.habit_rate = (due > 0).then(|| done as f32 / due as f32);
    }

    pub fn habit_ctx(&self, habit: Id) -> Ctx<'_> {
        static EMPTY: std::sync::OnceLock<HashMap<NaiveDate, HabitEntry>> =
            std::sync::OnceLock::new();
        Ctx {
            entries: self
                .entries
                .get(&habit)
                .unwrap_or_else(|| EMPTY.get_or_init(HashMap::new)),
            holidays: &self.holidays,
            today: self.today,
        }
    }

    pub fn stored_entries_of(&self, habit: Id) -> Option<&HashMap<NaiveDate, HabitEntry>> {
        self.stored_entries.get(&habit)
    }

    pub fn stored_entry(&self, habit: Id, d: NaiveDate) -> HabitEntry {
        self.stored_entries
            .get(&habit)
            .and_then(|m| m.get(&d))
            .copied()
            .unwrap_or_default()
    }

    pub fn completed_today(&self) -> u32 {
        self.day_stats.get(&self.today).map_or(0, |d| d.completed)
    }

    pub fn focus_today(&self) -> i64 {
        let live = match &self.timer.active {
            Some(a) if a.phase == Phase::Focus => a.elapsed_s(),
            _ => 0,
        };
        // The running session is not logged yet; count it as it happens.
        let since_switch = self
            .timer
            .active
            .as_ref()
            .map_or(0, |a| a.elapsed_s() - a.segment_s());
        self.day_stats.get(&self.today).map_or(0, |d| d.focus_s) + live - since_switch
    }

    pub fn card_title(&self, id: Id) -> Option<String> {
        self.in_progress
            .iter()
            .map(|(c, _)| c)
            .chain(self.cards.iter())
            .find(|c| c.id == id)
            .map(|c| c.title.clone())
            .or_else(|| self.db.card(id).ok().flatten().map(|c| c.title))
    }

    // ---- timer plumbing ----

    pub fn apply(&mut self, effects: Vec<Effect>) {
        let mut logged = false;
        let mut ended = false;
        for e in effects {
            match e {
                Effect::Log(r) => {
                    let res = self.db.insert_session(&r);
                    self.check(res);
                    logged = true;
                }
                Effect::Ended { phase, completed } => {
                    self.reason.clear();
                    ended = true;
                    if completed && self.live {
                        let cue = match phase {
                            Phase::Focus => crate::sound::Cue::FocusEnd,
                            _ => crate::sound::Cue::BreakEnd,
                        };
                        crate::sound::play(cue, &self.cfg.sounds);
                    }
                    if completed && self.live && self.cfg.notifications.enabled {
                        let (title, body) = match phase {
                            Phase::Focus => ("Focus session complete", "Time for a break."),
                            _ => ("Break over", "Ready for the next focus session?"),
                        };
                        crate::notify::send(title, body);
                    }
                }
                Effect::Started(_) => {
                    self.reason.clear();
                    self.minute_warned = false;
                    // Started right after an end in the same batch means
                    // auto-start; a manual start comes on its own.
                    if ended && self.live {
                        crate::sound::play(crate::sound::Cue::AutoStart, &self.cfg.sounds);
                    }
                }
            }
        }
        if logged {
            self.reload_board();
            self.reload_habits();
            self.reload_sessions();
        }
        self.publish_status();
    }

    /// Sounds once when a timed phase reaches its last minute. Phases of 90
    /// seconds or less are too short for a warning to be useful.
    fn minute_warning(&mut self) {
        let Some(a) = &self.timer.active else { return };
        if self.minute_warned || a.paused || a.open_ended || a.planned_s <= 90 {
            return;
        }
        if a.remaining_s() <= 60 {
            self.minute_warned = true;
            if self.live {
                crate::sound::play(crate::sound::Cue::Warning, &self.cfg.sounds);
            }
        }
    }

    fn publish_status(&mut self) {
        let t = &self.timer;
        let phase = t.phase();
        let (state, remaining, elapsed, planned, progress, open) = match &t.active {
            Some(a) => (
                if a.paused { "paused" } else { "running" },
                a.remaining_s(),
                a.elapsed_s(),
                a.planned_s,
                a.progress(),
                a.open_ended,
            ),
            None => (
                "idle",
                t.planned_for(phase),
                0,
                t.planned_for(phase),
                0.0,
                false,
            ),
        };
        let status = Status {
            state: state.into(),
            phase: phase.as_str().into(),
            remaining_s: remaining,
            elapsed_s: elapsed,
            planned_s: planned,
            // Rounded so the tray is not redrawn for invisible changes.
            progress: (progress * 200.0).round() / 200.0,
            open_ended: open,
            task: t
                .active
                .as_ref()
                .map_or(t.card_id, |a| a.card_id)
                .and_then(|id| self.card_title(id)),
            completed_today: self.completed_today(),
            daily_goal: self.cfg.timer.daily_goal,
            text: dates::clock(if open { elapsed } else { remaining }),
        };
        if status != self.last_status {
            bridge::set_status(status.clone());
            if let Some(tray) = &self.tray {
                let s = status.clone();
                tray.update(move |t| t.status = s);
            }
            self.last_status = status;
        }
    }

    /// Sends the notifications that are due. Each fires once.
    fn reminders(&mut self) {
        let n = self.cfg.notifications.clone();
        if !n.enabled || !self.live {
            return;
        }
        let today = self.today;
        let today_s = today.to_string();
        if n.daily_summary && self.db.meta("summary_date").as_deref() != Some(&today_s) {
            let _ = self.db.set_meta("summary_date", &today_s);
            let mut lines: Vec<String> = self
                .day_items(today)
                .into_iter()
                .filter(|o| !(o.item.kind == ItemKind::Deadline && o.item.done))
                .map(|o| match o.time {
                    Some(t) => format!("{} {}", self.time_text(t), o.item.title),
                    None => o.item.title.clone(),
                })
                .collect();
            lines.extend(
                self.due_cards
                    .iter()
                    .filter(|(c, _, done)| !done && c.due.is_some_and(|d| d <= today))
                    .map(|(c, _, _)| format!("Due: {}", c.title)),
            );
            if !lines.is_empty() {
                crate::notify::send("Today", lines.join("\n"));
            }
        }
        if n.appointment_lead_min > 0 {
            let lead = Duration::minutes(n.appointment_lead_min as i64);
            let due: Vec<(String, String)> = self
                .day_items(today)
                .into_iter()
                .filter(|o| o.item.kind == ItemKind::Appointment)
                .filter_map(|o| {
                    let at = today.and_time(o.time?);
                    (self.now >= at - lead && self.now < at).then(|| {
                        (
                            format!("appt:{}:{today}", o.item.id),
                            format!("{} at {}", o.item.title, self.time_text(at.time())),
                        )
                    })
                })
                .collect();
            for (key, text) in due {
                if self.notified.insert(key) {
                    crate::notify::send("Coming up", text);
                }
            }
        }
        if n.habit_reminder_hour < 24
            && chrono::Timelike::hour(&self.now) >= n.habit_reminder_hour
            && self.db.meta("habit_reminder_date").as_deref() != Some(&today_s)
        {
            let _ = self.db.set_meta("habit_reminder_date", &today_s);
            let open: Vec<&str> = self
                .habits
                .iter()
                .filter(|h| {
                    !h.archived
                        && self
                            .streaks
                            .get(&h.id)
                            .is_some_and(|s| s.at_risk && s.current > 0)
                })
                .map(|h| h.name.as_str())
                .collect();
            if !open.is_empty() {
                crate::notify::send("Keep the chain going", open.join(", "));
            }
        }
    }

    fn external(&mut self, cmd: Command) -> Task<Message> {
        let now = now();
        match cmd {
            Command::Toggle => return self.update(Message::TimerToggle),
            Command::Start => {
                let fx = self.timer.start(now);
                self.apply(fx);
            }
            Command::Pause => {
                let fx = self.timer.pause(now, "");
                self.apply(fx);
            }
            Command::Skip => return self.update(Message::TimerSkip),
            Command::Abandon => return self.update(Message::TimerAbandon),
            Command::Status => {}
            Command::Show { view } => {
                if let Some(name) = view {
                    match View::ALL
                        .iter()
                        .find(|v| v.label().eq_ignore_ascii_case(name.trim()))
                    {
                        Some(v) => {
                            self.view = *v;
                            self.modal = Modal::None;
                            if *v == View::Analytics {
                                self.reload_analytics();
                            }
                        }
                        None => self.toast(format!("There is no view called \"{name}\"."), true),
                    }
                }
                return match self.window {
                    Some(id) => window::gain_focus(id),
                    None => {
                        let (id, open) = window::open(Self::window_settings());
                        self.window = Some(id);
                        open.map(Message::WindowOpened)
                    }
                };
            }
            Command::Quit => return self.quit(),
            Command::Add { title, due, board } => {
                self.quick_add(&title, due.as_deref(), board.as_deref())
            }
            Command::Event(text) => self.add_item_text(&text),
        }
        Task::none()
    }

    fn quit(&mut self) -> Task<Message> {
        // Do not lose a session in progress.
        let fx = self.timer.pause(now(), "quit");
        self.apply(fx);
        if self
            .timer
            .active
            .as_ref()
            .is_some_and(|a| a.phase == Phase::Focus)
        {
            let fx = self.timer.abandon(now(), "quit");
            self.apply(fx);
        }
        crate::ipc::cleanup(&config::socket_path());
        iced::exit()
    }

    /// Adds whatever a line of text describes: see `intake::parse`.
    pub fn intake(&mut self, text: &str) {
        match intake::parse(text, self.today) {
            Err(e) => self.toast(e, true),
            Ok(Intake::Item(t)) => self.add_item_text(&t),
            Ok(Intake::Card {
                title,
                due,
                board,
                tags,
            }) => {
                // Without a board in the text, the one on screen.
                let current = self
                    .boards
                    .iter()
                    .find(|b| Some(b.id) == self.board_id)
                    .map(|b| b.name.clone());
                let board = board.or(current);
                match intake::add_card(&self.db, &title, due, board.as_deref(), &tags, now()) {
                    Ok((_, place)) => {
                        self.reload_board();
                        self.toast(format!("Added \"{title}\" to {place}"), false);
                    }
                    Err(e) => self.toast(e, true),
                }
            }
        }
    }

    pub fn add_item_text(&mut self, text: &ItemText) {
        match intake::add_item(&self.db, text, self.today, false) {
            Ok(Added::Duplicate(i)) => {
                self.toast(format!("\"{}\" is already in the calendar", i.title), false)
            }
            Ok(added) => {
                let title = added.item().title.clone();
                self.reload_calendar();
                self.toast(format!("Added \"{title}\" to the calendar"), false);
            }
            Err(e) => self.toast(e, true),
        }
    }

    /// Adds a card to the first list of a board, by board name when given.
    pub fn quick_add(&mut self, title: &str, due: Option<&str>, board: Option<&str>) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        let board_id = board
            .and_then(|name| {
                self.boards
                    .iter()
                    .find(|b| b.name.eq_ignore_ascii_case(name.trim()))
            })
            .map(|b| b.id)
            .or(self.board_id);
        let Some(board_id) = board_id else { return };
        let list = self
            .db
            .lists(board_id)
            .ok()
            .and_then(|l| l.first().map(|l| l.id));
        let Some(list) = list else { return };
        let res = self.db.add_card(list, title, now());
        let Some(id) = self.check(res) else { return };
        let mut note = String::new();
        if let Some(text) = due {
            match dates::parse_date(text, self.today) {
                Some(d) => {
                    if let Ok(Some(mut c)) = self.db.card(id) {
                        c.due = Some(d);
                        let res = self.db.update_card(&c);
                        self.check(res);
                    }
                }
                None => note = format!(" (could not read the due date \"{text}\")"),
            }
        }
        self.reload_board();
        self.toast(format!("Added \"{title}\"{note}"), !note.is_empty());
    }

    // ---- update ----

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                let now = now();
                self.now = now;
                if now.date() != self.today {
                    self.today = now.date();
                    self.notified.clear();
                    self.reload_all();
                }
                let fx = self.timer.tick(now);
                self.apply(fx);
                self.minute_warning();
                self.poll_theme();
                if self.last_data_check.elapsed().as_millis() >= 900 {
                    self.last_data_check = Instant::now();
                    self.sync_outside();
                }
                self.reminders();
                if self
                    .toast
                    .as_ref()
                    .is_some_and(|(_, err, at)| at.elapsed().as_secs() >= if *err { 8 } else { 3 })
                {
                    self.toast = None;
                }
            }
            Message::External(cmd) => return self.external(cmd),
            Message::WindowOpened(id) => return window::size(id).map(Message::Resized),
            Message::Resized(size) => self.size = size,
            Message::WindowClosed(id) => {
                if self.window == Some(id) {
                    self.window = None;
                    self.zen = false;
                    self.drag = None;
                    // Without a tray there would be no way back to the window.
                    if self.tray.is_none() {
                        return self.quit();
                    }
                }
            }
            Message::Key(key, mods, captured) => return self.key(key, mods, captured),
            Message::Cursor(pos) => {
                if let Some(d) = self.drag.as_mut() {
                    let start = *d.start.get_or_insert(pos);
                    d.pos = pos;
                    if (pos.x - start.x).hypot(pos.y - start.y) > 6.0 {
                        d.active = true;
                    }
                }
            }
            Message::MouseUp => {
                if let Some(d) = self.drag.take() {
                    if d.active {
                        self.drop_card(&d);
                    } else {
                        return self.update(Message::CardOpen(d.card));
                    }
                }
            }
            Message::Go(view) => {
                self.view = view;
                self.modal = Modal::None;
                if view == View::Analytics {
                    self.reload_analytics();
                }
            }
            Message::Field(f, v) => self.set_field(f, v),
            Message::Submit(f) => return self.submit(f),
            Message::Toggle(t, on) => self.toggle(t, on),
            Message::CloseModal => self.modal = Modal::None,
            Message::PromptOpen(action) => {
                let (title, value) = match &action {
                    PromptAction::NewBoard => ("New board", String::new()),
                    PromptAction::NewList => ("New list", String::new()),
                    PromptAction::RenameBoard(id) => (
                        "Rename board",
                        self.boards
                            .iter()
                            .find(|b| b.id == *id)
                            .map(|b| b.name.clone())
                            .unwrap_or_default(),
                    ),
                    PromptAction::RenameList(id) => (
                        "Rename list",
                        self.lists
                            .iter()
                            .find(|l| l.id == *id)
                            .map(|l| l.name.clone())
                            .unwrap_or_default(),
                    ),
                };
                self.modal = Modal::Prompt {
                    title,
                    value,
                    action,
                };
                return iced::widget::operation::focus("prompt");
            }
            Message::Ask(action) => {
                let text = match &action {
                    ConfirmAction::DeleteBoard(_) => {
                        "Delete this board and all its cards? Logged sessions are kept.".to_string()
                    }
                    ConfirmAction::DeleteList(id) => {
                        let n = self.cards.iter().filter(|c| c.list_id == *id).count();
                        format!("Delete this list and its {n} card(s)?")
                    }
                    ConfirmAction::DeleteHabit(_) => "Delete this habit and its whole history?".to_string(),
                    ConfirmAction::DeleteCard(_) => "Delete this card? Logged sessions are kept.".to_string(),
                    ConfirmAction::DeleteCalendar(id) => {
                        let n = self.items.iter().filter(|i| i.calendar_id == *id).count();
                        format!("Delete this calendar and its {n} item(s)?")
                    }
                    ConfirmAction::ImportData => {
                        "Importing replaces everything currently in Pomolog. Continue?".to_string()
                    }
                    ConfirmAction::WipeData => {
                        "Delete all boards, cards, sessions, calendar items and habits? This cannot be undone."
                            .to_string()
                    }
                };
                self.modal = Modal::Confirm { text, action };
            }
            Message::ConfirmYes => {
                if let Modal::Confirm { action, .. } =
                    std::mem::replace(&mut self.modal, Modal::None)
                {
                    self.confirmed(action);
                }
            }
            Message::Run(action) => {
                self.modal = Modal::None;
                return self.run(action);
            }
            Message::Noop => {}

            Message::TimerToggle => {
                let fx = self.timer.toggle(now());
                self.apply(fx);
            }
            Message::TimerSkip => {
                let fx = self.timer.skip(now());
                self.apply(fx);
            }
            Message::TimerAbandon => {
                let reason = std::mem::take(&mut self.reason);
                let fx = self.timer.abandon(now(), &reason);
                self.apply(fx);
            }
            Message::TimerFinish => {
                let fx = self.timer.finish(now());
                self.apply(fx);
            }
            Message::TimerExtend(secs) => {
                self.timer.extend(secs);
                self.publish_status();
            }
            Message::TimerCard(choice) => {
                let fx = self.timer.set_card(now(), choice.id);
                self.apply(fx);
            }
            Message::StartCard(id) => {
                let now = now();
                let mut fx = self.timer.set_card(now, Some(id));
                // Starting a task during a break means: get to work.
                if self.timer.phase().is_break() {
                    fx.extend(self.timer.skip(now));
                    fx.extend(self.timer.set_card(now, Some(id)));
                }
                if self.timer.active.is_none()
                    || self.timer.active.as_ref().is_some_and(|a| a.paused)
                {
                    fx.extend(self.timer.start(now));
                }
                self.apply(fx);
                self.view = View::Timer;
                self.modal = Modal::None;
            }
            Message::Zen(on) => {
                self.zen = on;
                if let Some(id) = self.window {
                    return window::set_mode(
                        id,
                        if on {
                            window::Mode::Fullscreen
                        } else {
                            window::Mode::Windowed
                        },
                    );
                }
            }
            Message::SessionDelete(id) => {
                let res = self.db.delete_session(id);
                self.check(res);
                self.reload_board();
                self.reload_habits();
                self.reload_sessions();
            }
            Message::ManualAdd => self.manual_add(),

            m @ (Message::BoardPick(_)
            | Message::CardAddTo(_)
            | Message::CardPress(_)
            | Message::CardHover(..)
            | Message::ListHover(..)
            | Message::CardOpen(_)
            | Message::CardNotes(_)
            | Message::CardList(_)
            | Message::CardSave) => return self.update_board(m),

            m @ (Message::CalShift(_)
            | Message::CalToday
            | Message::CalSelect(_)
            | Message::ItemNew(..)
            | Message::ItemOpen(_)
            | Message::ItemKindSet(_)
            | Message::ItemRecur(_)
            | Message::ItemSave
            | Message::ItemDelete(_)
            | Message::ItemDone(..)
            | Message::IcsKind(_)
            | Message::IcsImport
            | Message::IcsCalendar(_)
            | Message::ItemCalendar(_)
            | Message::ItemNotes(_)
            | Message::CalendarNew
            | Message::CalendarOpen(_)
            | Message::CalendarColor(_)
            | Message::CalendarSave
            | Message::CalendarVisible(..)) => return self.update_calendar(m),

            m @ (Message::HabitNew
            | Message::HabitOpen(_)
            | Message::HabitKindSet(_)
            | Message::HabitSched(_)
            | Message::HabitPerWeek(_)
            | Message::HabitBoard(_)
            | Message::HabitSave
            | Message::HabitCheck(..)
            | Message::HabitSkip(..)) => return self.update_habits(m),

            Message::AnaRange(r) => {
                self.ana.range = r;
                self.reload_analytics();
            }

            m @ (Message::SetTheme(_)
            | Message::SetMode(_)
            | Message::SetSource(_)
            | Message::SetClock(_)
            | Message::SetRadius(_)
            | Message::SetScale(_)
            | Message::SetVolume(_)
            | Message::TestSound
            | Message::SetEveryN(_)
            | Message::OverrideClear(_)
            | Message::ThemeSaveAs
            | Message::Export
            | Message::Import) => self.update_settings(m),
        }
        Task::none()
    }

    fn manual_add(&mut self) {
        let Ok(mins) = self.manual_min.trim().parse::<f64>() else {
            self.toast("Enter the minutes worked as a number.", true);
            return;
        };
        if !(mins > 0.0 && mins <= 24.0 * 60.0) {
            self.toast("Minutes must be between 0 and 1440.", true);
            return;
        }
        let secs = (mins * 60.0).round() as i64;
        let end = now();
        let res = self.db.insert_session(&SessionRecord {
            kind: Phase::Focus,
            card_id: self.timer.card_id,
            started_at: end - Duration::seconds(secs),
            ended_at: end,
            duration_s: secs,
            planned_s: secs,
            completed: true,
            interruptions: Vec::new(),
        });
        if self.check(res).is_some() {
            self.manual_min.clear();
            self.reload_board();
            self.reload_habits();
            self.reload_sessions();
        }
    }

    fn key(
        &mut self,
        key: keyboard::Key,
        mods: keyboard::Modifiers,
        captured: bool,
    ) -> Task<Message> {
        use keyboard::Key;
        use keyboard::key::Named;
        // Single keys act only when no text field or dialog is in the way.
        let free = !captured && matches!(self.modal, Modal::None);
        let chord = std::mem::take(&mut self.pending_g);
        if free
            && self.cfg.keys.vim
            && !mods.command()
            && !mods.alt()
            && let Some(task) = self.vim_key(&key, mods, chord)
        {
            return task;
        }
        match key.as_ref() {
            Key::Named(Named::Escape) => {
                if !matches!(self.modal, Modal::None) {
                    self.modal = Modal::None;
                } else if self.adding_to.is_some() {
                    self.adding_to = None;
                } else if self.zen {
                    return self.update(Message::Zen(false));
                }
            }
            Key::Character("k") if mods.command() => {
                self.modal = Modal::Palette {
                    query: String::new(),
                };
                return iced::widget::operation::focus("palette");
            }
            Key::Character(c) if mods.command() => {
                if let Some(v) = c
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| View::ALL.get(n.wrapping_sub(1)))
                {
                    return self.update(Message::Go(*v));
                }
            }
            // Only when no text field took the key.
            Key::Named(Named::Space) if !captured && matches!(self.modal, Modal::None) => {
                return self.update(Message::TimerToggle);
            }
            _ => {}
        }
        Task::none()
    }

    fn run(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Go(v) => self.update(Message::Go(v)),
            Action::TimerToggle => self.update(Message::TimerToggle),
            Action::TimerSkip => self.update(Message::TimerSkip),
            Action::Zen => {
                self.view = View::Timer;
                self.update(Message::Zen(!self.zen))
            }
            Action::StartCard(id) => self.update(Message::StartCard(id)),
            Action::Intake(text) => {
                self.intake(&text);
                Task::none()
            }
            Action::Theme(name) => self.update(Message::SetTheme(name)),
        }
    }

    /// What the command palette offers for a query, best match first.
    pub fn palette_actions(&self, query: &str) -> Vec<(String, Action)> {
        let q = query.trim().to_lowercase();
        let mut all: Vec<(String, Action)> = Vec::new();
        let toggle = match &self.timer.active {
            Some(a) if !a.paused => "Pause timer",
            Some(_) => "Resume timer",
            None => "Start timer",
        };
        all.push((toggle.into(), Action::TimerToggle));
        all.push(("Skip to next phase".into(), Action::TimerSkip));
        all.push(("Toggle zen mode".into(), Action::Zen));
        all.extend(
            View::ALL
                .iter()
                .map(|v| (format!("Go to {}", v.label()), Action::Go(*v))),
        );
        all.extend(self.in_progress.iter().map(|(c, board)| {
            (
                format!("Focus on: {} ({board})", c.title),
                Action::StartCard(c.id),
            )
        }));
        all.extend(
            self.set
                .themes
                .iter()
                .map(|t| (format!("Theme: {t}"), Action::Theme(t.clone()))),
        );
        let mut hits: Vec<(String, Action)> = all
            .into_iter()
            .filter(|(label, _)| q.is_empty() || label.to_lowercase().contains(&q))
            .collect();
        // Anything typed can also be added: the intake says as what.
        if let Ok(what) = intake::parse(query, self.today) {
            hits.push((
                format!("Add {}", what.describe()),
                Action::Intake(query.trim().to_string()),
            ));
        }
        hits.truncate(9);
        hits
    }

    fn set_field(&mut self, f: Field, v: String) {
        match f {
            Field::Reason => {
                self.timer.set_last_reason(&v);
                self.reason = v;
            }
            Field::ManualMin => self.manual_min = v,
            Field::BoardSearch => self.search = v,
            Field::NewCard => self.new_card = v,
            Field::Prompt => {
                if let Modal::Prompt { value, .. } = &mut self.modal {
                    *value = v;
                }
            }
            Field::PaletteQuery => {
                if let Modal::Palette { query } = &mut self.modal {
                    *query = v;
                }
            }
            Field::CardTitle | Field::CardEstimate | Field::CardDue | Field::CardTags => {
                if let Modal::Card(c) = &mut self.modal {
                    match f {
                        Field::CardTitle => c.title = v,
                        Field::CardEstimate => c.estimate = v,
                        Field::CardDue => c.due = v,
                        _ => c.tags = v,
                    }
                }
            }
            Field::ItemTitle
            | Field::ItemDate
            | Field::ItemTime
            | Field::ItemDuration
            | Field::ItemUntil
            | Field::ItemTz => {
                if let Modal::Item(i) = &mut self.modal {
                    match f {
                        Field::ItemTitle => i.title = v,
                        Field::ItemDate => i.date = v,
                        Field::ItemTime => i.time = v,
                        Field::ItemDuration => i.duration = v,
                        Field::ItemUntil => i.until = v,
                        _ => i.tz = v,
                    }
                }
            }
            Field::CalendarName | Field::CalendarTz => {
                if let Modal::Calendar(c) = &mut self.modal {
                    match f {
                        Field::CalendarName => c.name = v,
                        _ => c.tz = v,
                    }
                }
            }
            Field::HabitName
            | Field::HabitTarget
            | Field::HabitUnit
            | Field::HabitTag
            | Field::HabitSkips => {
                if let Modal::Habit(h) = &mut self.modal {
                    match f {
                        Field::HabitName => h.name = v,
                        Field::HabitTarget => h.target = v,
                        Field::HabitUnit => h.unit = v,
                        Field::HabitTag => h.tag = v,
                        _ => h.skips = v,
                    }
                }
            }
            Field::HabitQty(id) => {
                self.habit_qty.insert(id, v);
            }
            Field::IcsPath => self.ics_path = v,
            Field::ExportPath => self.set.export_path = v,
            Field::ImportPath => self.set.import_path = v,
            Field::SetThemeName => self.set.theme_name = v,
            Field::SetFocus
            | Field::SetShort
            | Field::SetLong
            | Field::SetGoal
            | Field::SetRatio
            | Field::SetFont
            | Field::SetLead
            | Field::SetHabitHour
            | Field::Override(_) => self.settings_field(f, v),
        }
    }

    fn submit(&mut self, f: Field) -> Task<Message> {
        match f {
            Field::NewCard => {
                let title = std::mem::take(&mut self.new_card);
                if let (Some(list), false) = (self.adding_to, title.trim().is_empty()) {
                    let res = self.db.add_card(list, title.trim(), now());
                    self.check(res);
                    self.reload_board();
                    // Stay in the field so several cards can be typed in a row.
                    return iced::widget::operation::focus("new-card");
                }
                self.adding_to = None;
            }
            Field::Prompt => {
                if let Modal::Prompt { value, action, .. } =
                    std::mem::replace(&mut self.modal, Modal::None)
                {
                    self.prompt_done(action, value.trim());
                }
            }
            Field::PaletteQuery => {
                if let Modal::Palette { query } = &self.modal
                    && let Some((_, action)) = self.palette_actions(query).into_iter().next()
                {
                    return self.update(Message::Run(action));
                }
            }
            Field::CardTitle | Field::CardEstimate | Field::CardDue | Field::CardTags => {
                return self.update(Message::CardSave);
            }
            Field::ItemTitle
            | Field::ItemDate
            | Field::ItemTime
            | Field::ItemDuration
            | Field::ItemUntil
            | Field::ItemTz => {
                return self.update(Message::ItemSave);
            }
            Field::CalendarName | Field::CalendarTz => {
                return self.update(Message::CalendarSave);
            }
            Field::HabitName | Field::HabitTarget | Field::HabitUnit | Field::HabitTag => {
                return self.update(Message::HabitSave);
            }
            Field::HabitQty(id) => self.habit_qty_submit(id),
            Field::ManualMin => self.manual_add(),
            Field::IcsPath => return self.update(Message::IcsImport),
            _ => {}
        }
        Task::none()
    }

    fn prompt_done(&mut self, action: PromptAction, name: &str) {
        if name.is_empty() {
            return;
        }
        let res = match action {
            PromptAction::NewBoard => self.db.add_board(name).map(|id| {
                self.board_id = Some(id);
                let _ = self.db.set_meta("board", &id.to_string());
            }),
            PromptAction::RenameBoard(id) => self.db.rename_board(id, name),
            PromptAction::NewList => match self.board_id {
                Some(b) => self.db.add_list(b, name).map(|_| ()),
                None => Ok(()),
            },
            PromptAction::RenameList(id) => self.db.rename_list(id, name),
        };
        self.check(res);
        self.reload_board();
    }

    fn confirmed(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::DeleteBoard(id) => {
                let res = self.db.delete_board(id);
                self.check(res);
                if self.db.boards().map(|b| b.is_empty()).unwrap_or(false) {
                    let _ = self.db.add_board("Tasks");
                }
                self.reload_all();
            }
            ConfirmAction::DeleteList(id) => {
                let res = self.db.delete_list(id);
                self.check(res);
                self.reload_board();
            }
            ConfirmAction::DeleteCard(id) => {
                let res = self.db.delete_card(id);
                self.check(res);
                self.reload_board();
            }
            ConfirmAction::DeleteHabit(id) => {
                let res = self.db.delete_habit(id);
                self.check(res);
                self.reload_habits();
            }
            ConfirmAction::DeleteCalendar(id) => {
                let res = self.db.delete_calendar(id);
                self.check(res);
                self.reload_calendar();
            }
            ConfirmAction::ImportData => self.import_data(),
            ConfirmAction::WipeData => {
                let res = crate::core::export::wipe(&mut self.db);
                if self.check(res).is_some() {
                    let _ = self.db.add_board("Tasks");
                    self.timer = Timer::new(self.cfg.timer.to_cfg());
                    self.reload_all();
                    self.toast("All data deleted.", false);
                }
            }
        }
    }

    pub fn expand_path(path: &str) -> std::path::PathBuf {
        expand(path)
    }
}

fn key_events(event: iced::Event, status: iced::event::Status, _: window::Id) -> Option<Message> {
    match event {
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => Some(
            Message::Key(key, modifiers, status == iced::event::Status::Captured),
        ),
        _ => None,
    }
}

fn drag_events(event: iced::Event, _: iced::event::Status, _: window::Id) -> Option<Message> {
    use iced::mouse;
    match event {
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::Cursor(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::MouseUp)
        }
        _ => None,
    }
}

fn external_commands() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::SinkExt;
    iced::stream::channel(32, async |mut out| {
        let Some(mut rx) = bridge::take_receiver() else {
            return std::future::pending().await;
        };
        while let Some(cmd) = rx.recv().await {
            let _ = out.send(Message::External(cmd)).await;
        }
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
