//! The window frame: sidebar, modal layer, toast, and small form helpers.

use super::app::{Action, App, ConfirmAction, El, Field, Message, Modal, View};
use super::style::{self, Btn, P, alpha, btn, icon};
use crate::core::dates;
use crate::core::model::Phase;
use iced::widget::{
    Space, bottom_center, button, center, column, container, mouse_area, opaque, pin, row,
    scrollable, space, stack, text, text_input,
};
use iced::{Alignment, Color, Element, Fill, Length, Padding, window};

/// A text button with an optional leading icon.
pub fn action<'a>(
    p: P,
    kind: Btn,
    icon_name: Option<&str>,
    label: &'a str,
    msg: Message,
) -> El<'a> {
    let fg = match kind {
        Btn::Primary => p.on_accent,
        Btn::Danger => p.dim,
        _ => p.text,
    };
    let mut content = row![].spacing(6).align_y(Alignment::Center);
    if let Some(name) = icon_name {
        content = content.push(icon(name, 15.0, fg));
    }
    if !label.is_empty() {
        content = content.push(text(label).size(13));
    }
    button(content)
        .padding([6.0, 11.0])
        .style(btn(p, kind))
        .on_press(msg)
        .into()
}

/// A square button holding just an icon.
pub fn icon_btn<'a>(p: P, kind: Btn, name: &str, msg: Message) -> El<'a> {
    let fg = if kind == Btn::Primary {
        p.on_accent
    } else {
        p.dim
    };
    button(icon(name, 15.0, fg))
        .padding(6)
        .style(btn(p, kind))
        .on_press(msg)
        .into()
}

/// A text input wired to a `Field`.
pub fn field<'a>(
    p: P,
    placeholder: &str,
    value: &str,
    f: Field,
) -> iced::widget::TextInput<'a, Message> {
    text_input(placeholder, value)
        .on_input(move |v| Message::Field(f, v))
        .on_submit(Message::Submit(f))
        .padding([7.0, 10.0])
        .size(13)
        .style(style::input(p))
}

/// A caption above a control.
pub fn labelled<'a>(p: P, label: &'a str, control: impl Into<El<'a>>) -> El<'a> {
    column![text(label).size(11).color(p.dim), control.into()]
        .spacing(4)
        .into()
}

pub fn section<'a>(p: P, title: &'a str, body: impl Into<El<'a>>) -> El<'a> {
    container(column![style::h2(p, title), body.into()].spacing(p.u * 1.25))
        .padding(p.u * 2.0)
        .width(Fill)
        .style(style::card(p))
        .into()
}

/// A view's frame: title, tools, body. When there is no room for the tools
/// beside the title they go underneath it.
pub fn page<'a>(
    p: P,
    wide: bool,
    title: impl Into<String>,
    right: Option<El<'a>>,
    body: impl Into<El<'a>>,
) -> El<'a> {
    let title = style::h1(title.into());
    let head: El = match (right, wide) {
        (Some(r), true) => row![title, space::horizontal(), r]
            .align_y(Alignment::Center)
            .spacing(p.u)
            .into(),
        (Some(r), false) => column![title, r].spacing(p.u).into(),
        (None, _) => title,
    };
    column![head, body.into()]
        .spacing(p.u * 2.0)
        .height(Fill)
        .into()
}

pub fn vscroll<'a>(p: P, content: impl Into<El<'a>>) -> El<'a> {
    scrollable(container(content).padding(Padding {
        right: 12.0,
        ..Padding::ZERO
    }))
    .style(style::scroll(p))
    .height(Fill)
    .width(Fill)
    .into()
}

impl App {
    pub fn view(&self, _window: window::Id) -> El<'_> {
        let p = self.p;
        let body: El = if self.zen {
            self.view_zen(p)
        } else {
            let content: El = match self.view {
                View::Today => self.view_today(p),
                View::Timer => self.view_timer(p),
                View::Board => self.view_board(p),
                View::Calendar => self.view_calendar(p),
                View::Habits => self.view_habits(p),
                View::Analytics => self.view_analytics(p),
                View::Settings => self.view_settings(p),
            };
            let pad = if self.wide() { p.u * 3.0 } else { p.u * 1.75 };
            row![
                self.sidebar(p),
                container(content).padding(pad).width(Fill).height(Fill)
            ]
            .into()
        };
        let mut layers = stack![
            container(body)
                .width(Fill)
                .height(Fill)
                .style(style::flat(p.bg))
        ]
        .width(Fill)
        .height(Fill);

        if let Some(dialog) = self.view_modal(p) {
            let top = matches!(self.modal, Modal::Palette { .. });
            let placed: El = if top {
                container(dialog)
                    .center_x(Fill)
                    .padding(Padding {
                        top: 90.0,
                        ..Padding::ZERO
                    })
                    .into()
            } else {
                center(dialog).into()
            };
            layers = layers.push(opaque(
                mouse_area(
                    container(placed)
                        .width(Fill)
                        .height(Fill)
                        .style(style::flat(Color {
                            a: if p.dark { 0.55 } else { 0.3 },
                            ..Color::BLACK
                        })),
                )
                .on_press(Message::CloseModal),
            ));
        }

        if let Some(d) = self.drag.as_ref().filter(|d| d.active)
            && let Some(card) = self.cards.iter().find(|c| c.id == d.card)
        {
            // Offset from the pointer so the lists underneath still see it.
            layers = layers.push(
                pin(container(text(&card.title).size(13))
                    .padding(p.u * 1.25)
                    .width(240)
                    .style(style::dialog(p)))
                .position(iced::Point::new(d.pos.x + 14.0, d.pos.y + 10.0)),
            );
        }

        if let Some((msg, error, _)) = &self.toast {
            let tint = if *error { p.danger } else { p.success };
            layers = layers.push(bottom_center(
                container(
                    container(text(msg).size(13))
                        .padding([8.0, 14.0])
                        .max_width(640)
                        .style(move |_| container::Style {
                            background: Some(style::mix(p.surface3, tint, 0.25).into()),
                            border: iced::Border {
                                color: alpha(tint, 0.7),
                                width: 1.0,
                                radius: p.radius.into(),
                            },
                            text_color: Some(p.text),
                            ..Default::default()
                        }),
                )
                .padding(p.u * 3.0),
            ));
        }
        layers.into()
    }

    fn sidebar(&self, p: P) -> El<'_> {
        let rail = self.rail();
        let mut nav = column![].spacing(2);
        for v in View::ALL {
            let on = self.view == v;
            let fg = if on { p.text } else { p.dim };
            let glyph = icon(v.icon(), 17.0, if on { p.accent } else { p.dim });
            let label: El = if rail {
                glyph
            } else {
                row![glyph, text(v.label()).size(14).color(fg)]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into()
            };
            nav = nav.push(
                button(label)
                    .width(Fill)
                    .padding([8.0, 10.0])
                    .style(btn(p, if on { Btn::Selected } else { Btn::Ghost }))
                    .on_press(Message::Go(v)),
            );
        }

        let phase = self.timer.phase();
        let tint = p.phase(phase);
        let (time, running) = match &self.timer.active {
            Some(a) => (
                dates::clock(if a.open_ended {
                    a.elapsed_s()
                } else {
                    a.remaining_s()
                }),
                !a.paused,
            ),
            None => (dates::clock(self.timer.planned_for(phase)), false),
        };
        let state = match &self.timer.active {
            Some(a) if a.paused => "Paused",
            Some(_) => phase.label(),
            None if phase == Phase::Focus => "Ready",
            None => "Break next",
        };
        let mini = container(
            row![
                column![
                    text(state).size(11).color(tint),
                    text(time.clone()).size(22)
                ]
                .spacing(1),
                space::horizontal(),
                button(icon(
                    if running { "pause" } else { "play" },
                    16.0,
                    p.on_accent
                ))
                .padding(9)
                .style(btn(p, Btn::Primary))
                .on_press(Message::TimerToggle),
            ]
            .align_y(Alignment::Center),
        )
        .padding(p.u * 1.5)
        .style(style::panel(p, alpha(tint, 0.14)));

        let goal = self.cfg.timer.daily_goal;
        let done = self.completed_today();
        let progress: El = if goal > 0 {
            dots(p, done.min(goal), goal, tint)
        } else {
            Space::new().into()
        };

        if rail {
            let toggle = button(icon(
                if running { "pause" } else { "play" },
                16.0,
                p.on_accent,
            ))
            .padding(9)
            .style(btn(p, Btn::Primary))
            .on_press(Message::TimerToggle);
            return container(
                column![
                    container(icon("timer", 20.0, p.accent)).padding([2.0, 8.0]),
                    Space::new().height(p.u),
                    nav,
                    space::vertical(),
                    text(time).size(11).color(tint),
                    toggle,
                ]
                .spacing(p.u)
                .align_x(Alignment::Center),
            )
            .padding([p.u * 2.0, p.u])
            .width(60)
            .height(Fill)
            .style(style::flat(p.surface))
            .into();
        }

        container(
            column![
                row![
                    icon("timer", 20.0, p.accent),
                    text(crate::config::APP_TITLE).size(17)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                Space::new().height(p.u),
                nav,
                space::vertical(),
                progress,
                mini,
                text("Ctrl+K for commands")
                    .size(11)
                    .color(alpha(p.dim, 0.8)),
            ]
            .spacing(p.u),
        )
        .padding(p.u * 2.0)
        .width(212)
        .height(Fill)
        .style(style::flat(p.surface))
        .into()
    }

    fn view_modal(&self, p: P) -> Option<El<'_>> {
        let (width, body): (f32, El) = match &self.modal {
            Modal::None => return None,
            Modal::Card(c) => (520.0, self.card_editor(p, c)),
            Modal::Item(i) => (460.0, self.item_editor(p, i)),
            Modal::Habit(h) => (480.0, self.habit_editor(p, h)),
            Modal::Calendar(c) => (420.0, self.calendar_editor(p, c)),
            Modal::Keys => (560.0, self.keys_help(p)),
            Modal::Prompt { title, value, .. } => (
                380.0,
                column![
                    text(*title).size(17),
                    field(p, "Name", value, Field::Prompt).id("prompt"),
                    row![
                        space::horizontal(),
                        action(p, Btn::Ghost, None, "Cancel", Message::CloseModal),
                        action(
                            p,
                            Btn::Primary,
                            None,
                            "Save",
                            Message::Submit(Field::Prompt)
                        ),
                    ]
                    .spacing(8),
                ]
                .spacing(p.u * 1.5)
                .into(),
            ),
            Modal::Confirm {
                text: msg,
                action: what,
            } => {
                let label = match what {
                    ConfirmAction::ImportData => "Replace everything",
                    ConfirmAction::WipeData => "Delete everything",
                    _ => "Delete",
                };
                (
                    400.0,
                    column![
                        text(msg).size(14),
                        row![
                            space::horizontal(),
                            action(p, Btn::Ghost, None, "Cancel", Message::CloseModal),
                            action(p, Btn::Tint(p.danger), None, label, Message::ConfirmYes),
                        ]
                        .spacing(8),
                    ]
                    .spacing(p.u * 2.0)
                    .into(),
                )
            }
            Modal::Palette { query } => {
                let mut list = column![].spacing(2);
                for (i, (label, act)) in self.palette_actions(query).into_iter().enumerate() {
                    let name = match &act {
                        Action::Go(v) => v.icon(),
                        Action::TimerToggle | Action::StartCard(_) => "play",
                        Action::TimerSkip => "skip",
                        Action::Zen => "zen",
                        Action::Intake(_) => "plus",
                        Action::Theme(_) => "moon",
                    };
                    list = list.push(
                        button(
                            row![icon(name, 15.0, p.dim), text(label).size(13)]
                                .spacing(10)
                                .align_y(Alignment::Center),
                        )
                        .width(Fill)
                        .padding([7.0, 10.0])
                        .style(btn(p, if i == 0 { Btn::Selected } else { Btn::Ghost }))
                        .on_press(Message::Run(act)),
                    );
                }
                (
                    520.0,
                    column![
                        field(
                            p,
                            "A command, or something to add: Dentist fri 2pm, note: …",
                            query,
                            Field::PaletteQuery
                        )
                        .id("palette")
                        .size(15)
                        .padding(10),
                        list,
                    ]
                    .spacing(p.u)
                    .into(),
                )
            }
        };
        // Clicks inside the dialog must not reach the backdrop.
        Some(opaque(
            container(body)
                .padding(p.u * 2.5)
                .width(Length::Fixed(width))
                .style(style::dialog(p)),
        ))
    }
}

/// `done` filled dots out of `total`.
pub fn dots<'a>(p: P, done: u32, total: u32, color: Color) -> El<'a> {
    let mut r = row![].spacing(4);
    for i in 0..total.min(16) {
        let fill = if i < done { color } else { p.surface3 };
        r = r.push(
            container(Space::new().width(8).height(8)).style(move |_| container::Style {
                background: Some(fill.into()),
                border: iced::Border {
                    radius: 99.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }
    r.into()
}

/// A horizontal meter: label, value text, and a bar of `frac` (0..=1).
pub fn meter<'a>(
    p: P,
    label: impl Into<String>,
    value: impl Into<String>,
    frac: f32,
    color: Color,
) -> El<'a> {
    let filled = (frac.clamp(0.0, 1.0) * 1000.0).round() as u16;
    let bar: El = row![
        container(Space::new().height(6))
            .width(Length::FillPortion(filled.max(1)))
            .style(style::panel(
                p,
                if filled == 0 {
                    Color::TRANSPARENT
                } else {
                    color
                }
            )),
        Space::new().width(Length::FillPortion((1000 - filled).max(1))),
    ]
    .into();
    column![
        row![
            text(label.into()).size(13).wrapping(text::Wrapping::None),
            space::horizontal(),
            text(value.into()).size(12).color(p.dim)
        ]
        .spacing(8),
        container(bar)
            .width(Fill)
            .style(style::panel(p, p.surface3)),
    ]
    .spacing(4)
    .into()
}

pub fn empty<'a>(p: P, msg: &'a str) -> Element<'a, Message> {
    container(text(msg).size(13).color(p.dim))
        .padding([p.u, 0.0])
        .into()
}
