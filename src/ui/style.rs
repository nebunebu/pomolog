//! The palette in the toolkit's colour type, plus the widget styles and
//! small building blocks every view shares.

use crate::theme::{Rgb, Theme as Look};
use iced::widget::{button, container, scrollable, svg, text, text_input};
use iced::{Background, Border, Color, Element, Length, Shadow, Vector};
use std::collections::HashMap;
use std::sync::OnceLock;

pub fn col(c: Rgb) -> Color {
    Color {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// Everything a view needs to draw, cheap to copy into style closures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct P {
    pub bg: Color,
    pub surface: Color,
    pub surface2: Color,
    pub surface3: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub on_accent: Color,
    pub focus: Color,
    pub short_break: Color,
    pub long_break: Color,
    pub holiday: Color,
    pub appointment: Color,
    pub deadline: Color,
    pub habit: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub outline: Color,
    pub radius: f32,
    /// Base spacing unit: 8 normally, 6 when compact.
    pub u: f32,
    pub dark: bool,
}

impl P {
    pub fn new(look: &Look) -> P {
        let c = &look.c;
        P {
            bg: col(c.bg),
            surface: col(c.surface),
            surface2: col(c.surface2),
            surface3: col(c.surface3),
            text: col(c.text),
            dim: col(c.text_dim),
            accent: col(c.accent),
            on_accent: col(c.on_accent),
            focus: col(c.focus),
            short_break: col(c.short_break),
            long_break: col(c.long_break),
            holiday: col(c.holiday),
            appointment: col(c.appointment),
            deadline: col(c.deadline),
            habit: col(c.habit),
            success: col(c.success),
            warning: col(c.warning),
            danger: col(c.danger),
            outline: col(c.outline),
            radius: look.radius,
            u: if look.compact { 6.0 } else { 8.0 },
            dark: look.dark,
        }
    }

    pub fn phase(&self, phase: crate::core::model::Phase) -> Color {
        use crate::core::model::Phase::*;
        match phase {
            Focus => self.focus,
            ShortBreak => self.short_break,
            LongBreak => self.long_break,
        }
    }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// The toolkit theme, used by widgets that are not styled by hand.
pub fn iced_theme(look: &Look) -> iced::Theme {
    let c = &look.c;
    iced::Theme::custom(
        look.name.clone(),
        iced::theme::Palette {
            background: col(c.bg),
            text: col(c.text),
            primary: col(c.accent),
            success: col(c.success),
            warning: col(c.warning),
            danger: col(c.danger),
        },
    )
}

// ---- containers ----

pub fn card(p: P) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.surface)),
        border: Border {
            color: alpha(p.outline, 0.6),
            width: 1.0,
            radius: p.radius.into(),
        },
        text_color: Some(p.text),
        ..Default::default()
    }
}

pub fn panel(p: P, fill: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(fill)),
        border: Border {
            radius: p.radius.into(),
            ..Default::default()
        },
        text_color: Some(p.text),
        ..Default::default()
    }
}

pub fn flat(fill: Color) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(fill)),
        ..Default::default()
    }
}

pub fn dialog(p: P) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(p.surface)),
        border: Border {
            color: p.outline,
            width: 1.0,
            radius: (p.radius + 4.0).into(),
        },
        text_color: Some(p.text),
        shadow: Shadow {
            color: Color {
                a: 0.45,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 12.0),
            blur_radius: 40.0,
        },
        ..Default::default()
    }
}

/// A small rounded label tinted with `c`.
pub fn chip<'a, M: 'a>(p: P, label: impl text::IntoFragment<'a>, c: Color) -> Element<'a, M> {
    container(text(label).size(11).color(mix(c, p.text, 0.25)))
        .padding([2.0, 7.0])
        .style(move |_| container::Style {
            background: Some(Background::Color(alpha(c, 0.18))),
            border: Border {
                radius: 99.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

// ---- buttons ----

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Btn {
    /// Filled with the accent.
    Primary,
    /// Subtle filled surface.
    Soft,
    /// No background until hovered.
    Ghost,
    /// Ghost that turns red on hover.
    Danger,
    /// Filled with a specific colour.
    Tint(Color),
    /// Solid fill in a specific colour.
    Fill(Color),
    /// Empty with a border, like an unticked box.
    Outline,
    /// Ghost showing an "on" state.
    Selected,
}

pub fn btn(p: P, kind: Btn) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let hover = matches!(status, button::Status::Hovered);
        let pressed = matches!(status, button::Status::Pressed);
        let disabled = matches!(status, button::Status::Disabled);
        let (bg, fg) = match kind {
            Btn::Primary => {
                let base = p.accent;
                let bg = if pressed {
                    mix(base, p.bg, 0.2)
                } else if hover {
                    mix(base, p.text, 0.12)
                } else {
                    base
                };
                (Some(bg), p.on_accent)
            }
            Btn::Tint(c) => {
                let bg = if pressed {
                    0.38
                } else if hover {
                    0.30
                } else {
                    0.20
                };
                (Some(alpha(c, bg)), mix(c, p.text, 0.3))
            }
            Btn::Fill(c) => (Some(if hover { mix(c, p.text, 0.15) } else { c }), p.bg),
            Btn::Soft => {
                let bg = if pressed {
                    p.surface3
                } else if hover {
                    mix(p.surface2, p.surface3, 0.6)
                } else {
                    p.surface2
                };
                (Some(bg), p.text)
            }
            Btn::Ghost => {
                let bg = if pressed {
                    Some(p.surface3)
                } else if hover {
                    Some(p.surface2)
                } else {
                    None
                };
                (bg, p.text)
            }
            Btn::Danger => {
                if hover || pressed {
                    (
                        Some(alpha(p.danger, if pressed { 0.32 } else { 0.2 })),
                        p.danger,
                    )
                } else {
                    (None, p.dim)
                }
            }
            Btn::Outline => (
                Some(if hover || pressed { p.surface3 } else { p.bg }),
                p.text,
            ),
            Btn::Selected => (
                Some(alpha(p.accent, if hover { 0.26 } else { 0.18 })),
                p.text,
            ),
        };
        let outline = kind == Btn::Outline;
        button::Style {
            background: bg.map(|c| Background::Color(if disabled { alpha(c, 0.4) } else { c })),
            text_color: if disabled { alpha(fg, 0.45) } else { fg },
            border: Border {
                radius: (p.radius * 0.8).into(),
                color: if outline {
                    mix(p.outline, p.text, 0.25)
                } else {
                    Color::TRANSPARENT
                },
                width: if outline { 1.5 } else { 0.0 },
            },
            ..Default::default()
        }
    }
}

// ---- inputs ----

pub fn input(p: P) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    move |_, status| {
        let focused = matches!(status, text_input::Status::Focused { .. });
        text_input::Style {
            background: Background::Color(p.bg),
            border: Border {
                color: if focused { p.accent } else { p.outline },
                width: 1.0,
                radius: (p.radius * 0.7).into(),
            },
            icon: p.dim,
            placeholder: alpha(p.dim, 0.7),
            value: p.text,
            selection: alpha(p.accent, 0.35),
        }
    }
}

/// Like `input`, with a red border when `bad`.
pub fn input_checked(
    p: P,
    bad: bool,
) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    move |t, status| {
        let mut s = input(p)(t, status);
        if bad {
            s.border.color = p.danger;
        }
        s
    }
}

pub fn scroll(p: P) -> impl Fn(&iced::Theme, scrollable::Status) -> scrollable::Style {
    move |theme, status| {
        let mut s = scrollable::default(theme, status);
        let rail = |active: bool| scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: Background::Color(alpha(p.dim, if active { 0.6 } else { 0.25 })),
                border: Border {
                    radius: 99.0.into(),
                    ..Default::default()
                },
            },
        };
        let (v, h) = match status {
            scrollable::Status::Hovered {
                is_vertical_scrollbar_hovered: v,
                is_horizontal_scrollbar_hovered: h,
                ..
            } => (v, h),
            scrollable::Status::Dragged {
                is_vertical_scrollbar_dragged: v,
                is_horizontal_scrollbar_dragged: h,
                ..
            } => (v, h),
            _ => (false, false),
        };
        s.vertical_rail = rail(v);
        s.horizontal_rail = rail(h);
        s.container = container::Style::default();
        s
    }
}

// ---- text ----

pub fn h1<'a, M: 'a>(s: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(s).size(24).into()
}

pub fn h2<'a, M: 'a>(p: P, s: impl text::IntoFragment<'a>) -> Element<'a, M> {
    text(s).size(13).color(p.dim).into()
}

pub fn dim<'a>(p: P, s: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(s).size(13).color(p.dim)
}

// ---- icons ----

const ICONS: &[(&str, &str)] = &[
    (
        "today",
        r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>"#,
    ),
    (
        "timer",
        r#"<circle cx="12" cy="13" r="8"/><path d="M12 9v4l2.5 2M9.5 2.5h5"/>"#,
    ),
    (
        "board",
        r#"<rect x="3" y="4" width="5" height="16" rx="1.5"/><rect x="9.5" y="4" width="5" height="10" rx="1.5"/><rect x="16" y="4" width="5" height="13" rx="1.5"/>"#,
    ),
    (
        "calendar",
        r#"<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/>"#,
    ),
    (
        "habits",
        r#"<path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/>"#,
    ),
    (
        "analytics",
        r#"<path d="M5 20V11M11 20V4M17 20v-7M2.5 20h19"/>"#,
    ),
    (
        "settings",
        r#"<path d="M4 6h8M18 6h2M4 12h2M12 12h8M4 18h10M20 18h0"/><circle cx="15" cy="6" r="2.2"/><circle cx="9" cy="12" r="2.2"/><circle cx="17" cy="18" r="2.2"/>"#,
    ),
    ("play", r##"<path d="M8 5v14l11-7z" fill="#fff"/>"##),
    (
        "pause",
        r#"<path d="M8.5 5v14M15.5 5v14" stroke-width="3"/>"#,
    ),
    (
        "skip",
        r##"<path d="M5 5v14l10-7z" fill="#fff"/><path d="M19 5v14"/>"##,
    ),
    (
        "stop",
        r##"<rect x="6" y="6" width="12" height="12" rx="2" fill="#fff"/>"##,
    ),
    ("plus", r#"<path d="M12 5v14M5 12h14"/>"#),
    ("x", r#"<path d="M6 6l12 12M18 6L6 18"/>"#),
    ("check", r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#),
    ("trash", r#"<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13"/>"#),
    ("left", r#"<path d="M15 5l-7 7 7 7"/>"#),
    ("right", r#"<path d="M9 5l7 7-7 7"/>"#),
    ("flag", r#"<path d="M5 21V4M5 4h13l-2.5 4 2.5 4H5"/>"#),
    (
        "search",
        r#"<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>"#,
    ),
    (
        "flame",
        r#"<path d="M12 3c1 3.5 5 5.5 5 10a5 5 0 0 1-10 0c0-1.8.7-3 1.8-4 .4 1.4 1.2 2 2 2C10.300 8.500 11 5.500 12 3z"/>"#,
    ),
    (
        "zen",
        r#"<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>"#,
    ),
    (
        "star",
        r#"<path d="M12 3l2.7 5.6 6.1.8-4.5 4.3 1.1 6.1L12 16.9 6.6 19.800l1.1-6.1L3.200 9.400l6.1-.8z"/>"#,
    ),
    ("edit", r#"<path d="M4 20l1-4L16 5l3 3L8 19zM14 7l3 3"/>"#),
    (
        "clock",
        r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>"#,
    ),
    (
        "note",
        r#"<path d="M6 3h9l4 4v14H6zM15 3v4h4M9 12h7M9 16h5"/>"#,
    ),
    (
        "moon",
        r#"<path d="M20 14.500A8 8 0 0 1 9.500 4a8 8 0 1 0 10.500 10.500z"/>"#,
    ),
];

fn handles() -> &'static HashMap<&'static str, svg::Handle> {
    static H: OnceLock<HashMap<&'static str, svg::Handle>> = OnceLock::new();
    H.get_or_init(|| {
        ICONS
            .iter()
            .map(|(name, body)| {
                let doc = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##
                );
                (*name, svg::Handle::from_memory(doc.into_bytes()))
            })
            .collect()
    })
}

pub fn icon<'a, M: 'a>(name: &str, size: f32, color: Color) -> Element<'a, M> {
    match handles().get(name) {
        Some(h) => svg(h.clone())
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .style(move |_, _| svg::Style { color: Some(color) })
            .into(),
        None => text("?").size(size).into(),
    }
}
