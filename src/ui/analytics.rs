//! Review of where the time went.

use super::app::{App, El, Message, Range};
use super::shell::{action, empty, meter, page, section, vscroll};
use super::style::{self, Btn, P, alpha};
use super::timer::trim_num;
use super::widgets::{Bars, Cell, WeekGrid};
use crate::core::dates;
use chrono::{Datelike, Duration};
use iced::widget::{Space, canvas, column, container, row, text};
use iced::{Alignment, Color, Fill, Length};

fn tile<'a>(p: P, label: &'a str, value: String, note: String, tint: Color) -> El<'a> {
    container(
        column![
            text(label).size(10).color(p.dim),
            // System accent colours can be dark; lift them towards the text.
            text(value).size(24).color(style::mix(tint, p.text, 0.35)),
            text(note).size(11).color(p.dim),
        ]
        .spacing(3),
    )
    .padding(p.u * 1.75)
    .width(Fill)
    .style(style::card(p))
    .into()
}

fn ranked<'a>(p: P, rows: &[(String, i64)], tint: Color, none: &'a str) -> El<'a> {
    if rows.is_empty() {
        return empty(p, none);
    }
    let max = rows.iter().map(|r| r.1).max().unwrap_or(1).max(1) as f32;
    let mut col = column![].spacing(9);
    for (name, secs) in rows.iter().take(7) {
        col = col.push(meter(
            p,
            name.clone(),
            dates::human(*secs),
            *secs as f32 / max,
            tint,
        ));
    }
    col.into()
}

impl App {
    pub fn view_analytics(&self, p: P) -> El<'_> {
        let a = &self.ana;
        let s = &a.summary;
        let (from, to) = a.range.bounds(self.today);
        let span = (to - from).num_days() + 1;

        let mut ranges = row![].spacing(4);
        for r in Range::ALL {
            ranges = ranges.push(action(
                p,
                if r == a.range {
                    Btn::Selected
                } else {
                    Btn::Ghost
                },
                None,
                r.label(),
                Message::AnaRange(r),
            ));
        }

        let goal = self.cfg.timer.daily_goal;
        let per_session = if s.completed + s.unfinished > 0 {
            s.interruptions as f32 / (s.completed + s.unfinished) as f32
        } else {
            0.0
        };
        let tiles = [
            tile(
                p,
                "FOCUS TIME",
                dates::human(s.focus_s),
                match s.best_day {
                    Some((d, secs)) if span > 1 => {
                        format!("best: {} on {}", dates::human(secs), d.format("%-d %b"))
                    }
                    _ => format!("on {} day(s)", s.active_days),
                },
                p.focus,
            ),
            tile(
                p,
                "SESSIONS",
                s.completed.to_string(),
                if s.unfinished > 0 {
                    format!("{} given up", s.unfinished)
                } else {
                    "all finished".into()
                },
                p.text,
            ),
            tile(
                p,
                "GOAL REACHED",
                if goal > 0 {
                    format!("{} / {span}", s.goal_days)
                } else {
                    "–".into()
                },
                if goal > 0 {
                    format!("days with {goal}+ sessions")
                } else {
                    "no daily goal set".into()
                },
                p.success,
            ),
            tile(
                p,
                "INTERRUPTIONS",
                s.interruptions.to_string(),
                format!("{per_session:.1} per session"),
                if per_session > 1.0 { p.warning } else { p.text },
            ),
            tile(
                p,
                "HABITS KEPT",
                a.habit_rate
                    .map_or("–".into(), |r| format!("{:.0}%", r * 100.0)),
                format!("{} card(s) finished", s.cards_done),
                p.habit,
            ),
        ];
        let wide = self.wide();
        let mut tile_rows = column![].spacing(p.u * 1.5);
        let mut line = row![].spacing(p.u * 1.5);
        for (i, t) in tiles.into_iter().enumerate() {
            // Five across when there is room, otherwise three then two.
            if !wide && i == 3 {
                tile_rows = tile_rows.push(std::mem::replace(&mut line, row![].spacing(p.u * 1.5)));
            }
            line = line.push(t);
        }
        let tiles = tile_rows.push(line);

        // Focus over the range: per day, or per week for the year.
        let (values, labels, highlight): (Vec<f32>, Vec<String>, Option<usize>) = if span > 100 {
            let first = dates::week_start(from);
            let weeks = ((to - first).num_days() / 7 + 1) as usize;
            let mut v = vec![0f32; weeks];
            for (d, day) in &a.days {
                let i = ((*d - first).num_days() / 7) as usize;
                if let Some(slot) = v.get_mut(i) {
                    *slot += day.focus_s as f32 / 3600.0;
                }
            }
            let labels = (0..weeks)
                .map(|i| {
                    let d = first + Duration::weeks(i as i64);
                    if d.day() <= 7 {
                        d.format("%b").to_string()
                    } else {
                        String::new()
                    }
                })
                .collect();
            (v, labels, Some(weeks - 1))
        } else {
            let n = span as usize;
            let v = (0..n)
                .map(|i| {
                    a.days
                        .get(&(from + Duration::days(i as i64)))
                        .map_or(0.0, |d| d.focus_s as f32 / 3600.0)
                })
                .collect();
            let every = if n <= 10 {
                1
            } else if n <= 31 {
                5
            } else {
                14
            };
            let labels = (0..n)
                .map(|i| {
                    let d = from + Duration::days(i as i64);
                    match () {
                        _ if n <= 7 => d.format("%a").to_string(),
                        _ if i % every == 0 => d.format("%-d %b").to_string(),
                        _ => String::new(),
                    }
                })
                .collect();
            (v, labels, Some(n - 1))
        };
        let goal_hours =
            (span <= 100 && goal > 0).then(|| goal as f32 * self.timer.cfg.focus_s as f32 / 3600.0);
        let trend = canvas(Bars {
            values,
            labels,
            color: p.focus,
            text: p.dim,
            grid: alpha(p.outline, 0.8),
            highlight,
            goal: goal_hours,
        })
        .width(Fill)
        .height(150);

        let hour_max = a.hours.iter().copied().max().unwrap_or(0);
        let hours = canvas(Bars {
            values: a.hours.iter().map(|s| *s as f32).collect(),
            labels: (0..24)
                .map(|h| {
                    if h % 3 == 0 {
                        format!("{h:02}")
                    } else {
                        String::new()
                    }
                })
                .collect(),
            color: p.appointment,
            text: p.dim,
            grid: alpha(p.outline, 0.8),
            highlight: a.hours.iter().position(|s| *s == hour_max && hour_max > 0),
            goal: None,
        })
        .width(Fill)
        .height(120);

        // A year of days, always, regardless of the chosen range.
        let grid_from = dates::week_start(self.today) - Duration::weeks(52);
        let max_day = self
            .day_stats
            .values()
            .map(|d| d.focus_s)
            .max()
            .unwrap_or(0)
            .max(1) as f32;
        let cells = (0..53 * 7)
            .map(|i| {
                let d = grid_from + Duration::days(i);
                if d > self.today {
                    return Cell::None;
                }
                match self.day_stats.get(&d) {
                    Some(day) if day.focus_s > 0 => {
                        let t = (day.focus_s as f32 / max_day).sqrt();
                        Cell::Fill(alpha(p.focus, 0.25 + 0.75 * t))
                    }
                    _ => Cell::Empty,
                }
            })
            .collect();
        let heat = WeekGrid {
            cells,
            empty: p.surface2,
            cell: 12.0,
            gap: 3.0,
        };
        let heat_size = heat.size();

        let mut est = column![].spacing(9);
        if a.estimates.is_empty() {
            est = est.push(empty(
                p,
                "Finish cards that have an estimate to see how accurate you are.",
            ));
        }
        for e in &a.estimates {
            let ratio = (e.actual / e.estimate) as f32;
            let tint = if ratio > 1.25 {
                p.warning
            } else if ratio < 0.75 {
                p.appointment
            } else {
                p.success
            };
            est = est.push(meter(
                p,
                e.title.clone(),
                format!("{:.1} of {} estimated", e.actual, trim_num(e.estimate)),
                ratio / 2.0,
                tint,
            ));
        }

        let reasons_max = a.reasons.iter().map(|r| r.1).max().unwrap_or(1).max(1) as f32;
        let mut why = column![].spacing(9);
        if a.reasons.is_empty() {
            why = why.push(empty(p, "No interruptions in this period."));
        }
        for (reason, n) in &a.reasons {
            why = why.push(meter(
                p,
                reason.clone(),
                format!("{n}×"),
                *n as f32 / reasons_max,
                p.warning,
            ));
        }

        let body = column![
            tiles,
            section(
                p,
                if span > 100 {
                    "FOCUS HOURS PER WEEK"
                } else {
                    "FOCUS HOURS PER DAY"
                },
                trend
            ),
            half(
                p,
                wide,
                section(
                    p,
                    "BY TASK",
                    ranked(p, &a.by_card, p.focus, "No focus time in this period.")
                ),
                section(
                    p,
                    "BY TAG",
                    ranked(p, &a.by_tag, p.accent, "Tag cards to see time per tag.")
                ),
            ),
            half(
                p,
                wide,
                section(
                    p,
                    "BY BOARD",
                    ranked(
                        p,
                        &a.by_board,
                        p.long_break,
                        "No focus time in this period."
                    )
                ),
                section(p, "TIME OF DAY", hours),
            ),
            half(
                p,
                wide,
                section(p, "ESTIMATE VS ACTUAL", est),
                section(p, "WHAT INTERRUPTED YOU", why)
            ),
            section(
                p,
                "THE LAST YEAR",
                column![
                    iced::widget::scrollable(
                        container(
                            canvas(heat)
                                .width(Length::Fixed(heat_size.width))
                                .height(Length::Fixed(heat_size.height))
                        )
                        .padding(iced::Padding {
                            bottom: 10.0,
                            ..Default::default()
                        })
                    )
                    .direction(iced::widget::scrollable::Direction::Horizontal(
                        Default::default()
                    ))
                    .anchor_right()
                    .style(style::scroll(p))
                    .width(Fill),
                    row![
                        text("less").size(10).color(p.dim),
                        legend(p, 0.25),
                        legend(p, 0.5),
                        legend(p, 0.75),
                        legend(p, 1.0),
                        text("more focus").size(10).color(p.dim)
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                ]
                .spacing(8)
            ),
        ]
        .spacing(p.u * 1.5);

        page(
            p,
            wide,
            "Analytics",
            Some(ranges.wrap().into()),
            vscroll(p, body),
        )
    }
}

fn legend<'a>(p: P, strength: f32) -> El<'a> {
    container(Space::new().width(11).height(11))
        .style(style::panel(p, alpha(p.focus, strength)))
        .into()
}

/// Two sections side by side, or stacked when there is no room.
fn half<'a>(p: P, wide: bool, a: El<'a>, b: El<'a>) -> El<'a> {
    if wide {
        row![
            container(a).width(Length::FillPortion(1)),
            container(b).width(Length::FillPortion(1))
        ]
        .spacing(p.u * 1.5)
        .into()
    } else {
        column![a, b].spacing(p.u * 1.5).into()
    }
}
