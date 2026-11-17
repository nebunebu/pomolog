//! Month and week calendar. Holidays, appointments, deadlines and habits are
//! four different things and are drawn four different ways.

use super::app::{App, CalendarEdit, ConfirmAction, El, Field, ItemEdit, Message, Modal, Toggle};
use super::shell::{action, empty, field, icon_btn, labelled, page, vscroll};
use super::style::{self, Btn, P, alpha, btn, chip, icon, mix};
use crate::core::habits::{self, DayState};
use crate::core::model::*;
use crate::core::{dates, ics, tz};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use iced::widget::{
    Space, button, checkbox, column, container, pick_list, row, space, text, text_editor,
};
use iced::{Alignment, Border, Color, Fill, Length, Task};

const FILTERS: [(&str, &str); 5] = [
    ("Holidays", "star"),
    ("Appointments", "clock"),
    ("Deadlines", "flag"),
    ("Habits", "habits"),
    ("Notes", "note"),
];

/// Colours a calendar can be given.
pub const SWATCHES: [&str; 8] = [
    "#e5484d", "#f08c3a", "#e2b93d", "#46a758", "#3ba7a0", "#4a8fe7", "#8e6fe0", "#d6589f",
];

fn kind_icon(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Holiday => "star",
        ItemKind::Appointment => "clock",
        ItemKind::Deadline => "flag",
        ItemKind::Note => "note",
    }
}

/// A distinct colour per habit, so stacked chain lines can be told apart.
pub fn habit_color(p: P, index: usize) -> Color {
    let base = crate::theme::Rgb {
        r: p.habit.r,
        g: p.habit.g,
        b: p.habit.b,
        a: p.habit.a,
    };
    style::col(base.rotate(index as f32 * 47.0))
}

impl App {
    fn kind_color(&self, p: P, kind: ItemKind) -> Color {
        match kind {
            ItemKind::Holiday => p.holiday,
            ItemKind::Appointment => p.appointment,
            ItemKind::Deadline => p.deadline,
            ItemKind::Note => p.dim,
        }
    }

    fn calendar_color(&self, id: Id) -> Option<Color> {
        let c = self.calendars.iter().find(|c| c.id == id)?;
        crate::theme::Rgb::parse(c.color.as_deref()?).map(style::col)
    }

    /// The colour of its calendar if that has one, else the colour of its
    /// kind.
    fn item_color(&self, p: P, i: &Item) -> Color {
        self.calendar_color(i.calendar_id)
            .unwrap_or_else(|| self.kind_color(p, i.kind))
    }

    fn calendar_tz(&self, id: Id) -> String {
        self.calendars
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.tz.clone())
            .unwrap_or_default()
    }

    pub(super) fn update_calendar(&mut self, m: Message) -> Task<Message> {
        match m {
            Message::CalShift(n) => {
                if self.cal_week {
                    self.cal_selected += Duration::days(7 * n as i64);
                    self.cal_month = dates::month_start(self.cal_selected);
                } else {
                    self.cal_month = dates::add_months(self.cal_month, n);
                }
            }
            Message::CalToday => {
                self.cal_selected = self.today;
                self.cal_month = dates::month_start(self.today);
            }
            Message::CalSelect(d) => self.cal_selected = d,
            Message::ItemNew(kind, date) => {
                // The first calendar that is switched on.
                let calendar = self
                    .calendars
                    .iter()
                    .find(|c| c.visible)
                    .map_or(DEFAULT_CALENDAR, |c| c.id);
                self.modal = Modal::Item(ItemEdit {
                    id: 0,
                    kind,
                    title: String::new(),
                    date: date.to_string(),
                    time: String::new(),
                    duration: String::new(),
                    recur: Recur::None,
                    until: String::new(),
                    notes: text_editor::Content::new(),
                    done: false,
                    source: None,
                    calendar,
                    tz: self.calendar_tz(calendar),
                });
                return iced::widget::operation::focus("item-title");
            }
            Message::ItemOpen(id) => {
                if let Some(i) = self.items.iter().find(|i| i.id == id) {
                    self.modal = Modal::Item(ItemEdit {
                        id,
                        kind: i.kind,
                        title: i.title.clone(),
                        date: i.date.to_string(),
                        time: i
                            .time
                            .map(|t| t.format("%H:%M").to_string())
                            .unwrap_or_default(),
                        duration: i.duration_min.map(|d| d.to_string()).unwrap_or_default(),
                        recur: i.recur,
                        until: i.recur_until.map(|d| d.to_string()).unwrap_or_default(),
                        notes: text_editor::Content::with_text(&i.notes),
                        done: i.done,
                        source: i.source.clone(),
                        calendar: i.calendar_id,
                        tz: i.tz.clone().unwrap_or_default(),
                    });
                }
            }
            Message::ItemKindSet(k) => {
                if let Modal::Item(i) = &mut self.modal {
                    i.kind = k;
                }
            }
            Message::ItemRecur(r) => {
                if let Modal::Item(i) = &mut self.modal {
                    i.recur = r;
                }
            }
            Message::ItemSave => self.item_save(),
            Message::ItemDelete(id) => {
                let res = self.db.delete_item(id);
                self.check(res);
                self.modal = Modal::None;
                self.reload_calendar();
            }
            Message::ItemDone(id, done) => {
                if let Some(mut i) = self.items.iter().find(|i| i.id == id).cloned() {
                    i.done = done;
                    let res = self.db.save_item(&i);
                    self.check(res);
                    self.reload_calendar();
                }
            }
            Message::ItemCalendar(c) => {
                let zone = self.calendar_tz(c.id);
                if let Modal::Item(i) = &mut self.modal {
                    // A new item follows the zone of the calendar it goes in.
                    if i.id == 0 {
                        i.tz = zone;
                    }
                    i.calendar = c.id;
                }
            }
            Message::ItemNotes(action) => {
                if let Modal::Item(i) = &mut self.modal {
                    i.notes.perform(action);
                }
            }
            Message::CalendarNew => {
                self.modal = Modal::Calendar(CalendarEdit {
                    id: 0,
                    name: String::new(),
                    color: Some(SWATCHES[self.calendars.len() % SWATCHES.len()].to_string()),
                    tz: String::new(),
                });
                return iced::widget::operation::focus("calendar-name");
            }
            Message::CalendarOpen(id) => {
                if let Some(c) = self.calendars.iter().find(|c| c.id == id) {
                    self.modal = Modal::Calendar(CalendarEdit {
                        id,
                        name: c.name.clone(),
                        color: c.color.clone(),
                        tz: c.tz.clone().unwrap_or_default(),
                    });
                }
            }
            Message::CalendarColor(color) => {
                if let Modal::Calendar(c) = &mut self.modal {
                    c.color = color;
                }
            }
            Message::CalendarSave => self.calendar_save(),
            Message::CalendarVisible(id, on) => {
                if let Some(mut c) = self.calendars.iter().find(|c| c.id == id).cloned() {
                    c.visible = on;
                    let res = self.db.save_calendar(&c);
                    self.check(res);
                    self.reload_calendar();
                }
            }
            Message::IcsCalendar(c) => self.ics_calendar = c.id,
            Message::IcsKind(k) => self.ics_kind = k,
            Message::IcsImport => {
                let path = Self::expand_path(&self.ics_path);
                match std::fs::read_to_string(&path) {
                    Err(e) => self.toast(format!("Could not read {}: {e}", path.display()), true),
                    Ok(text) => {
                        let res = ics::import(&self.db, &text, self.ics_kind, self.ics_calendar);
                        if let Some((added, skipped)) = self.check(res) {
                            self.reload_calendar();
                            if added + skipped == 0 {
                                self.toast("No events were found in that file.", true);
                            } else {
                                self.toast(
                                    format!(
                                        "Imported {added} event(s); {skipped} were already there."
                                    ),
                                    false,
                                );
                                self.ics_path.clear();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn item_save(&mut self) {
        let Modal::Item(e) = &self.modal else { return };
        let fail = |app: &mut App, msg: &str| app.toast(msg.to_string(), true);
        let title = e.title.trim().to_string();
        if title.is_empty() {
            return fail(self, "Give it a title.");
        }
        let Some(date) = dates::parse_date(&e.date, self.today) else {
            return fail(
                self,
                "Could not read the date. Try 2026-12-01, tomorrow, fri or +3d.",
            );
        };
        let mut zone = match e.tz.trim() {
            "" => None,
            z => match tz::canonical(z) {
                Some(z) => Some(z),
                None => {
                    return fail(
                        self,
                        "Unknown time zone. Try America/New_York, Eastern or UTC.",
                    );
                }
            },
        };
        let time = match e.time.trim() {
            "" => None,
            _ if !e.kind.timed() => None,
            // "9pm Eastern" in the time field names the zone too.
            s => match tz::parse_time(s) {
                Some((t, z)) => {
                    if let Some(z) = z {
                        zone = Some(z.name().to_string());
                    }
                    Some(t)
                }
                None => return fail(self, "Could not read the time. Try 14:30 or 2:30pm."),
            },
        };
        let duration_min = match e.duration.trim() {
            "" => None,
            _ if time.is_none() => None,
            s => match s.parse::<i64>() {
                Ok(m) if (1..=24 * 60).contains(&m) => Some(m),
                _ => return fail(self, "The duration is a number of minutes."),
            },
        };
        let recur_until = match e.until.trim() {
            "" => None,
            _ if e.recur == Recur::None || e.kind == ItemKind::Deadline => None,
            s => match dates::parse_date(s, self.today) {
                Some(d) if d >= date => Some(d),
                Some(_) => return fail(self, "The end date is before the start."),
                None => return fail(self, "Could not read the end date."),
            },
        };
        let mut item = Item {
            id: e.id,
            kind: e.kind,
            title,
            date,
            time,
            duration_min,
            recur: e.recur,
            recur_until,
            done: e.done,
            source: e.source.clone(),
            notes: e.notes.text().trim_end().to_string(),
            calendar_id: e.calendar,
            tz: zone,
        };
        // Drops what the kind has no use for: a deadline does not repeat, a
        // holiday or a note has no time.
        item.normalise();
        let res = self.db.save_item(&item);
        if self.check(res).is_some() {
            self.modal = Modal::None;
            self.cal_selected = date;
            self.cal_month = dates::month_start(date);
            self.reload_calendar();
        }
    }

    fn calendar_save(&mut self) {
        let Modal::Calendar(e) = &self.modal else {
            return;
        };
        let name = e.name.trim().to_string();
        if name.is_empty() {
            return self.toast("Give the calendar a name.", true);
        }
        if self
            .calendars
            .iter()
            .any(|c| c.id != e.id && c.name.eq_ignore_ascii_case(&name))
        {
            return self.toast("There is already a calendar with that name.", true);
        }
        let zone = match e.tz.trim() {
            "" => None,
            z => match tz::canonical(z) {
                Some(z) => Some(z),
                None => {
                    return self.toast(
                        "Unknown time zone. Try America/New_York, Eastern or UTC.",
                        true,
                    );
                }
            },
        };
        let visible = self
            .calendars
            .iter()
            .find(|c| c.id == e.id)
            .is_none_or(|c| c.visible);
        let cal = Calendar {
            id: e.id,
            name,
            color: e.color.clone(),
            tz: zone,
            visible,
        };
        let res = self.db.save_calendar(&cal);
        if self.check(res).is_some() {
            self.modal = Modal::None;
            self.reload_calendar();
        }
    }

    /// One line for an item as it occurs on `date`, at local time `time`.
    pub(super) fn item_row<'a>(
        &'a self,
        p: P,
        i: &'a Item,
        date: NaiveDate,
        time: Option<NaiveTime>,
    ) -> El<'a> {
        let tint = self.item_color(p, i);
        let name = kind_icon(i.kind);
        let late = i.kind == ItemKind::Deadline && !i.done && date < self.today;
        let mut line = row![].spacing(8).align_y(Alignment::Center);
        if i.kind == ItemKind::Deadline {
            line = line.push(
                button(icon(
                    if i.done { "check" } else { name },
                    14.0,
                    if i.done { p.success } else { tint },
                ))
                .padding(4)
                .style(btn(p, Btn::Soft))
                .on_press(Message::ItemDone(i.id, !i.done)),
            );
        } else {
            line = line.push(container(icon(name, 15.0, tint)).padding(4));
        }
        if let Some(t) = time {
            let end = i.duration_min.map(|m| t + Duration::minutes(m));
            let label = match end {
                Some(e) => format!("{}–{}", self.time_text(t), self.time_text(e)),
                None => self.time_text(t),
            };
            line = line.push(text(label).size(12).color(tint));
        }
        let title = text(&i.title)
            .size(13)
            .color(if i.done { p.dim } else { p.text });
        if i.kind == ItemKind::Note && !i.notes.is_empty() {
            line = line.push(
                column![title, text(&i.notes).size(12).color(p.dim)]
                    .spacing(2)
                    .width(Fill),
            );
        } else {
            line = line.push(title.width(Fill));
        }
        if late {
            line = line.push(chip(
                p,
                format!("overdue · {}", dates::relative(date, self.today)),
                p.danger,
            ));
        }
        // Written in another zone: say what it was there.
        if let (Some(t), Some(z)) = (i.time, i.tz.as_deref()) {
            line = line.push(chip(
                p,
                format!(
                    "{} {}",
                    self.time_text(t),
                    tz::abbreviation(date.and_time(t), z)
                ),
                p.dim,
            ));
        }
        if i.recur != Recur::None {
            line = line.push(chip(p, i.recur.as_str(), p.dim));
        }
        button(line)
            .width(Fill)
            .padding([3.0, 4.0])
            .style(btn(p, Btn::Ghost))
            .on_press(Message::ItemOpen(i.id))
            .into()
    }

    fn day_cell(&self, p: P, d: NaiveDate, in_month: bool, max_items: usize) -> El<'_> {
        let [f_hol, f_appt, f_dead, f_habit, f_note] = self.cal_filters;
        let items = self.day_items(d);
        let holiday = items
            .iter()
            .map(|o| o.item)
            .find(|i| i.kind == ItemKind::Holiday)
            .filter(|_| f_hol);
        let selected = d == self.cal_selected;
        let is_today = d == self.today;

        let number = container(text(d.day().to_string()).size(12).color(if is_today {
            p.on_accent
        } else if in_month {
            p.text
        } else {
            alpha(p.dim, 0.6)
        }))
        .padding([1.0, 6.0])
        .style(move |_| container::Style {
            background: is_today.then(|| p.accent.into()),
            border: Border {
                radius: 99.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let mut head = row![number].spacing(4).align_y(Alignment::Center);
        if let Some(h) = holiday {
            head = head.push(
                text(&h.title)
                    .size(10)
                    .color(mix(p.holiday, p.text, 0.2))
                    .wrapping(text::Wrapping::None),
            );
        }

        // Timed and dated things, as chips.
        let mut lines: Vec<El> = Vec::new();
        if f_appt {
            for o in items
                .iter()
                .filter(|o| o.item.kind == ItemKind::Appointment)
            {
                let label = match o.time {
                    Some(t) => format!("{} {}", self.time_text(t), o.item.title),
                    None => o.item.title.clone(),
                };
                lines.push(cell_line(
                    p,
                    label,
                    self.item_color(p, o.item),
                    false,
                    false,
                ));
            }
        }
        if f_note {
            for o in items.iter().filter(|o| o.item.kind == ItemKind::Note) {
                let i = o.item;
                lines.push(cell_line(
                    p,
                    i.title.clone(),
                    self.item_color(p, i),
                    false,
                    false,
                ));
            }
        }
        if f_dead {
            for o in items.iter().filter(|o| o.item.kind == ItemKind::Deadline) {
                let i = o.item;
                lines.push(cell_line(
                    p,
                    i.title.clone(),
                    self.item_color(p, i),
                    true,
                    i.done,
                ));
            }
            for (c, _, done) in self.due_cards.iter().filter(|(c, ..)| c.due == Some(d)) {
                lines.push(cell_line(p, c.title.clone(), p.deadline, true, *done));
            }
        }
        let extra = lines.len().saturating_sub(max_items);
        let mut body = column![head].spacing(2);
        for l in lines.into_iter().take(max_items) {
            body = body.push(l);
        }
        if extra > 0 {
            body = body.push(
                container(text(format!("+{extra} more")).size(10).color(p.dim)).padding([0.0, 5.0]),
            );
        }
        body = body.push(space::vertical());

        // Habits: one thin line each. Cells touch, so done days join up.
        if f_habit {
            let mut chains = column![].spacing(2);
            for (idx, h) in self
                .habits
                .iter()
                .filter(|h| !h.archived)
                .enumerate()
                .take(6)
            {
                let c = habit_color(p, idx);
                let fill = match habits::day_state(h, d, &self.habit_ctx(h.id)) {
                    DayState::Done => c,
                    DayState::Skipped => alpha(c, 0.35),
                    DayState::Missed => alpha(p.danger, 0.22),
                    DayState::Pending => alpha(c, 0.18),
                    DayState::Rest | DayState::Outside => Color::TRANSPARENT,
                };
                chains = chains.push(
                    container(Space::new().height(3))
                        .width(Fill)
                        .style(style::flat(fill)),
                );
            }
            body = body.push(chains);
        }
        // Focus that day, as a share of the daily goal.
        let goal = self.cfg.timer.daily_goal.max(1) as f32;
        let frac = self
            .day_stats
            .get(&d)
            .map_or(0.0, |s| (s.completed as f32 / goal).min(1.0));
        let filled = (frac * 100.0).round() as u16;
        body = body.push(row![
            container(Space::new().height(2))
                .width(Length::FillPortion(filled.max(1)))
                .style(style::flat(if filled == 0 {
                    Color::TRANSPARENT
                } else {
                    p.focus
                })),
            Space::new().width(Length::FillPortion((100 - filled).max(1))),
        ]);

        let base = if !in_month {
            p.bg
        } else if holiday.is_some() {
            mix(p.surface, p.holiday, 0.13)
        } else {
            p.surface
        };
        button(body.height(Fill))
            .width(Fill)
            .height(Fill)
            .padding(iced::Padding {
                top: 5.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            })
            .style(move |_, status| iced::widget::button::Style {
                background: Some(
                    if matches!(status, iced::widget::button::Status::Hovered) {
                        mix(base, p.text, 0.05)
                    } else {
                        base
                    }
                    .into(),
                ),
                text_color: p.text,
                border: Border {
                    color: if selected {
                        p.accent
                    } else {
                        alpha(p.outline, 0.45)
                    },
                    width: if selected { 2.0 } else { 0.5 },
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .on_press(Message::CalSelect(d))
            .into()
    }

    fn day_detail(&self, p: P) -> El<'_> {
        let d = self.cal_selected;
        let items = self.day_items(d);
        let mut col = column![
            text(d.format("%A").to_string()).size(12).color(p.dim),
            text(d.format("%-d %B %Y").to_string()).size(19),
        ]
        .spacing(2);

        let mut list = column![].spacing(2);
        let mut any = false;
        for o in &items {
            any = true;
            list = list.push(self.item_row(p, o.item, d, o.time));
        }
        for (c, board, done) in self.due_cards.iter().filter(|(c, ..)| c.due == Some(d)) {
            any = true;
            list = list.push(
                button(
                    row![
                        container(icon(
                            if *done { "check" } else { "flag" },
                            15.0,
                            if *done { p.success } else { p.deadline }
                        ))
                        .padding(4),
                        text(&c.title).size(13).width(Fill).color(if *done {
                            p.dim
                        } else {
                            p.text
                        }),
                        chip(p, board.clone(), p.dim),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([3.0, 4.0])
                .style(btn(p, Btn::Ghost))
                .on_press(Message::CardOpen(c.id)),
            );
        }
        if !any {
            list = list.push(empty(p, "Nothing on this day."));
        }
        col = col.push(Space::new().height(p.u)).push(list);

        let mut habit_list = column![].spacing(4);
        let mut habits_shown = 0;
        for h in self.habits.iter().filter(|h| !h.archived) {
            if matches!(
                habits::day_state(h, d, &self.habit_ctx(h.id)),
                DayState::Outside | DayState::Rest
            ) {
                continue;
            }
            habits_shown += 1;
            habit_list = habit_list.push(self.habit_today_row(p, h, d, true));
        }
        if habits_shown > 0 {
            col = col
                .push(Space::new().height(p.u))
                .push(style::h2(p, "HABITS"))
                .push(habit_list);
        }
        if let Some(s) = self.day_stats.get(&d) {
            col = col
                .push(Space::new().height(p.u))
                .push(style::h2(p, "FOCUS"))
                .push(
                    text(format!(
                        "{} in {} session(s)",
                        dates::human(s.focus_s),
                        s.completed
                    ))
                    .size(13),
                );
        }

        col = col
            .push(Space::new().height(p.u))
            .push(style::h2(p, "ADD"))
            .push(
                row![
                    action(
                        p,
                        Btn::Tint(p.appointment),
                        Some("clock"),
                        "Appointment",
                        Message::ItemNew(ItemKind::Appointment, d)
                    ),
                    action(
                        p,
                        Btn::Tint(p.deadline),
                        Some("flag"),
                        "Deadline",
                        Message::ItemNew(ItemKind::Deadline, d)
                    ),
                ]
                .spacing(6),
            );
        col = col.push(
            row![
                action(
                    p,
                    Btn::Tint(p.holiday),
                    Some("star"),
                    "Holiday or day off",
                    Message::ItemNew(ItemKind::Holiday, d),
                ),
                action(
                    p,
                    Btn::Soft,
                    Some("note"),
                    "Note",
                    Message::ItemNew(ItemKind::Note, d)
                ),
            ]
            .spacing(6),
        );

        // Calendars: tick to show, click the name to edit.
        let mut cals = column![].spacing(2);
        for c in &self.calendars {
            let swatch = self.calendar_color(c.id).unwrap_or(p.appointment);
            cals = cals.push(
                row![
                    checkbox(c.visible).on_toggle({
                        let id = c.id;
                        move |on| Message::CalendarVisible(id, on)
                    }),
                    container(Space::new().width(10).height(10)).style(style::panel(p, swatch)),
                    button(
                        text(&c.name)
                            .size(13)
                            .color(if c.visible { p.text } else { p.dim })
                    )
                    .width(Fill)
                    .padding([3.0, 4.0])
                    .style(btn(p, Btn::Ghost))
                    .on_press(Message::CalendarOpen(c.id)),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        col = col
            .push(Space::new().height(p.u * 1.5))
            .push(style::h2(p, "CALENDARS"))
            .push(cals)
            .push(action(
                p,
                Btn::Soft,
                Some("plus"),
                "New calendar",
                Message::CalendarNew,
            ));

        col = col
            .push(Space::new().height(p.u * 1.5))
            .push(style::h2(p, "IMPORT A CALENDAR FILE"))
            .push(field(
                p,
                "Path to an .ics file",
                &self.ics_path,
                Field::IcsPath,
            ))
            .push(
                row![
                    pick_list(&ItemKind::ALL[..2], Some(self.ics_kind), Message::IcsKind)
                        .text_size(13),
                    pick_list(
                        self.calendars.clone(),
                        self.calendars
                            .iter()
                            .find(|c| c.id == self.ics_calendar)
                            .cloned(),
                        Message::IcsCalendar
                    )
                    .text_size(13),
                    action(p, Btn::Soft, None, "Import", Message::IcsImport),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .wrap(),
            )
            .push(style::dim(p, "Public holiday calendars are published as .ics files.").size(11));

        let col = col.spacing(6);
        if self.wide() {
            container(vscroll(p, col))
                .padding(p.u * 2.0)
                .width(310)
                .height(Fill)
                .style(style::card(p))
                .into()
        } else {
            // The page scrolls as a whole here, so no inner scroll area.
            container(col)
                .padding(p.u * 2.0)
                .width(Fill)
                .style(style::card(p))
                .into()
        }
    }

    pub fn view_calendar(&self, p: P) -> El<'_> {
        let (start, weeks, title) = if self.cal_week {
            let s = dates::week_start(self.cal_selected);
            let e = s + Duration::days(6);
            let title = if s.month() == e.month() {
                format!("{}–{} {}", s.day(), e.day(), e.format("%B %Y"))
            } else {
                format!("{} – {}", s.format("%-d %b"), e.format("%-d %b %Y"))
            };
            (s, 1, title)
        } else {
            let s = dates::week_start(self.cal_month);
            let end = dates::add_months(self.cal_month, 1);
            let weeks = ((end - s).num_days() as f32 / 7.0).ceil() as i64;
            (s, weeks, self.cal_month.format("%B %Y").to_string())
        };

        let mut header = row![];
        for i in 0..7 {
            let d = start + Duration::days(i);
            let label = if self.cal_week {
                d.format("%a %-d").to_string()
            } else {
                d.format("%a").to_string()
            };
            header = header.push(
                container(text(label).size(11).color(p.dim))
                    .width(Fill)
                    .padding([4.0, 6.0]),
            );
        }
        let wide = self.wide();
        // With room, the weeks share the height. Without, each gets a fixed
        // height and the whole view scrolls.
        let row_h = if wide {
            Fill
        } else {
            Length::Fixed(if self.cal_week { 240.0 } else { 96.0 })
        };
        let mut grid = column![header];
        for w in 0..weeks {
            let mut r = row![].height(row_h);
            for i in 0..7 {
                let d = start + Duration::days(w * 7 + i);
                let in_month = self.cal_week || d.month() == self.cal_month.month();
                r = r.push(self.day_cell(
                    p,
                    d,
                    in_month,
                    if self.cal_week {
                        14
                    } else if wide {
                        3
                    } else {
                        2
                    },
                ));
            }
            grid = grid.push(r);
        }

        let mut filters = row![].spacing(6).align_y(Alignment::Center);
        let tints = [p.holiday, p.appointment, p.deadline, p.habit, p.dim];
        for (i, (label, name)) in FILTERS.iter().enumerate() {
            let on = self.cal_filters[i];
            filters = filters.push(
                button(
                    row![
                        icon(name, 13.0, if on { tints[i] } else { p.dim }),
                        text(*label).size(12).color(if on { p.text } else { p.dim })
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .padding([4.0, 9.0])
                .style(btn(p, if on { Btn::Tint(tints[i]) } else { Btn::Ghost }))
                .on_press(Message::Toggle(Toggle::CalFilter(i), !on)),
            );
        }

        let nav = row![
            icon_btn(p, Btn::Soft, "left", Message::CalShift(-1)),
            icon_btn(p, Btn::Soft, "right", Message::CalShift(1)),
            action(p, Btn::Soft, None, "Today", Message::CalToday),
            action(
                p,
                if self.cal_week {
                    Btn::Ghost
                } else {
                    Btn::Selected
                },
                None,
                "Month",
                Message::Toggle(Toggle::CalWeek, false)
            ),
            action(
                p,
                if self.cal_week {
                    Btn::Selected
                } else {
                    Btn::Ghost
                },
                None,
                "Week",
                Message::Toggle(Toggle::CalWeek, true)
            ),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let grid = container(grid)
            .width(Fill)
            .style(move |_| container::Style {
                border: Border {
                    color: alpha(p.outline, 0.6),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });
        let body: El = if wide {
            row![grid.height(Fill), self.day_detail(p)]
                .spacing(p.u * 2.0)
                .height(Fill)
                .into()
        } else {
            vscroll(p, column![grid, self.day_detail(p)].spacing(p.u * 1.5))
        };
        page(
            p,
            wide,
            title,
            Some(nav.wrap().into()),
            column![filters.wrap(), body].spacing(p.u * 1.5),
        )
    }

    pub(super) fn item_editor<'a>(&'a self, p: P, e: &'a ItemEdit) -> El<'a> {
        let tint = self.kind_color(p, e.kind);
        let date_bad = dates::parse_date(&e.date, self.today).is_none();
        let time_bad = !e.time.trim().is_empty() && tz::parse_time(&e.time).is_none();
        let tz_bad = !e.tz.trim().is_empty() && tz::parse(&e.tz).is_none();
        let mut kinds = row![].spacing(6);
        for k in ItemKind::ALL {
            kinds = kinds.push(action(
                p,
                if k == e.kind {
                    Btn::Tint(self.kind_color(p, k))
                } else {
                    Btn::Ghost
                },
                Some(kind_icon(k)),
                k.label(),
                Message::ItemKindSet(k),
            ));
        }
        let mut col = column![
            kinds.wrap(),
            field(p, "Title", &e.title, Field::ItemTitle)
                .id("item-title")
                .size(15),
        ]
        .spacing(p.u * 1.5);

        let mut when = row![labelled(
            p,
            "DATE",
            field(p, "2026-12-01, fri, Jan 5", &e.date, Field::ItemDate)
                .style(style::input_checked(p, date_bad))
        )]
        .spacing(p.u * 1.5);
        if e.kind.timed() {
            when = when.push(labelled(
                p,
                "TIME (OPTIONAL)",
                field(p, "14:30 or 2:30pm", &e.time, Field::ItemTime)
                    .style(style::input_checked(p, time_bad)),
            ));
        }
        if e.kind == ItemKind::Appointment {
            when = when.push(labelled(
                p,
                "MINUTES",
                field(p, "45", &e.duration, Field::ItemDuration),
            ));
        }
        col = col.push(when);

        let mut place = row![labelled(
            p,
            "CALENDAR",
            pick_list(
                self.calendars.clone(),
                self.calendars.iter().find(|c| c.id == e.calendar).cloned(),
                Message::ItemCalendar
            )
            .width(190)
            .text_size(13)
        )]
        .spacing(p.u * 1.5);
        if e.kind.timed() {
            place = place.push(labelled(
                p,
                "TIME ZONE (LOCAL IF EMPTY)",
                field(p, "Eastern, Europe/Berlin", &e.tz, Field::ItemTz)
                    .style(style::input_checked(p, tz_bad)),
            ));
        }
        col = col.push(place);

        match e.kind {
            ItemKind::Deadline => {
                col = col.push(
                    checkbox(e.done)
                        .label("Done")
                        .on_toggle(|v| Message::Toggle(Toggle::ItemDone, v))
                        .text_size(13),
                );
            }
            _ => {
                let mut rep = row![labelled(
                    p,
                    "REPEATS",
                    pick_list(&Recur::ALL[..], Some(e.recur), Message::ItemRecur)
                        .width(190)
                        .text_size(13)
                )]
                .spacing(p.u * 1.5);
                if e.recur != Recur::None {
                    rep = rep.push(labelled(
                        p,
                        "UNTIL (OPTIONAL)",
                        field(p, "date", &e.until, Field::ItemUntil),
                    ));
                }
                col = col.push(rep);
            }
        }
        col = col.push(
            text_editor(&e.notes)
                .placeholder(if e.kind == ItemKind::Note {
                    "The note"
                } else {
                    "Notes"
                })
                .on_action(Message::ItemNotes)
                .height(if e.kind == ItemKind::Note { 150 } else { 70 })
                .size(13),
        );
        if e.kind == ItemKind::Holiday {
            col = col.push(
                style::dim(
                    p,
                    "Habits set to rest on holidays do not lose their chain on this day.",
                )
                .size(11),
            );
        }

        let mut buttons = row![].spacing(8).align_y(Alignment::Center);
        if e.id != 0 {
            buttons = buttons.push(action(
                p,
                Btn::Danger,
                Some("trash"),
                "Delete",
                Message::ItemDelete(e.id),
            ));
        }
        buttons = buttons
            .push(space::horizontal())
            .push(action(p, Btn::Ghost, None, "Cancel", Message::CloseModal))
            .push(action(p, Btn::Tint(tint), None, "Save", Message::ItemSave));
        col.push(buttons).into()
    }

    pub(super) fn calendar_editor<'a>(&'a self, p: P, e: &'a CalendarEdit) -> El<'a> {
        let tz_bad = !e.tz.trim().is_empty() && tz::parse(&e.tz).is_none();
        let swatch = |color: Option<Color>, value: Option<String>, on: bool| -> El<'a> {
            let fill = color.unwrap_or(p.surface3);
            button(Space::new().width(18).height(18))
                .padding(0)
                .style(move |_, _| iced::widget::button::Style {
                    background: Some(fill.into()),
                    border: Border {
                        color: if on { p.text } else { alpha(p.outline, 0.6) },
                        width: if on { 2.0 } else { 1.0 },
                        radius: 99.0.into(),
                    },
                    ..Default::default()
                })
                .on_press(Message::CalendarColor(value))
                .into()
        };
        let mut colours = row![swatch(None, None, e.color.is_none())]
            .spacing(8)
            .align_y(Alignment::Center);
        for hex in SWATCHES {
            colours = colours.push(swatch(
                crate::theme::Rgb::parse(hex).map(style::col),
                Some(hex.to_string()),
                e.color.as_deref() == Some(hex),
            ));
        }
        let mut buttons = row![].spacing(8).align_y(Alignment::Center);
        if e.id != 0 && e.id != DEFAULT_CALENDAR {
            buttons = buttons.push(action(
                p,
                Btn::Danger,
                Some("trash"),
                "Delete",
                Message::Ask(ConfirmAction::DeleteCalendar(e.id)),
            ));
        }
        buttons = buttons
            .push(space::horizontal())
            .push(action(p, Btn::Ghost, None, "Cancel", Message::CloseModal))
            .push(action(p, Btn::Primary, None, "Save", Message::CalendarSave));
        column![
            text(if e.id == 0 {
                "New calendar"
            } else {
                "Calendar"
            })
            .size(17),
            field(p, "Name", &e.name, Field::CalendarName).id("calendar-name"),
            labelled(p, "COLOUR (FIRST IS THE THEME'S)", colours),
            labelled(
                p,
                "TIME ZONE FOR NEW ITEMS (LOCAL IF EMPTY)",
                field(p, "Eastern, Europe/Berlin", &e.tz, Field::CalendarTz)
                    .style(style::input_checked(p, tz_bad))
            ),
            buttons,
        ]
        .spacing(p.u * 1.5)
        .into()
    }
}

/// A one-line entry inside a day cell: a coloured edge and a label.
fn cell_line<'a>(p: P, label: String, tint: Color, flag: bool, done: bool) -> El<'a> {
    let fg = if done { p.dim } else { mix(tint, p.text, 0.45) };
    let mut r = row![
        container(Space::new().width(3).height(12)).style(style::flat(if done {
            p.dim
        } else {
            tint
        }))
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if flag {
        r = r.push(icon(if done { "check" } else { "flag" }, 9.0, fg));
    }
    r = r.push(
        text(label)
            .size(10)
            .color(fg)
            .wrapping(text::Wrapping::None),
    );
    container(r)
        .width(Fill)
        .clip(true)
        .style(style::flat(alpha(tint, if done { 0.05 } else { 0.13 })))
        .into()
}
