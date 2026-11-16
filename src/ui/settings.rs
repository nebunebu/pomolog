//! Settings: timer, appearance and themes, notifications, data.

use super::app::{App, ConfirmAction, El, Field, Message, Modal, Toggle};
use super::shell::{action, field, icon_btn, labelled, page, section, vscroll};
use super::style::{self, Btn, P, btn};
use crate::config::{self, Mode, Source};
use crate::core::export;
use crate::theme::{Colors, Rgb};
use iced::widget::{
    Space, button, column, container, pick_list, row, slider, space, text, toggler,
};
use iced::{Alignment, Border, Fill};

impl App {
    pub(super) fn toggle(&mut self, t: Toggle, on: bool) {
        match t {
            Toggle::AutoBreaks => self.cfg.timer.auto_start_breaks = on,
            Toggle::AutoFocus => self.cfg.timer.auto_start_focus = on,
            Toggle::Flow => self.cfg.timer.flow = on,
            Toggle::Compact => self.cfg.appearance.compact = on,
            Toggle::Notifications => self.cfg.notifications.enabled = on,
            Toggle::DailySummary => self.cfg.notifications.daily_summary = on,
            Toggle::HabitRest | Toggle::HabitArchived | Toggle::HabitWeekday(_) => {
                if let Modal::Habit(h) = &mut self.modal {
                    match t {
                        Toggle::HabitRest => h.rest = on,
                        Toggle::HabitArchived => h.archived = on,
                        Toggle::HabitWeekday(i) => h.weekdays[i.min(6)] = on,
                        _ => {}
                    }
                }
                return;
            }
            Toggle::ItemDone => {
                if let Modal::Item(i) = &mut self.modal {
                    i.done = on;
                }
                return;
            }
            Toggle::CalWeek => {
                self.cal_week = on;
                return;
            }
            Toggle::CalFilter(i) => {
                self.cal_filters[i.min(3)] = on;
                return;
            }
        }
        // Only the configuration toggles reach this point.
        self.timer.cfg = self.cfg.timer.to_cfg();
        self.restyle_and_save();
    }

    fn restyle_and_save(&mut self) {
        self.restyle();
        self.save_cfg();
    }

    pub(super) fn settings_field(&mut self, f: Field, v: String) {
        let minutes = |s: &str| {
            s.trim()
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|m| *m > 0.0 && *m <= 600.0)
        };
        let whole = |s: &str, max: u32| s.trim().parse::<u32>().ok().filter(|n| *n <= max);
        let mut changed = true;
        match f {
            Field::SetFocus => {
                if let Some(m) = minutes(&v) {
                    self.cfg.timer.focus_min = m;
                }
                self.set.focus = v;
            }
            Field::SetShort => {
                if let Some(m) = minutes(&v) {
                    self.cfg.timer.short_break_min = m;
                }
                self.set.short = v;
            }
            Field::SetLong => {
                if let Some(m) = minutes(&v) {
                    self.cfg.timer.long_break_min = m;
                }
                self.set.long = v;
            }
            Field::SetGoal => {
                if let Some(n) = whole(&v, 48) {
                    self.cfg.timer.daily_goal = n;
                }
                self.set.goal = v;
            }
            Field::SetRatio => {
                if let Some(n) = whole(&v, 100).filter(|n| *n >= 5) {
                    self.cfg.timer.flow_break_ratio = n as f64 / 100.0;
                }
                self.set.ratio = v;
            }
            Field::SetFont => {
                self.cfg.appearance.font = v.trim().to_string();
                self.set.font = v;
            }
            Field::SetLead => {
                if let Some(n) = whole(&v, 600) {
                    self.cfg.notifications.appointment_lead_min = n;
                }
                self.set.lead = v;
            }
            Field::SetHabitHour => {
                if let Some(n) = whole(&v, 24) {
                    self.cfg.notifications.habit_reminder_hour = n;
                }
                self.set.habit_hour = v;
            }
            Field::Override(key) => {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    self.cfg.overrides.remove(key);
                } else if Rgb::parse(trimmed).is_some() {
                    self.cfg
                        .overrides
                        .insert(key.to_string(), trimmed.to_string());
                } else {
                    changed = false;
                }
                self.set.overrides.insert(key.to_string(), v);
            }
            _ => changed = false,
        }
        if changed {
            self.timer.cfg = self.cfg.timer.to_cfg();
            self.restyle_and_save();
            self.reload_analytics();
        }
    }

    pub(super) fn update_settings(&mut self, m: Message) {
        match m {
            Message::SetTheme(name) => self.cfg.appearance.theme = name,
            Message::SetMode(mode) => self.cfg.appearance.mode = mode,
            Message::SetSource(source) => self.cfg.appearance.source = source,
            Message::SetRadius(r) => self.cfg.appearance.radius = Some(r),
            Message::SetScale(s) => self.cfg.appearance.scale = (s * 20.0).round() / 20.0,
            Message::SetEveryN(n) => {
                self.cfg.timer.long_break_every = n;
                self.timer.cfg = self.cfg.timer.to_cfg();
            }
            Message::OverrideClear(key) => {
                self.cfg.overrides.remove(key);
                self.set.overrides.remove(key);
            }
            Message::ThemeSaveAs => return self.theme_save_as(),
            Message::Export => return self.export_data(),
            Message::Import => {
                if self.set.import_path.trim().is_empty() {
                    self.toast("Enter the path of an export file first.", true);
                } else {
                    self.modal = Modal::Confirm {
                        text: "Importing replaces everything currently in Pomolog. Continue?"
                            .into(),
                        action: ConfirmAction::ImportData,
                    };
                }
                return;
            }
            _ => return,
        }
        self.restyle_and_save();
    }

    /// Writes the look currently on screen as a standalone theme file and
    /// switches to it.
    fn theme_save_as(&mut self) {
        let name: String = self
            .set
            .theme_name
            .trim()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let name = name.trim_matches('-').to_string();
        if name.is_empty() || name == "system" {
            self.toast("Give the theme a name (letters, digits, dashes).", true);
            return;
        }
        let mut out = format!(
            "name = \"{name}\"\nmode = \"{}\"\nradius = {}\n\n[colors]\n",
            if self.look.dark { "dark" } else { "light" },
            self.look.radius
        );
        for key in Colors::KEYS {
            if let Some(c) = self.look.c.get(key) {
                out.push_str(&format!("{key} = \"{}\"\n", c.to_hex()));
            }
        }
        let dir = config::themes_dir();
        let path = dir.join(format!("{name}.toml"));
        let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, out));
        match written {
            Ok(()) => {
                // The overrides are now part of the file.
                self.cfg.overrides.clear();
                self.set.overrides.clear();
                self.cfg.appearance.theme = name;
                self.set.theme_name.clear();
                self.restyle_and_save();
                self.toast(format!("Saved {}", path.display()), false);
            }
            Err(e) => self.toast(format!("Could not write {}: {e}", path.display()), true),
        }
    }

    fn export_data(&mut self) {
        let path = Self::expand_path(&self.set.export_path);
        let res = export::export(&self.db);
        let Some(data) = self.check(res) else { return };
        let text = serde_json::to_string_pretty(&data).unwrap_or_default();
        match std::fs::write(&path, text) {
            Ok(()) => self.toast(format!("Exported to {}", path.display()), false),
            Err(e) => self.toast(format!("Could not write {}: {e}", path.display()), true),
        }
    }

    pub(super) fn import_data(&mut self) {
        let path = Self::expand_path(&self.set.import_path);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return self.toast(format!("Could not read {}: {e}", path.display()), true),
        };
        let data = match serde_json::from_str(&text) {
            Ok(d) => d,
            Err(e) => {
                return self.toast(format!("{} is not valid JSON: {e}", path.display()), true);
            }
        };
        match export::import(&mut self.db, &data) {
            Ok(n) => {
                self.timer = crate::core::timer::Timer::new(self.cfg.timer.to_cfg());
                self.board_id = None;
                self.reload_all();
                self.toast(format!("Imported {n} rows."), false);
            }
            Err(e) => self.toast(format!("Nothing was imported: {e}"), true),
        }
    }

    pub fn view_settings(&self, p: P) -> El<'_> {
        let t = &self.cfg.timer;
        let a = &self.cfg.appearance;
        let n = &self.cfg.notifications;
        let s = &self.set;
        let switch = |label: &'static str, on: bool, which: Toggle| -> El<'_> {
            row![
                text(label).size(13),
                space::horizontal(),
                toggler(on)
                    .on_toggle(move |v| Message::Toggle(which, v))
                    .size(18)
            ]
            .align_y(Alignment::Center)
            .into()
        };
        let num = |label: &'static str, value: &str, f: Field| -> El<'_> {
            labelled(p, label, field(p, "", value, f).width(110))
        };

        let mut every = row![].spacing(4).align_y(Alignment::Center);
        for k in [0u32, 2, 3, 4, 5, 6] {
            every = every.push(
                button(
                    text(if k == 0 {
                        "Never".to_string()
                    } else {
                        k.to_string()
                    })
                    .size(12),
                )
                .padding([5.0, 10.0])
                .style(btn(
                    p,
                    if t.long_break_every == k {
                        Btn::Selected
                    } else {
                        Btn::Ghost
                    },
                ))
                .on_press(Message::SetEveryN(k)),
            );
        }

        let mut timer = column![
            row![
                num("FOCUS (MIN)", &s.focus, Field::SetFocus),
                num("SHORT BREAK (MIN)", &s.short, Field::SetShort),
                num("LONG BREAK (MIN)", &s.long, Field::SetLong),
                num("DAILY GOAL (SESSIONS)", &s.goal, Field::SetGoal),
            ]
            .spacing(p.u * 1.5)
            .wrap()
            .vertical_spacing(p.u),
            labelled(p, "LONG BREAK AFTER THIS MANY FOCUS SESSIONS", every),
            switch(
                "Start breaks automatically",
                t.auto_start_breaks,
                Toggle::AutoBreaks
            ),
            switch(
                "Start the next focus session automatically",
                t.auto_start_focus,
                Toggle::AutoFocus
            ),
            switch(
                "Flow mode: count up until you stop, then take a proportional break",
                t.flow,
                Toggle::Flow
            ),
        ]
        .spacing(p.u * 1.5);
        if t.flow {
            timer = timer.push(num("BREAK AS % OF TIME WORKED", &s.ratio, Field::SetRatio));
        }

        // ---- appearance ----
        let mut look = column![
            row![
                labelled(
                    p,
                    "THEME",
                    pick_list(s.themes.clone(), Some(a.theme.clone()), Message::SetTheme).width(180).text_size(13)
                ),
                labelled(p, "MODE", pick_list(&Mode::ALL[..], Some(a.mode), Message::SetMode).width(150).text_size(13)),
                labelled(
                    p,
                    "SYSTEM COLOURS FROM",
                    pick_list(&Source::ALL[..], Some(a.source), Message::SetSource).width(150).text_size(13)
                ),
            ]
            .spacing(p.u * 1.5)
            .wrap()
            .vertical_spacing(p.u),
            style::dim(
                p,
                format!(
                    "Showing \"{}\" ({}). The system theme follows your GTK or Qt colours and updates when they change.",
                    self.look.name, self.look.origin
                )
            )
            .size(12),
        ]
        .spacing(p.u * 1.5);
        for problem in &self.look.problems {
            look = look.push(text(problem).size(12).color(p.danger));
        }
        look = look
            .push(
                row![
                    labelled(
                        p,
                        "FONT (APPLIES AFTER RESTART)",
                        field(p, "System font", &s.font, Field::SetFont).width(220)
                    ),
                    labelled(
                        p,
                        "CORNER RADIUS",
                        row![
                            slider(0.0..=20.0, self.look.radius, Message::SetRadius).width(160),
                            text(format!("{:.0}", self.look.radius))
                                .size(12)
                                .color(p.dim)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    ),
                    labelled(
                        p,
                        "INTERFACE SIZE",
                        row![
                            slider(0.7..=1.6, a.scale, Message::SetScale)
                                .step(0.05_f32)
                                .width(160),
                            text(format!("{:.0}%", a.scale * 100.0))
                                .size(12)
                                .color(p.dim)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    ),
                ]
                .spacing(p.u * 2.0)
                .align_y(Alignment::End)
                .wrap()
                .vertical_spacing(p.u),
            )
            .push(switch("Compact spacing", a.compact, Toggle::Compact));

        // Colour overrides: every palette colour, editable.
        let mut grid = column![].spacing(6);
        for pair in Colors::KEYS.chunks(if self.wide() { 2 } else { 1 }) {
            let mut r = row![].spacing(p.u * 2.0);
            for &key in pair {
                let current = self.look.c.get(key).unwrap_or(Rgb::BLACK);
                let typed = s.overrides.get(key).cloned().unwrap_or_default();
                let overridden = self.cfg.overrides.contains_key(key);
                let bad = !typed.trim().is_empty() && Rgb::parse(&typed).is_none();
                let swatch =
                    container(Space::new().width(22).height(22)).style(move |_| container::Style {
                        background: Some(style::col(current).into()),
                        border: Border {
                            color: p.outline,
                            width: 1.0,
                            radius: 5.0.into(),
                        },
                        ..Default::default()
                    });
                let mut line = row![
                    swatch,
                    text(key)
                        .size(12)
                        .width(92)
                        .color(if overridden { p.text } else { p.dim }),
                    field(p, &current.to_hex(), &typed, Field::Override(key))
                        .width(104)
                        .style(style::input_checked(p, bad)),
                ]
                .spacing(8)
                .align_y(Alignment::Center);
                line = line.push(if overridden {
                    icon_btn(p, Btn::Ghost, "x", Message::OverrideClear(key))
                } else {
                    Space::new().width(27).into()
                });
                r = r.push(line);
            }
            grid = grid.push(r);
        }
        look = look
            .push(labelled(
                p,
                "COLOUR OVERRIDES (HEX, ON TOP OF THE THEME)",
                grid,
            ))
            .push(
                row![
                    field(p, "my-theme", &s.theme_name, Field::SetThemeName).width(180),
                    action(
                        p,
                        Btn::Soft,
                        None,
                        "Save this look as a theme file",
                        Message::ThemeSaveAs
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .push(
                style::dim(
                    p,
                    format!("Theme files live in {}", config::themes_dir().display()),
                )
                .size(11),
            );

        let notes = column![
            switch("Desktop notifications", n.enabled, Toggle::Notifications),
            switch(
                "Summarise today's items once a day",
                n.daily_summary,
                Toggle::DailySummary
            ),
            row![
                num(
                    "WARN BEFORE APPOINTMENTS (MIN, 0 = OFF)",
                    &s.lead,
                    Field::SetLead
                ),
                num(
                    "HABIT REMINDER HOUR (24 = OFF)",
                    &s.habit_hour,
                    Field::SetHabitHour
                ),
            ]
            .spacing(p.u * 1.5)
            .wrap()
            .vertical_spacing(p.u),
        ]
        .spacing(p.u * 1.5);

        let data = column![
            row![
                field(
                    p,
                    "~/pomolog-export.json",
                    &s.export_path,
                    Field::ExportPath
                ),
                action(p, Btn::Soft, None, "Export everything", Message::Export),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                field(
                    p,
                    "Path to an export file",
                    &s.import_path,
                    Field::ImportPath
                ),
                action(
                    p,
                    Btn::Soft,
                    None,
                    "Import (replaces all data)",
                    Message::Import
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                style::dim(p, format!("Database: {}", config::db_path().display())).size(11),
                space::horizontal(),
                action(
                    p,
                    Btn::Danger,
                    Some("trash"),
                    "Delete all data",
                    Message::Ask(ConfirmAction::WipeData)
                ),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(p.u * 1.5);

        let help = column![
            style::dim(p, "Space starts and pauses · Ctrl+K opens commands · Ctrl+1…7 switches view · Esc closes").size(12),
            style::dim(p, "From a terminal or keybind: pomolog toggle | skip | abandon | show | status --json | add \"Task\" --due fri")
                .size(12),
        ]
        .spacing(4);

        let mut body = column![].spacing(p.u * 1.5).width(Fill);
        if let Some(e) = &s.config_error {
            body = body.push(text(e).size(13).color(p.danger));
        }
        body = body
            .push(section(p, "TIMER", timer))
            .push(section(p, "APPEARANCE", look))
            .push(section(p, "NOTIFICATIONS", notes))
            .push(section(p, "DATA", data))
            .push(section(p, "SHORTCUTS", help));
        page(p, true, "Settings", None, vscroll(p, body))
    }
}
