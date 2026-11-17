//! The kanban board and the card editor.

use super::app::{
    App, CardEdit, Choice, ConfirmAction, Drag, El, Field, Message, Modal, PromptAction,
};
use super::shell::{action, field, icon_btn, labelled, page};
use super::style::{self, Btn, P, alpha, btn, chip, icon};
use super::timer::trim_num;
use crate::core::dates;
use crate::core::model::*;
use chrono::Local;
use iced::widget::{
    Space, button, column, container, mouse_area, pick_list, row, scrollable, space, text,
    text_editor,
};
use iced::{Alignment, Border, Fill, Length, Task};

impl App {
    pub(super) fn update_board(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::BoardPick(c) => {
                self.board_id = c.id;
                if let Some(id) = c.id {
                    let _ = self.db.set_meta("board", &id.to_string());
                }
                self.adding_to = None;
                self.reload_board();
            }
            Message::CardAddTo(list) => {
                self.adding_to = list;
                self.new_card.clear();
                if list.is_some() {
                    return iced::widget::operation::focus("new-card");
                }
            }
            Message::CardPress(id) => {
                // The pointer is already inside this card and its list, so no
                // "enter" event will arrive for them.
                let list = self.cards.iter().find(|c| c.id == id).map(|c| c.list_id);
                self.drag = Some(Drag {
                    card: id,
                    over_card: Some(id),
                    over_list: list,
                    ..Default::default()
                });
            }
            Message::CardHover(id, on) => {
                if let Some(d) = self.drag.as_mut() {
                    if on {
                        d.over_card = Some(id);
                    } else if d.over_card == Some(id) {
                        d.over_card = None;
                    }
                }
            }
            Message::ListHover(id, on) => {
                let home = self
                    .drag
                    .as_ref()
                    .and_then(|d| self.cards.iter().find(|c| c.id == d.card))
                    .map(|c| c.list_id);
                if let Some(d) = self.drag.as_mut() {
                    if on {
                        d.over_list = Some(id);
                        // Entering another list means the card was left
                        // behind, even if its "exit" event was missed.
                        if d.over_card == Some(d.card) && home != Some(id) {
                            d.over_card = None;
                        }
                    } else if d.over_list == Some(id) {
                        d.over_list = None;
                    }
                }
            }
            Message::CardOpen(id) => {
                let found = self.db.card(id);
                if let Some(Some(c)) = self.check(found) {
                    self.modal = Modal::Card(Box::new(CardEdit {
                        id,
                        title: c.title,
                        notes: text_editor::Content::with_text(&c.notes),
                        estimate: if c.estimate > 0.0 {
                            trim_num(c.estimate)
                        } else {
                            String::new()
                        },
                        due: c.due.map(|d| d.to_string()).unwrap_or_default(),
                        tags: c.tags.replace(',', ", "),
                        list: c.list_id,
                        // The card may be on a board other than the one on screen.
                        lists: self
                            .db
                            .sibling_lists(c.list_id)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|l| Choice {
                                id: Some(l.id),
                                label: l.name,
                            })
                            .collect(),
                    }));
                }
            }
            Message::CardNotes(a) => {
                if let Modal::Card(c) = &mut self.modal {
                    c.notes.perform(a);
                }
            }
            Message::CardList(choice) => {
                if let (Modal::Card(c), Some(id)) = (&mut self.modal, choice.id) {
                    c.list = id;
                }
            }
            Message::CardSave => self.card_save(),
            _ => {}
        }
        Task::none()
    }

    fn card_save(&mut self) {
        let Modal::Card(e) = &self.modal else { return };
        let title = e.title.trim().to_string();
        if title.is_empty() {
            self.toast("A card needs a title.", true);
            return;
        }
        let estimate = match e.estimate.trim() {
            "" => 0.0,
            s => match s.replace(',', ".").parse::<f64>() {
                Ok(v) if (0.0..=999.0).contains(&v) => v,
                _ => {
                    self.toast("The estimate must be a number of pomodoros.", true);
                    return;
                }
            },
        };
        let due = match e.due.trim() {
            "" => None,
            s => match dates::parse_date(s, self.today) {
                Some(d) => Some(d),
                None => {
                    self.toast(
                        "Could not read the due date. Try 2026-12-01, tomorrow, fri or +3d.",
                        true,
                    );
                    return;
                }
            },
        };
        let (id, list) = (e.id, e.list);
        let notes = e.notes.text().trim_end().to_string();
        let tags = e.tags.clone();
        let found = self.db.card(id);
        let Some(Some(mut card)) = self.check(found) else {
            self.modal = Modal::None;
            return;
        };
        card.title = title;
        card.notes = notes;
        card.estimate = estimate;
        card.due = due;
        card.tags = tags;
        let res = self.db.update_card(&card);
        self.check(res);
        if list != card.list_id {
            let res = self
                .db
                .move_card(id, list, usize::MAX, Local::now().naive_local());
            self.check(res);
        }
        self.modal = Modal::None;
        self.reload_board();
        self.reload_sessions();
    }

    /// Finishes a drag: the card lands before the card it was dropped on,
    /// or at the end of the list it was dropped in.
    pub(super) fn drop_card(&mut self, d: &Drag) {
        // Dropped back on itself: leave it where it was.
        if d.over_card == Some(d.card) {
            return;
        }
        let over = d
            .over_card
            .and_then(|c| self.cards.iter().find(|k| k.id == c));
        let list = over.map(|c| c.list_id).or(d.over_list);
        let Some(list) = list else { return };
        let index = match over {
            Some(target) => self
                .cards
                .iter()
                .filter(|c| c.list_id == list && c.id != d.card)
                .position(|c| c.id == target.id)
                .unwrap_or(usize::MAX),
            None => usize::MAX,
        };
        let res = self
            .db
            .move_card(d.card, list, index, Local::now().naive_local());
        self.check(res);
        self.reload_board();
        self.reload_sessions();
    }

    pub(super) fn card_matches(&self, c: &Card) -> bool {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        // "#tag" matches tags only; anything else matches title, notes and tags.
        match q.strip_prefix('#') {
            Some(tag) => c.tag_list().iter().any(|t| t.starts_with(tag)),
            None => {
                c.title.to_lowercase().contains(&q)
                    || c.notes.to_lowercase().contains(&q)
                    || c.tags.to_lowercase().contains(&q)
            }
        }
    }

    fn card_tile<'a>(&'a self, p: P, c: &'a Card, done_list: bool) -> El<'a> {
        let drag = self.drag.as_ref().filter(|d| d.active);
        let dragging = drag.is_some_and(|d| d.card == c.id);
        let target = drag.is_some_and(|d| d.over_card == Some(c.id) && d.card != c.id);
        // Chosen with the keyboard.
        let target = target || self.board_sel == Some(c.id);
        let focus_s = self.timer.cfg.focus_s.max(1) as f64;

        let mut meta = row![].spacing(5).align_y(Alignment::Center);
        if let Some(due) = c.due {
            let late = due < self.today && !done_list;
            meta = meta.push(chip(
                p,
                dates::relative(due, self.today),
                if late {
                    p.danger
                } else if done_list {
                    p.dim
                } else {
                    p.deadline
                },
            ));
        }
        for t in c.tag_list().into_iter().take(3) {
            meta = meta.push(chip(p, format!("#{t}"), p.accent));
        }

        let mut body = column![text(&c.title).size(13)].spacing(6);
        if c.due.is_some() || !c.tags.is_empty() {
            body = body.push(meta);
        }
        if c.estimate > 0.0 || c.spent_s > 0 {
            let spent = c.spent_s as f64 / focus_s;
            let label = if c.estimate > 0.0 {
                format!("{spent:.1} / {}", trim_num(c.estimate))
            } else {
                dates::human(c.spent_s)
            };
            let over = c.estimate > 0.0 && spent > c.estimate + 0.05;
            let frac = if c.estimate > 0.0 {
                (spent / c.estimate).min(1.0) as f32
            } else {
                0.0
            };
            let tint = if over { p.warning } else { p.focus };
            let filled = (frac * 100.0).round() as u16;
            let mut line = row![
                icon("timer", 12.0, p.dim),
                text(label).size(11).color(p.dim)
            ]
            .spacing(5)
            .align_y(Alignment::Center);
            if c.estimate > 0.0 {
                line = line.push(
                    container(row![
                        container(Space::new().height(4))
                            .width(Length::FillPortion(filled.max(1)))
                            .style(style::panel(
                                p,
                                if filled == 0 {
                                    iced::Color::TRANSPARENT
                                } else {
                                    tint
                                }
                            )),
                        Space::new().width(Length::FillPortion((100 - filled).max(1))),
                    ])
                    .width(Fill)
                    .style(style::panel(p, p.surface3)),
                );
            }
            body = body.push(line);
        }

        let tile = container(body)
            .padding(p.u * 1.25)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(
                    if dragging {
                        alpha(p.surface2, 0.4)
                    } else {
                        p.surface2
                    }
                    .into(),
                ),
                border: Border {
                    color: if target {
                        p.accent
                    } else {
                        alpha(p.outline, 0.5)
                    },
                    width: if target { 2.0 } else { 1.0 },
                    radius: (p.radius * 0.8).into(),
                },
                text_color: Some(if dragging { p.dim } else { p.text }),
                ..Default::default()
            });
        mouse_area(tile)
            .on_press(Message::CardPress(c.id))
            .on_enter(Message::CardHover(c.id, true))
            .on_exit(Message::CardHover(c.id, false))
            .interaction(iced::mouse::Interaction::Grab)
            .into()
    }

    fn list_column<'a>(&'a self, p: P, list: &'a List) -> El<'a> {
        let cards: Vec<&Card> = self
            .cards
            .iter()
            .filter(|c| c.list_id == list.id && self.card_matches(c))
            .collect();
        let total = self.cards.iter().filter(|c| c.list_id == list.id).count();
        let drop_here = self
            .drag
            .as_ref()
            .is_some_and(|d| d.active && d.over_list == Some(list.id) && d.over_card.is_none());
        let tint = match list.kind {
            ListKind::Progress => p.focus,
            ListKind::Done => p.success,
            ListKind::Normal => p.dim,
        };

        let mut head = row![
            container(Space::new().width(8).height(8)).style(style::panel(p, tint)),
            text(&list.name).size(13),
            text(if cards.len() == total {
                total.to_string()
            } else {
                format!("{} of {total}", cards.len())
            })
            .size(12)
            .color(p.dim),
            space::horizontal(),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        head = head.push(icon_btn(
            p,
            Btn::Ghost,
            "edit",
            Message::PromptOpen(PromptAction::RenameList(list.id)),
        ));
        if list.kind == ListKind::Normal {
            head = head.push(icon_btn(
                p,
                Btn::Danger,
                "trash",
                Message::Ask(ConfirmAction::DeleteList(list.id)),
            ));
        }

        let mut col = column![].spacing(6);
        for c in cards {
            col = col.push(self.card_tile(p, c, list.kind == ListKind::Done));
        }
        if self.adding_to == Some(list.id) {
            col = col.push(
                row![
                    field(
                        p,
                        "Card title, Enter to add",
                        &self.new_card,
                        Field::NewCard
                    )
                    .id("new-card"),
                    icon_btn(p, Btn::Ghost, "x", Message::CardAddTo(None)),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        } else {
            col = col.push(
                button(
                    row![
                        icon("plus", 14.0, p.dim),
                        text("Add card").size(12).color(p.dim)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([6.0, 8.0])
                .style(btn(p, Btn::Ghost))
                .on_press(Message::CardAddTo(Some(list.id))),
            );
        }

        let body = container(
            column![
                head,
                scrollable(col.padding(iced::Padding {
                    right: 8.0,
                    ..Default::default()
                }))
                .style(style::scroll(p))
                .height(Fill)
            ]
            .spacing(p.u),
        )
        .padding(p.u * 1.25)
        .width(292)
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(p.surface.into()),
            border: Border {
                color: if drop_here {
                    p.accent
                } else {
                    alpha(p.outline, 0.6)
                },
                width: if drop_here { 2.0 } else { 1.0 },
                radius: p.radius.into(),
            },
            text_color: Some(p.text),
            ..Default::default()
        });
        mouse_area(body)
            .on_enter(Message::ListHover(list.id, true))
            .on_exit(Message::ListHover(list.id, false))
            .into()
    }

    pub fn view_board(&self, p: P) -> El<'_> {
        let choices: Vec<Choice> = self
            .boards
            .iter()
            .map(|b| Choice {
                id: Some(b.id),
                label: b.name.clone(),
            })
            .collect();
        let selected = choices.iter().find(|c| c.id == self.board_id).cloned();

        let mut tools = row![
            pick_list(choices, selected, Message::BoardPick).text_size(13),
            icon_btn(
                p,
                Btn::Ghost,
                "plus",
                Message::PromptOpen(PromptAction::NewBoard)
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if let Some(b) = self.board_id {
            tools = tools.push(icon_btn(
                p,
                Btn::Ghost,
                "edit",
                Message::PromptOpen(PromptAction::RenameBoard(b)),
            ));
            tools = tools.push(icon_btn(
                p,
                Btn::Danger,
                "trash",
                Message::Ask(ConfirmAction::DeleteBoard(b)),
            ));
        }
        tools = tools
            .push(Space::new().width(p.u))
            .push(
                field(p, "Search cards, or #tag", &self.search, Field::BoardSearch)
                    .id("board-search")
                    .width(220),
            )
            .push(action(
                p,
                Btn::Soft,
                Some("plus"),
                "List",
                Message::PromptOpen(PromptAction::NewList),
            ));

        let mut lists = row![].spacing(p.u * 1.5);
        for l in &self.lists {
            lists = lists.push(self.list_column(p, l));
        }
        let board = scrollable(lists.padding(iced::Padding {
            bottom: 12.0,
            ..Default::default()
        }))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .style(style::scroll(p))
        .width(Fill)
        .height(Fill);

        page(p, self.wide(), "Board", Some(tools.wrap().into()), board)
    }

    pub(super) fn card_editor<'a>(&'a self, p: P, e: &'a CardEdit) -> El<'a> {
        let lists = e.lists.clone();
        let selected = lists.iter().find(|c| c.id == Some(e.list)).cloned();
        let due_bad = !e.due.trim().is_empty() && dates::parse_date(&e.due, self.today).is_none();
        let est_bad = !e.estimate.trim().is_empty()
            && e.estimate.trim().replace(',', ".").parse::<f64>().is_err();

        column![
            field(p, "Title", &e.title, Field::CardTitle).size(16),
            text_editor(&e.notes)
                .placeholder("Notes")
                .on_action(Message::CardNotes)
                .height(120)
                .size(13),
            row![
                labelled(
                    p,
                    "ESTIMATE (POMODOROS)",
                    field(p, "e.g. 3", &e.estimate, Field::CardEstimate)
                        .style(style::input_checked(p, est_bad))
                ),
                labelled(
                    p,
                    "DUE",
                    field(p, "2026-12-01, fri, +3d", &e.due, Field::CardDue)
                        .style(style::input_checked(p, due_bad))
                ),
            ]
            .spacing(p.u * 1.5),
            row![
                labelled(
                    p,
                    "TAGS",
                    field(p, "writing, deep", &e.tags, Field::CardTags)
                ),
                labelled(
                    p,
                    "LIST",
                    pick_list(lists, selected, Message::CardList)
                        .width(170)
                        .text_size(13)
                ),
            ]
            .spacing(p.u * 1.5),
            row![
                action(
                    p,
                    Btn::Danger,
                    Some("trash"),
                    "Delete",
                    Message::Ask(ConfirmAction::DeleteCard(e.id))
                ),
                space::horizontal(),
                action(
                    p,
                    Btn::Soft,
                    Some("play"),
                    "Focus on this",
                    Message::StartCard(e.id)
                ),
                action(p, Btn::Ghost, None, "Cancel", Message::CloseModal),
                action(p, Btn::Primary, None, "Save", Message::CardSave),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(p.u * 1.5)
        .into()
    }
}
