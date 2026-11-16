//! The timer view, zen mode, and the Today overview.

use super::app::{App, Choice, El, Field, Message, View};
use super::shell::{action, dots, empty, field, icon_btn, meter, page, section, vscroll};
use super::style::{self, Btn, P, alpha, btn, chip, icon};
use super::widgets::Ring;
use crate::core::habits::{self, DayState};
use crate::core::model::*;
use crate::core::{dates, recur};
use iced::widget::{
    Space, button, canvas, center, column, container, pick_list, row, space, stack, text,
};
use iced::{Alignment, Fill, Length};

impl App {
    fn task_choices(&self) -> (Vec<Choice>, Choice) {
        let none = Choice {
            id: None,
            label: "No task".into(),
        };
        let mut all = vec![none.clone()];
        all.extend(self.in_progress.iter().map(|(c, board)| Choice {
            id: Some(c.id),
            label: format!("{}  ·  {board}", c.title),
        }));
        let current = self
            .timer
            .active
            .as_ref()
            .map_or(self.timer.card_id, |a| a.card_id);
        let selected = match current {
            None => none,
            Some(id) => all
                .iter()
                .find(|c| c.id == Some(id))
                .cloned()
                .unwrap_or_else(|| Choice {
                    id: Some(id),
                    label: self.card_title(id).unwrap_or_else(|| "Unknown task".into()),
                }),
        };
        (all, selected)
    }

    fn ring(&self, p: P, size: f32) -> El<'_> {
        let phase = self.timer.phase();
        let tint = p.phase(phase);
        let (time, progress, sub) = match &self.timer.active {
            Some(a) => (
                dates::clock(if a.open_ended {
                    a.elapsed_s()
                } else {
                    a.remaining_s()
                }),
                a.progress(),
                if a.paused {
                    "Paused".to_string()
                } else {
                    phase.label().to_string()
                },
            ),
            None => (
                dates::clock(self.timer.planned_for(phase)),
                0.0,
                format!("{} next", phase.label()),
            ),
        };
        let ring = canvas(Ring {
            progress,
            color: if self.timer.is_running() {
                tint
            } else {
                alpha(tint, 0.6)
            },
            track: p.surface2,
            thickness: size * 0.045,
        })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size));
        stack![
            ring,
            center(
                column![
                    text(time).size(size * 0.21),
                    text(sub).size(size * 0.055).color(tint)
                ]
                .align_x(Alignment::Center)
                .spacing(2)
            ),
        ]
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
    }

    fn controls(&self, p: P) -> El<'_> {
        let big = |name: &'static str, label: &'static str, msg: Message| -> El<'_> {
            button(
                row![icon(name, 17.0, p.on_accent), text(label).size(15)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            )
            .padding([10.0, 22.0])
            .style(btn(p, Btn::Primary))
            .on_press(msg)
            .into()
        };
        let mut r = row![].spacing(8).align_y(Alignment::Center);
        match &self.timer.active {
            None => {
                r = r.push(big("play", "Start", Message::TimerToggle));
                r = r.push(action(
                    p,
                    Btn::Ghost,
                    Some("skip"),
                    "Switch phase",
                    Message::TimerSkip,
                ));
            }
            Some(a) => {
                let focus = a.phase == Phase::Focus;
                r = r.push(if a.paused {
                    big("play", "Resume", Message::TimerToggle)
                } else {
                    big("pause", "Pause", Message::TimerToggle)
                });
                if a.open_ended {
                    r = r.push(action(
                        p,
                        Btn::Soft,
                        Some("check"),
                        "Finish",
                        Message::TimerFinish,
                    ));
                } else {
                    r = r.push(action(
                        p,
                        Btn::Soft,
                        Some("plus"),
                        "5 min",
                        Message::TimerExtend(300),
                    ));
                    r = r.push(action(
                        p,
                        Btn::Ghost,
                        Some("skip"),
                        "Skip",
                        Message::TimerSkip,
                    ));
                }
                if focus {
                    r = r.push(action(
                        p,
                        Btn::Danger,
                        Some("x"),
                        "Abandon",
                        Message::TimerAbandon,
                    ));
                }
            }
        }
        r.into()
    }

    pub fn view_timer(&self, p: P) -> El<'_> {
        let (choices, selected) = self.task_choices();
        let every = self.timer.cfg.long_every;
        let mut left = column![self.ring(p, 300.0), self.controls(p),]
            .spacing(p.u * 3.0)
            .align_x(Alignment::Center);

        if let Some(a) = &self.timer.active
            && a.phase == Phase::Focus
            && a.paused
        {
            left = left.push(
                field(
                    p,
                    "What interrupted you? (optional)",
                    &self.reason,
                    Field::Reason,
                )
                .width(320),
            );
        }
        if every > 0 {
            left = left.push(
                row![
                    dots(p, self.timer.cycle.min(every), every, p.focus),
                    style::dim(p, "until a long break").size(12)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        let goal = self.cfg.timer.daily_goal;
        let done = self.completed_today();
        let summary = if goal > 0 {
            meter(
                p,
                format!("{done} of {goal} sessions"),
                dates::human(self.focus_today()),
                done as f32 / goal as f32,
                p.focus,
            )
        } else {
            meter(
                p,
                format!("{done} sessions"),
                dates::human(self.focus_today()),
                0.0,
                p.focus,
            )
        };

        let mut log = column![].spacing(2);
        if self.today_sessions.is_empty() {
            log = log.push(empty(p, "Nothing logged yet today."));
        }
        for s in &self.today_sessions {
            let tint = p.phase(s.kind);
            let what = match (s.kind, &s.card_title) {
                (Phase::Focus, Some(t)) => t.clone(),
                (Phase::Focus, None) => "Focus".to_string(),
                (k, _) => k.label().to_string(),
            };
            let mut line = row![
                container(Space::new().width(4).height(16)).style(style::panel(p, tint)),
                text(s.started_at.format("%H:%M").to_string())
                    .size(12)
                    .color(p.dim),
                text(what)
                    .size(13)
                    .width(Fill)
                    .wrapping(text::Wrapping::None),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if s.interruptions > 0 {
                line = line.push(chip(
                    p,
                    format!("{} interrupted", s.interruptions),
                    p.warning,
                ));
            }
            if s.kind == Phase::Focus && !s.completed {
                line = line.push(chip(p, "partial", p.dim));
            }
            line = line
                .push(text(dates::human(s.duration_s)).size(12).color(p.dim))
                .push(icon_btn(
                    p,
                    Btn::Danger,
                    "trash",
                    Message::SessionDelete(s.id),
                ));
            log = log.push(line);
        }

        let right = column![
            section(
                p,
                "TASK",
                column![
                    pick_list(choices, Some(selected), Message::TimerCard).width(Fill).text_size(13),
                    style::dim(p, "Cards in an In progress list appear here. Switching mid-session splits the time.")
                        .size(11),
                ]
                .spacing(6)
            ),
            section(p, "TODAY", summary),
            section(
                p,
                "SESSIONS",
                column![
                    log,
                    row![
                        field(p, "Minutes", &self.manual_min, Field::ManualMin).width(90),
                        action(p, Btn::Soft, Some("plus"), "Log time by hand", Message::ManualAdd),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(p.u * 1.5)
            ),
        ]
        .spacing(p.u * 2.0);

        let wide = self.wide();
        let body: El = if wide {
            row![
                container(left)
                    .center_x(Length::FillPortion(5))
                    .padding([p.u * 2.0, 0.0]),
                container(vscroll(p, right)).width(Length::FillPortion(4)),
            ]
            .spacing(p.u * 3.0)
            .into()
        } else {
            vscroll(
                p,
                column![container(left).center_x(Fill).padding([p.u, 0.0]), right]
                    .spacing(p.u * 3.0),
            )
        };
        page(
            p,
            true,
            "Timer",
            Some(action(
                p,
                Btn::Ghost,
                Some("zen"),
                "Zen",
                Message::Zen(true),
            )),
            body,
        )
    }

    pub fn view_zen(&self, p: P) -> El<'_> {
        let (_, selected) = self.task_choices();
        let task: El = match selected.id {
            Some(_) => text(selected.label).size(18).color(p.dim).into(),
            None => Space::new().into(),
        };
        column![
            row![
                space::horizontal(),
                action(
                    p,
                    Btn::Ghost,
                    Some("x"),
                    "Exit zen (Esc)",
                    Message::Zen(false)
                )
            ]
            .padding(p.u * 2.0),
            center(
                column![self.ring(p, 420.0), task, self.controls(p)]
                    .spacing(p.u * 3.0)
                    .align_x(Alignment::Center)
            ),
        ]
        .into()
    }

    pub fn view_today(&self, p: P) -> El<'_> {
        let today = self.today;

        // ---- schedule ----
        let mut schedule = column![].spacing(6);
        let mut any = false;
        for i in recur::on_day(&self.items, today) {
            any = true;
            schedule = schedule.push(self.item_row(p, i, today));
        }
        for (c, board, done) in &self.due_cards {
            let Some(due) = c.due else { continue };
            if *done || due > today {
                continue;
            }
            any = true;
            let late = due < today;
            schedule = schedule.push(
                button(
                    row![
                        icon("flag", 15.0, p.deadline),
                        text(&c.title).size(13).width(Fill),
                        chip(p, board.clone(), p.dim),
                        chip(
                            p,
                            if late {
                                format!("overdue · {}", dates::relative(due, today))
                            } else {
                                "due today".into()
                            },
                            if late { p.danger } else { p.deadline }
                        ),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([5.0, 6.0])
                .style(btn(p, Btn::Ghost))
                .on_press(Message::CardOpen(c.id)),
            );
        }
        // Overdue standalone deadlines.
        for i in self.items.iter().filter(|i| {
            i.kind == ItemKind::Deadline && !i.done && i.recur == Recur::None && i.date < today
        }) {
            any = true;
            schedule = schedule.push(self.item_row(p, i, i.date));
        }
        if !any {
            schedule = schedule.push(empty(p, "Nothing scheduled or due today."));
        }

        // ---- habits ----
        let mut habit_rows = column![].spacing(6);
        let mut open = 0;
        for h in self.habits.iter().filter(|h| !h.archived) {
            let state = habits::day_state(h, today, &self.habit_ctx(h.id));
            if matches!(state, DayState::Rest | DayState::Outside) {
                continue;
            }
            open += 1;
            habit_rows = habit_rows.push(self.habit_today_row(p, h, today, true));
        }
        if open == 0 {
            habit_rows = habit_rows.push(empty(
                p,
                if self.habits.is_empty() {
                    "No habits yet. Add one under Habits."
                } else {
                    "No habits scheduled today."
                },
            ));
        }

        // ---- focus ----
        let goal = self.cfg.timer.daily_goal;
        let done = self.completed_today();
        let focus = column![meter(
            p,
            if goal > 0 {
                format!("{done} of {goal} sessions")
            } else {
                format!("{done} sessions")
            },
            dates::human(self.focus_today()),
            if goal > 0 {
                done as f32 / goal as f32
            } else {
                0.0
            },
            p.focus,
        ),];

        let mut tasks = column![].spacing(4);
        if self.in_progress.is_empty() {
            tasks = tasks.push(empty(
                p,
                "Move a card to In progress to start working on it.",
            ));
        }
        let active_card = self.timer.active.as_ref().and_then(|a| a.card_id);
        for (c, board) in &self.in_progress {
            let running = active_card == Some(c.id) && self.timer.is_running();
            let est = if c.estimate > 0.0 {
                format!(
                    "{:.1} / {} pomodoros",
                    c.spent_s as f64 / self.timer.cfg.focus_s.max(1) as f64,
                    trim_num(c.estimate)
                )
            } else {
                dates::human(c.spent_s)
            };
            tasks = tasks.push(
                row![
                    button(icon(
                        if running { "pause" } else { "play" },
                        14.0,
                        p.on_accent
                    ))
                    .padding(7)
                    .style(btn(p, Btn::Primary))
                    .on_press(if running {
                        Message::TimerToggle
                    } else {
                        Message::StartCard(c.id)
                    }),
                    button(
                        column![
                            text(&c.title).size(13),
                            text(format!("{board}  ·  {est}")).size(11).color(p.dim)
                        ]
                        .spacing(1)
                    )
                    .width(Fill)
                    .padding([4.0, 8.0])
                    .style(btn(p, Btn::Ghost))
                    .on_press(Message::CardOpen(c.id)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        let wide = self.wide();
        let (focus, schedule, habits_box, tasks) = (
            section(p, "FOCUS", focus),
            section(p, "SCHEDULE", schedule),
            section(p, "HABITS", habit_rows),
            section(p, "IN PROGRESS", tasks),
        );
        let body: El = if wide {
            row![
                column![schedule, tasks]
                    .spacing(p.u * 2.0)
                    .width(Length::FillPortion(3)),
                column![focus, habits_box]
                    .spacing(p.u * 2.0)
                    .width(Length::FillPortion(2)),
            ]
            .spacing(p.u * 2.0)
            .into()
        } else {
            column![focus, schedule, habits_box, tasks]
                .spacing(p.u * 1.5)
                .into()
        };
        page(
            p,
            wide,
            today.format("%A, %-d %B").to_string(),
            Some(
                row![
                    action(
                        p,
                        Btn::Soft,
                        Some("plus"),
                        "Appointment",
                        Message::ItemNew(ItemKind::Appointment, today)
                    ),
                    action(
                        p,
                        Btn::Soft,
                        Some("plus"),
                        "Deadline",
                        Message::ItemNew(ItemKind::Deadline, today)
                    ),
                    action(
                        p,
                        Btn::Ghost,
                        Some("calendar"),
                        "Calendar",
                        Message::Go(View::Calendar)
                    ),
                ]
                .spacing(8)
                .wrap()
                .into(),
            ),
            vscroll(p, body),
        )
    }
}

pub fn trim_num(v: f64) -> String {
    if v.fract().abs() < 1e-9 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}
