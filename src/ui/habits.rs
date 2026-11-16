//! Don't-break-the-chain habits.

use super::app::{
    App, Choice, ConfirmAction, El, Field, HabitEdit, Message, Modal, SchedKind, Toggle, View,
};
use super::calendar::habit_color;
use super::shell::{action, empty, field, icon_btn, labelled, page, vscroll};
use super::style::{self, Btn, P, alpha, btn, chip, icon};
use super::timer::trim_num;
use super::widgets::{Cell, Chain, WeekGrid};
use crate::core::dates;
use crate::core::habits::{self, DayState};
use crate::core::model::*;
use chrono::{Duration, NaiveDate};
use iced::widget::{
    Space, button, canvas, checkbox, column, container, pick_list, row, space, text,
};
use iced::{Alignment, Color, Fill, Length, Task};

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn describe(h: &Habit, board: Option<&str>) -> String {
    let when = match &h.schedule {
        Schedule::Daily => "Every day".to_string(),
        Schedule::Weekdays(d) => {
            let mut d = d.clone();
            d.sort();
            d.iter()
                .filter_map(|i| DAYS.get(*i as usize))
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        }
        Schedule::PerWeek(n) => format!("{n} times a week"),
    };
    let what = match h.kind {
        HabitKind::Check => String::new(),
        HabitKind::Quantity => format!("  ·  {} {}", trim_num(h.target), h.unit),
        HabitKind::Focus => {
            let on = match (&h.link_tag, board) {
                (Some(t), Some(b)) => format!(" on #{t} in {b}"),
                (Some(t), None) => format!(" on #{t}"),
                (None, Some(b)) => format!(" in {b}"),
                (None, None) => String::new(),
            };
            format!("  ·  {} min of focus{on}", trim_num(h.target))
        }
    };
    format!("{when}{what}")
}

impl App {
    /// The date a habit control on screen refers to.
    fn habit_date(&self) -> NaiveDate {
        if self.view == View::Calendar {
            self.cal_selected
        } else {
            self.today
        }
    }

    pub(super) fn update_habits(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::HabitNew => {
                self.modal = Modal::Habit(HabitEdit {
                    id: 0,
                    name: String::new(),
                    kind: HabitKind::Check,
                    target: String::new(),
                    unit: String::new(),
                    sched: SchedKind::Daily,
                    weekdays: [true, true, true, true, true, false, false],
                    per_week: 3,
                    tag: String::new(),
                    board: None,
                    rest: false,
                    skips: "2".into(),
                    archived: false,
                    created: self.today,
                });
                return iced::widget::operation::focus("habit-name");
            }
            Message::HabitOpen(id) => {
                if let Some(h) = self.habits.iter().find(|h| h.id == id) {
                    let mut weekdays = [false; 7];
                    let (sched, per_week) = match &h.schedule {
                        Schedule::Daily => (SchedKind::Daily, 3),
                        Schedule::Weekdays(d) => {
                            for i in d {
                                if let Some(w) = weekdays.get_mut(*i as usize) {
                                    *w = true;
                                }
                            }
                            (SchedKind::Weekdays, 3)
                        }
                        Schedule::PerWeek(n) => (SchedKind::PerWeek, *n),
                    };
                    if sched != SchedKind::Weekdays {
                        weekdays = [true, true, true, true, true, false, false];
                    }
                    self.modal = Modal::Habit(HabitEdit {
                        id,
                        name: h.name.clone(),
                        kind: h.kind,
                        target: if h.kind == HabitKind::Check {
                            String::new()
                        } else {
                            trim_num(h.target)
                        },
                        unit: h.unit.clone(),
                        sched,
                        weekdays,
                        per_week,
                        tag: h.link_tag.clone().unwrap_or_default(),
                        board: h.link_board,
                        rest: h.rest_on_holidays,
                        skips: h.skips_per_month.to_string(),
                        archived: h.archived,
                        created: h.created,
                    });
                }
            }
            Message::HabitKindSet(k) => {
                if let Modal::Habit(h) = &mut self.modal {
                    h.kind = k;
                }
            }
            Message::HabitSched(s) => {
                if let Modal::Habit(h) = &mut self.modal {
                    h.sched = s;
                }
            }
            Message::HabitPerWeek(n) => {
                if let Modal::Habit(h) = &mut self.modal {
                    h.per_week = n;
                }
            }
            Message::HabitBoard(c) => {
                if let Modal::Habit(h) = &mut self.modal {
                    h.board = c.id;
                }
            }
            Message::HabitSave => self.habit_save(),
            Message::HabitCheck(id, date, on) => {
                let Some(h) = self.habits.iter().find(|h| h.id == id) else {
                    return Task::none();
                };
                let value = if on { h.target.max(1.0) } else { 0.0 };
                let res = self.db.set_habit_entry(
                    id,
                    date,
                    HabitEntry {
                        value,
                        skipped: false,
                    },
                );
                self.check(res);
                self.habit_qty.remove(&id);
                self.reload_habits();
            }
            Message::HabitSkip(id, date, on) => {
                let Some(h) = self.habits.iter().find(|h| h.id == id) else {
                    return Task::none();
                };
                let empty = Default::default();
                let stored = self.stored_entries_of(id).unwrap_or(&empty);
                if on && habits::skips_used(stored, date) >= h.skips_per_month {
                    let n = h.skips_per_month;
                    self.toast(format!("All {n} skip(s) for this month are used."), true);
                    return Task::none();
                }
                let mut e = self.stored_entry(id, date);
                e.skipped = on;
                let res = self.db.set_habit_entry(id, date, e);
                self.check(res);
                self.reload_habits();
            }
            _ => {}
        }
        Task::none()
    }

    pub(super) fn habit_qty_submit(&mut self, id: Id) {
        let date = self.habit_date();
        let Some(text) = self.habit_qty.get(&id) else {
            return;
        };
        match text.trim().replace(',', ".").parse::<f64>() {
            Ok(v) if (0.0..=1e9).contains(&v) => {
                let mut e = self.stored_entry(id, date);
                e.value = v;
                let res = self.db.set_habit_entry(id, date, e);
                self.check(res);
                self.habit_qty.remove(&id);
                self.reload_habits();
            }
            _ => self.toast("Enter the amount as a number.", true),
        }
    }

    fn habit_save(&mut self) {
        let Modal::Habit(e) = &self.modal else { return };
        let fail = |app: &mut App, msg: &str| app.toast(msg.to_string(), true);
        let name = e.name.trim().to_string();
        if name.is_empty() {
            return fail(self, "Give the habit a name.");
        }
        let target = match e.kind {
            HabitKind::Check => 1.0,
            _ => match e.target.trim().replace(',', ".").parse::<f64>() {
                Ok(v) if v > 0.0 && v <= 1e9 => v,
                _ => return fail(self, "The target must be a number above zero."),
            },
        };
        let schedule = match e.sched {
            SchedKind::Daily => Schedule::Daily,
            SchedKind::PerWeek => Schedule::PerWeek(e.per_week.clamp(1, 7)),
            SchedKind::Weekdays => {
                let days: Vec<u8> = (0..7u8).filter(|i| e.weekdays[*i as usize]).collect();
                if days.is_empty() {
                    return fail(self, "Pick at least one weekday.");
                }
                // All seven days is just "every day".
                if days.len() == 7 {
                    Schedule::Daily
                } else {
                    Schedule::Weekdays(days)
                }
            }
        };
        let Ok(skips) = e.skips.trim().parse::<u32>() else {
            return fail(self, "Skips per month is a whole number.");
        };
        let focus = e.kind == HabitKind::Focus;
        let habit = Habit {
            id: e.id,
            name,
            kind: e.kind,
            target,
            unit: if focus {
                "min".into()
            } else {
                e.unit.trim().to_string()
            },
            schedule,
            link_tag: Some(e.tag.trim().trim_start_matches('#').to_string())
                .filter(|t| focus && !t.is_empty()),
            link_board: e.board.filter(|_| focus),
            rest_on_holidays: e.rest,
            skips_per_month: skips.min(31),
            created: e.created,
            archived: e.archived,
        };
        let res = self.db.save_habit(&habit);
        if self.check(res).is_some() {
            self.modal = Modal::None;
            self.reload_habits();
        }
    }

    fn habit_index(&self, id: Id) -> usize {
        self.habits
            .iter()
            .filter(|h| !h.archived)
            .position(|h| h.id == id)
            .unwrap_or(0)
    }

    /// The control for marking a habit on `date`.
    /// `named` shows the habit's name and streak; inside the habit's own
    /// card those are already on screen.
    pub(super) fn habit_today_row<'a>(
        &'a self,
        p: P,
        h: &'a Habit,
        date: NaiveDate,
        named: bool,
    ) -> El<'a> {
        let cx = self.habit_ctx(h.id);
        let state = habits::day_state(h, date, &cx);
        let entry = cx.entries.get(&date).copied().unwrap_or_default();
        let done = state == DayState::Done;
        let tint = habit_color(p, self.habit_index(h.id));
        let streak = self.streaks.get(&h.id).copied().unwrap_or_default();

        let mut line = row![
            container(Space::new().width(4).height(22)).style(style::panel(p, tint)),
            text(if named { h.name.as_str() } else { "Today" })
                .size(13)
                .width(Fill)
                .color(if done { p.dim } else { p.text }),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        // The calendar's day panel is too narrow to fit the streak as well.
        if named && streak.current > 0 && self.view != View::Calendar {
            line = line.push(
                row![
                    icon("flame", 13.0, p.warning),
                    text(streak.current.to_string()).size(12).color(p.dim)
                ]
                .spacing(3)
                .align_y(Alignment::Center),
            );
        }
        match h.kind {
            HabitKind::Check => {}
            HabitKind::Quantity => {
                let shown = self.habit_qty.get(&h.id).cloned().unwrap_or_else(|| {
                    if entry.value > 0.0 {
                        trim_num(entry.value)
                    } else {
                        String::new()
                    }
                });
                line = line
                    .push(field(p, "0", &shown, Field::HabitQty(h.id)).width(64))
                    .push(
                        text(format!("/ {} {}", trim_num(h.target), h.unit))
                            .size(12)
                            .color(p.dim),
                    );
            }
            HabitKind::Focus => {
                line = line.push(
                    text(format!("{:.0} / {} min", entry.value, trim_num(h.target)))
                        .size(12)
                        .color(p.dim),
                );
            }
        }
        if state == DayState::Skipped {
            line = line.push(chip(p, "skipped", p.dim));
        }
        if !done && date <= self.today {
            let skipped = state == DayState::Skipped;
            line = line.push(
                button(icon("moon", 14.0, if skipped { tint } else { p.dim }))
                    .padding(6)
                    .style(btn(p, if skipped { Btn::Selected } else { Btn::Ghost }))
                    .on_press(Message::HabitSkip(h.id, date, !skipped)),
            );
        }
        line = line.push(
            // An empty box until done, so it cannot be mistaken for ticked.
            button(if done {
                icon("check", 15.0, p.bg)
            } else {
                Space::new().width(15).height(15).into()
            })
            .padding(6)
            .style(btn(p, if done { Btn::Fill(tint) } else { Btn::Outline }))
            .on_press(Message::HabitCheck(h.id, date, !done)),
        );
        line.into()
    }

    fn habit_cells(
        &self,
        p: P,
        h: &Habit,
        tint: Color,
        from: NaiveDate,
        days: i64,
    ) -> (Vec<Cell>, Vec<bool>) {
        let cx = self.habit_ctx(h.id);
        // Whether the chain was alive going into the window.
        let mut alive = false;
        let mut d = h.created;
        while d < from {
            match habits::day_state(h, d, &cx) {
                DayState::Done => alive = true,
                DayState::Missed => alive = false,
                _ => {}
            }
            d = d.succ_opt().unwrap();
        }
        let mut cells = Vec::new();
        let mut joined = Vec::new();
        for i in 0..days {
            let d = from + Duration::days(i);
            let state = habits::day_state(h, d, &cx);
            match state {
                DayState::Done => alive = true,
                DayState::Missed => alive = false,
                _ => {}
            }
            joined.push(alive && state != DayState::Outside && state != DayState::Pending);
            cells.push(match state {
                DayState::Done => Cell::Fill(tint),
                DayState::Skipped => Cell::Ring(alpha(tint, 0.6)),
                DayState::Missed => Cell::Dot(alpha(p.danger, 0.8)),
                DayState::Pending => Cell::Ring(tint),
                DayState::Rest => Cell::Empty,
                DayState::Outside if d > self.today => Cell::None,
                DayState::Outside => Cell::Empty,
            });
        }
        (cells, joined)
    }

    fn habit_card<'a>(&'a self, p: P, h: &'a Habit) -> El<'a> {
        let tint = if h.archived {
            p.dim
        } else {
            habit_color(p, self.habit_index(h.id))
        };
        let streak = self.streaks.get(&h.id).copied().unwrap_or_default();
        let cx = self.habit_ctx(h.id);
        let rate = habits::completion_rate(h, self.today - Duration::days(29), self.today, &cx);
        let board = h
            .link_board
            .and_then(|id| self.boards.iter().find(|b| b.id == id))
            .map(|b| b.name.as_str());

        let mut title = row![text(&h.name).size(16)]
            .spacing(8)
            .align_y(Alignment::Center);
        if h.archived {
            title = title.push(chip(p, "archived", p.dim));
        }
        if streak.at_risk && streak.current > 0 && !h.archived {
            title = title.push(chip(
                p,
                if streak.weeks {
                    "this week still open"
                } else {
                    "today still open"
                },
                p.warning,
            ));
        }
        title = title.push(space::horizontal()).push(icon_btn(
            p,
            Btn::Ghost,
            "edit",
            Message::HabitOpen(h.id),
        ));

        let stats = row![
            row![
                icon(
                    "flame",
                    20.0,
                    if streak.current > 0 { p.warning } else { p.dim }
                ),
                text(streak.current.to_string()).size(26),
                text(streak.unit(streak.current)).size(12).color(p.dim),
            ]
            .spacing(5)
            .align_y(Alignment::End),
            column![
                text("BEST").size(10).color(p.dim),
                text(format!(
                    "{} {}",
                    streak.longest,
                    streak.unit(streak.longest)
                ))
                .size(13)
            ],
            column![
                text("LAST 30 DAYS").size(10).color(p.dim),
                text(rate.map_or("–".to_string(), |r| format!("{:.0}%", r * 100.0))).size(13)
            ],
        ]
        .spacing(p.u * 3.0)
        .align_y(Alignment::End);

        let recent_from = self.today - Duration::days(20);
        let (cells, joined) = self.habit_cells(p, h, tint, recent_from, 21);
        let chain = Chain {
            cells,
            joined,
            color: alpha(tint, 0.55),
            empty: p.surface3,
            cell: 16.0,
        };
        let chain_size = chain.size();

        let grid_from = dates::week_start(self.today) - Duration::weeks(25);
        let (cells, _) = self.habit_cells(p, h, tint, grid_from, 26 * 7);
        let grid = WeekGrid {
            cells,
            empty: p.surface2,
            cell: 11.0,
            gap: 3.0,
        };
        let grid_size = grid.size();

        let mut left = column![
            title,
            text(describe(h, board)).size(12).color(p.dim),
            Space::new().height(4),
            stats,
        ]
        .spacing(4)
        .width(Fill);
        if !h.archived {
            left = left
                .push(Space::new().height(6))
                .push(self.habit_today_row(p, h, self.today, false));
        }

        let right = column![
            text("LAST 3 WEEKS").size(10).color(p.dim),
            canvas(chain)
                .width(Length::Fixed(chain_size.width))
                .height(Length::Fixed(chain_size.height)),
            Space::new().height(4),
            text("LAST 6 MONTHS").size(10).color(p.dim),
            canvas(grid)
                .width(Length::Fixed(grid_size.width))
                .height(Length::Fixed(grid_size.height)),
        ]
        .spacing(5);

        let body: El = if self.wide() {
            row![left, right].spacing(p.u * 3.0).into()
        } else {
            column![left, right].spacing(p.u * 2.0).into()
        };
        container(body)
            .padding(p.u * 2.0)
            .width(Fill)
            .style(style::card(p))
            .into()
    }

    pub fn view_habits(&self, p: P) -> El<'_> {
        let mut col = column![].spacing(p.u * 1.5);
        if self.habits.is_empty() {
            col = col.push(empty(
                p,
                "A habit is a chain: do it on each scheduled day and the chain grows. Add your first one.",
            ));
        }
        for h in &self.habits {
            col = col.push(self.habit_card(p, h));
        }
        col = col.push(
            row![
                icon("moon", 13.0, p.dim),
                style::dim(
                    p,
                    "skips a day without breaking the chain; each habit gets a few a month."
                )
                .size(12)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
        page(
            p,
            true,
            "Habits",
            Some(action(
                p,
                Btn::Primary,
                Some("plus"),
                "Habit",
                Message::HabitNew,
            )),
            vscroll(p, col),
        )
    }

    pub(super) fn habit_editor<'a>(&'a self, p: P, e: &'a HabitEdit) -> El<'a> {
        let mut col = column![
            field(p, "Name, e.g. Read", &e.name, Field::HabitName)
                .id("habit-name")
                .size(15),
            labelled(
                p,
                "TYPE",
                pick_list(&HabitKind::ALL[..], Some(e.kind), Message::HabitKindSet)
                    .width(Fill)
                    .text_size(13)
            ),
        ]
        .spacing(p.u * 1.5);

        match e.kind {
            HabitKind::Check => {}
            HabitKind::Quantity => {
                col = col.push(
                    row![
                        labelled(
                            p,
                            "TARGET PER DAY",
                            field(p, "20", &e.target, Field::HabitTarget)
                        ),
                        labelled(p, "UNIT", field(p, "pages", &e.unit, Field::HabitUnit)),
                    ]
                    .spacing(p.u * 1.5),
                );
            }
            HabitKind::Focus => {
                let mut boards = vec![Choice {
                    id: None,
                    label: "Any board".into(),
                }];
                boards.extend(self.boards.iter().map(|b| Choice {
                    id: Some(b.id),
                    label: b.name.clone(),
                }));
                let selected = boards.iter().find(|c| c.id == e.board).cloned();
                col = col
                    .push(
                        row![
                            labelled(
                                p,
                                "FOCUS MINUTES PER DAY",
                                field(p, "50", &e.target, Field::HabitTarget)
                            ),
                            labelled(
                                p,
                                "ONLY ON TAG",
                                field(p, "writing", &e.tag, Field::HabitTag)
                            ),
                            labelled(
                                p,
                                "ONLY ON BOARD",
                                pick_list(boards, selected, Message::HabitBoard)
                                    .width(150)
                                    .text_size(13)
                            ),
                        ]
                        .spacing(p.u * 1.5),
                    )
                    .push(
                        style::dim(p, "Ticks itself off once enough focus time is logged.")
                            .size(11),
                    );
            }
        }

        col = col.push(labelled(
            p,
            "SCHEDULE",
            pick_list(&SchedKind::ALL[..], Some(e.sched), Message::HabitSched)
                .width(Fill)
                .text_size(13),
        ));
        match e.sched {
            SchedKind::Daily => {}
            SchedKind::Weekdays => {
                let mut days = row![].spacing(4);
                for (i, name) in DAYS.iter().enumerate() {
                    let on = e.weekdays[i];
                    days = days.push(
                        button(text(*name).size(12))
                            .padding([5.0, 9.0])
                            .style(btn(p, if on { Btn::Selected } else { Btn::Ghost }))
                            .on_press(Message::Toggle(Toggle::HabitWeekday(i), !on)),
                    );
                }
                col = col.push(days);
            }
            SchedKind::PerWeek => {
                let mut counts = row![].spacing(4).align_y(Alignment::Center);
                for n in 1..=6u8 {
                    counts = counts.push(
                        button(text(n.to_string()).size(12))
                            .padding([5.0, 11.0])
                            .style(btn(
                                p,
                                if e.per_week == n {
                                    Btn::Selected
                                } else {
                                    Btn::Ghost
                                },
                            ))
                            .on_press(Message::HabitPerWeek(n)),
                    );
                }
                counts = counts.push(style::dim(p, "times a week, on any days").size(12));
                col = col.push(counts);
            }
        }

        col = col.push(
            row![
                checkbox(e.rest)
                    .label("Rest on holidays")
                    .on_toggle(|v| Message::Toggle(Toggle::HabitRest, v))
                    .text_size(13),
                space::horizontal(),
                text("Skips per month").size(12).color(p.dim),
                field(p, "2", &e.skips, Field::HabitSkips).width(54),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        let mut buttons = row![].spacing(8).align_y(Alignment::Center);
        if e.id != 0 {
            buttons = buttons
                .push(action(
                    p,
                    Btn::Danger,
                    Some("trash"),
                    "Delete",
                    Message::Ask(ConfirmAction::DeleteHabit(e.id)),
                ))
                .push(
                    checkbox(e.archived)
                        .label("Archived")
                        .on_toggle(|v| Message::Toggle(Toggle::HabitArchived, v))
                        .text_size(13),
                );
        }
        buttons = buttons
            .push(space::horizontal())
            .push(action(p, Btn::Ghost, None, "Cancel", Message::CloseModal))
            .push(action(p, Btn::Primary, None, "Save", Message::HabitSave));
        col.push(buttons).into()
    }
}
