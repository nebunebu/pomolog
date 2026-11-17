//! Vim-style keys: single letters that move around and act on what is
//! selected. They only fire when no text field or dialog has the keyboard.

use super::app::{App, El, Message, Modal, Toggle, View, now};
use super::style::{self, P, chip};
use crate::core::dates;
use crate::core::model::*;
use chrono::Duration;
use iced::widget::{column, row, text};
use iced::{Alignment, Fill, Task, keyboard};

/// What `?` shows.
const HELP: [(&str, &[(&str, &str)]); 5] = [
    (
        "ANYWHERE",
        &[
            (
                "g d/t/b/c/h/a/s",
                "go to Today, Timer, Board, Calendar, Habits, Analytics, Settings",
            ),
            (":", "command palette (also Ctrl+K)"),
            ("/", "search the board"),
            ("Space", "start or pause the timer"),
            ("?", "this list"),
        ],
    ),
    (
        "CALENDAR",
        &[
            ("h l", "previous / next day"),
            ("j k", "next / previous week"),
            ("H L", "previous / next month (week in week view)"),
            ("t", "today"),
            ("m w", "month / week view"),
            ("a d n", "new appointment / deadline / note on the day"),
            ("Enter", "open the day's first item"),
        ],
    ),
    (
        "BOARD",
        &[
            ("h l", "previous / next list"),
            ("j k", "next / previous card"),
            ("H L", "move the card to the list on the left / right"),
            ("J K", "move the card down / up"),
            ("o", "add a card to the list"),
            ("Enter", "open the card"),
            ("s", "start a focus session on the card"),
        ],
    ),
    (
        "HABITS",
        &[
            ("j k", "next / previous habit"),
            ("x", "tick or untick it for today"),
            ("Enter", "edit the habit"),
        ],
    ),
    ("TIMER", &[("s", "skip to the next phase")]),
];

impl App {
    /// Handles a key as a vim-style command. `None` means it was not one and
    /// the ordinary shortcuts get it. `chord` is true right after a `g`.
    pub(super) fn vim_key(
        &mut self,
        key: &keyboard::Key,
        mods: keyboard::Modifiers,
        chord: bool,
    ) -> Option<Task<Message>> {
        use keyboard::Key;
        use keyboard::key::Named;
        let c: String = match key.as_ref() {
            Key::Named(Named::Enter) => "enter".into(),
            // Some layouts report the unshifted key along with the modifier.
            Key::Character("/") if mods.shift() => "?".into(),
            Key::Character(";") if mods.shift() => ":".into(),
            Key::Character(c) if mods.shift() => c.to_uppercase(),
            Key::Character(c) => c.to_string(),
            _ => return None,
        };
        let c = c.as_str();
        if chord {
            let view = match c {
                "d" | "g" => View::Today,
                "t" => View::Timer,
                "b" => View::Board,
                "c" => View::Calendar,
                "h" => View::Habits,
                "a" => View::Analytics,
                "s" => View::Settings,
                _ => return Some(Task::none()),
            };
            return Some(self.update(Message::Go(view)));
        }
        match c {
            "g" => {
                self.pending_g = true;
                return Some(Task::none());
            }
            ":" => {
                self.modal = Modal::Palette {
                    query: String::new(),
                };
                return Some(iced::widget::operation::focus("palette"));
            }
            "?" => {
                self.modal = Modal::Keys;
                return Some(Task::none());
            }
            "/" => {
                self.view = View::Board;
                return Some(iced::widget::operation::focus("board-search"));
            }
            _ => {}
        }
        match self.view {
            View::Calendar => self.calendar_key(c),
            View::Board => self.board_key(c),
            View::Habits => self.habits_key(c),
            View::Timer if c == "s" => Some(self.update(Message::TimerSkip)),
            _ => None,
        }
    }

    fn calendar_key(&mut self, c: &str) -> Option<Task<Message>> {
        let day = self.cal_selected;
        let step = |app: &mut App, days: i64| {
            app.cal_selected = day + Duration::days(days);
            app.cal_month = dates::month_start(app.cal_selected);
            Task::none()
        };
        Some(match c {
            "h" => step(self, -1),
            "l" => step(self, 1),
            "j" => step(self, 7),
            "k" => step(self, -7),
            "H" => self.update(Message::CalShift(-1)),
            "L" => self.update(Message::CalShift(1)),
            "t" => self.update(Message::CalToday),
            "m" => self.update(Message::Toggle(Toggle::CalWeek, false)),
            "w" => self.update(Message::Toggle(Toggle::CalWeek, true)),
            "a" => self.update(Message::ItemNew(ItemKind::Appointment, day)),
            "d" => self.update(Message::ItemNew(ItemKind::Deadline, day)),
            "n" => self.update(Message::ItemNew(ItemKind::Note, day)),
            "enter" => {
                let first = self.day_items(day).first().map(|o| o.item.id)?;
                self.update(Message::ItemOpen(first))
            }
            _ => return None,
        })
    }

    /// The cards shown in a list, top to bottom.
    fn list_cards(&self, list: usize) -> Vec<Id> {
        let Some(l) = self.lists.get(list) else {
            return Vec::new();
        };
        self.cards
            .iter()
            .filter(|c| c.list_id == l.id && self.card_matches(c))
            .map(|c| c.id)
            .collect()
    }

    fn board_key(&mut self, c: &str) -> Option<Task<Message>> {
        if self.lists.is_empty() {
            return None;
        }
        let last = self.lists.len() - 1;
        // The selected card may have been moved by the mouse since.
        if let Some(card) = self
            .board_sel
            .and_then(|id| self.cards.iter().find(|c| c.id == id))
            && let Some(i) = self.lists.iter().position(|l| l.id == card.list_id)
        {
            self.board_list = i;
        }
        self.board_list = self.board_list.min(last);
        let cards = self.list_cards(self.board_list);
        let pos = self
            .board_sel
            .and_then(|id| cards.iter().position(|c| *c == id));
        if pos.is_none() {
            self.board_sel = None;
        }
        let list_id = self.lists[self.board_list].id;
        match c {
            "j" | "k" => {
                let next = match (pos, c) {
                    (Some(i), "j") => (i + 1).min(cards.len().saturating_sub(1)),
                    (Some(i), _) => i.saturating_sub(1),
                    (None, "j") => 0,
                    (None, _) => cards.len().saturating_sub(1),
                };
                self.board_sel = cards.get(next).copied();
            }
            "h" | "l" => {
                self.board_list = if c == "h" {
                    self.board_list.saturating_sub(1)
                } else {
                    (self.board_list + 1).min(last)
                };
                let there = self.list_cards(self.board_list);
                self.board_sel = there
                    .get(pos.unwrap_or(0).min(there.len().saturating_sub(1)))
                    .copied();
            }
            "H" | "L" => {
                let id = self.board_sel?;
                let to = if c == "H" {
                    self.board_list.checked_sub(1)?
                } else {
                    Some(self.board_list + 1).filter(|i| *i <= last)?
                };
                let res = self.db.move_card(id, self.lists[to].id, usize::MAX, now());
                self.check(res);
                self.board_list = to;
                self.reload_board();
            }
            "J" | "K" => {
                let (id, i) = (self.board_sel?, pos?);
                let to = if c == "K" {
                    i.checked_sub(1)?
                } else {
                    Some(i + 1).filter(|n| *n < cards.len())?
                };
                let res = self.db.move_card(id, list_id, to, now());
                self.check(res);
                self.reload_board();
            }
            "o" => return Some(self.update(Message::CardAddTo(Some(list_id)))),
            "enter" => return Some(self.update(Message::CardOpen(self.board_sel?))),
            "s" => return Some(self.update(Message::StartCard(self.board_sel?))),
            _ => return None,
        }
        Some(Task::none())
    }

    fn habits_key(&mut self, c: &str) -> Option<Task<Message>> {
        let ids: Vec<Id> = self
            .habits
            .iter()
            .filter(|h| !h.archived)
            .map(|h| h.id)
            .collect();
        let pos = self
            .habit_sel
            .and_then(|id| ids.iter().position(|h| *h == id));
        match c {
            "j" | "k" => {
                let next = match (pos, c) {
                    (Some(i), "j") => (i + 1).min(ids.len().saturating_sub(1)),
                    (Some(i), _) => i.saturating_sub(1),
                    (None, "j") => 0,
                    (None, _) => ids.len().saturating_sub(1),
                };
                self.habit_sel = ids.get(next).copied();
                Some(Task::none())
            }
            "x" => {
                let id = ids.get(pos?).copied()?;
                let done = self.stored_entry(id, self.today).value > 0.0;
                Some(self.update(Message::HabitCheck(id, self.today, !done)))
            }
            "enter" => Some(self.update(Message::HabitOpen(ids.get(pos?).copied()?))),
            _ => None,
        }
    }

    pub(super) fn keys_help(&self, p: P) -> El<'_> {
        let mut col = column![text("Keys").size(17)].spacing(p.u * 1.5);
        for (title, keys) in HELP {
            let mut rows = column![style::h2(p, title)].spacing(4);
            for (key, what) in keys {
                rows = rows.push(
                    row![
                        chip(p, *key, p.accent),
                        text(*what).size(13).width(Fill).color(p.text)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
            }
            col = col.push(rows);
        }
        if !self.cfg.keys.vim {
            col = col.push(
                style::dim(p, "The single-letter keys are switched off in Settings.").size(12),
            );
        }
        super::shell::vscroll(p, col)
    }
}
