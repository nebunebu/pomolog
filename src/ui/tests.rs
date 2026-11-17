//! Drives the update loop the way the interface would, without a window.

use super::*;
use crate::theme::palette::SystemInput;
use chrono::Datelike;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};

fn app() -> App {
    App::build(
        Db::open_in_memory().unwrap(),
        Config::default(),
        None,
        SystemInput::default(),
    )
}

fn send(app: &mut App, m: Message) {
    let _ = app.update(m);
}

fn type_in(app: &mut App, f: Field, v: &str) {
    send(app, Message::Field(f, v.to_string()));
}

fn list(app: &App, kind: ListKind) -> Id {
    app.lists.iter().find(|l| l.kind == kind).unwrap().id
}

fn add_card(app: &mut App, kind: ListKind, title: &str) -> Id {
    let id = app.db.add_card(list(app, kind), title, now()).unwrap();
    app.reload_board();
    id
}

fn titles_in(app: &App, list: Id) -> Vec<String> {
    app.cards
        .iter()
        .filter(|c| c.list_id == list)
        .map(|c| c.title.clone())
        .collect()
}

fn toast_is_error(app: &App) -> bool {
    app.toast.as_ref().is_some_and(|t| t.1)
}

fn drag(app: &mut App, card: Id) {
    send(app, Message::CardPress(card));
    send(app, Message::Cursor(Point::new(10.0, 10.0)));
    send(app, Message::Cursor(Point::new(80.0, 60.0)));
    assert!(app.drag.as_ref().unwrap().active);
}

#[test]
fn starts_with_a_board_and_three_lists() {
    let app = app();
    assert_eq!(app.boards.len(), 1);
    assert_eq!(app.lists.len(), 3);
    assert!(app.look.dark, "dark by default");
}

#[test]
fn dragging_a_card_to_a_list_moves_it_and_marks_it_done() {
    let mut app = app();
    let a = add_card(&mut app, ListKind::Normal, "a");
    add_card(&mut app, ListKind::Normal, "b");
    let done = list(&app, ListKind::Done);

    drag(&mut app, a);
    send(&mut app, Message::ListHover(done, true));
    send(&mut app, Message::MouseUp);

    assert!(app.drag.is_none());
    assert_eq!(titles_in(&app, done), ["a"]);
    assert!(app.db.card(a).unwrap().unwrap().completed_at.is_some());
    assert!(matches!(app.modal, Modal::None), "a drag is not a click");
}

#[test]
fn dropping_on_a_card_inserts_before_it() {
    let mut app = app();
    let a = add_card(&mut app, ListKind::Normal, "a");
    let c = add_card(&mut app, ListKind::Progress, "c");
    add_card(&mut app, ListKind::Progress, "d");
    let progress = list(&app, ListKind::Progress);

    drag(&mut app, a);
    send(&mut app, Message::ListHover(progress, true));
    send(&mut app, Message::CardHover(c, true));
    send(&mut app, Message::MouseUp);
    assert_eq!(titles_in(&app, progress), ["a", "c", "d"]);

    // Leaving the card and dropping in the empty part of the list appends.
    let d = app.cards.iter().find(|k| k.title == "d").unwrap().id;
    drag(&mut app, a);
    send(&mut app, Message::CardHover(d, true));
    send(&mut app, Message::CardHover(d, false));
    send(&mut app, Message::MouseUp);
    assert_eq!(titles_in(&app, progress), ["c", "d", "a"]);
}

#[test]
fn dropping_outside_any_list_changes_nothing() {
    let mut app = app();
    let a = add_card(&mut app, ListKind::Normal, "a");
    let todo = list(&app, ListKind::Normal);
    add_card(&mut app, ListKind::Normal, "b");
    drag(&mut app, a);
    send(&mut app, Message::CardHover(a, false));
    send(&mut app, Message::ListHover(todo, false));
    send(&mut app, Message::MouseUp);
    assert_eq!(titles_in(&app, todo), ["a", "b"]);

    // Released on itself without leaving: also nothing.
    drag(&mut app, a);
    send(&mut app, Message::MouseUp);
    assert_eq!(titles_in(&app, todo), ["a", "b"]);

    // Dragged off itself into the empty part of its own list: goes last.
    drag(&mut app, a);
    send(&mut app, Message::CardHover(a, false));
    send(&mut app, Message::MouseUp);
    assert_eq!(titles_in(&app, todo), ["b", "a"]);
}

#[test]
fn a_click_opens_the_editor_and_saving_validates() {
    let mut app = app();
    let a = add_card(&mut app, ListKind::Normal, "a");
    send(&mut app, Message::CardPress(a));
    send(&mut app, Message::MouseUp);
    assert!(matches!(app.modal, Modal::Card(_)));

    type_in(&mut app, Field::CardTitle, "Report");
    type_in(&mut app, Field::CardDue, "whenever");
    send(&mut app, Message::CardSave);
    assert!(
        matches!(app.modal, Modal::Card(_)),
        "stays open on a bad date"
    );
    assert!(toast_is_error(&app));
    assert_eq!(
        app.db.card(a).unwrap().unwrap().title,
        "a",
        "nothing saved yet"
    );

    type_in(&mut app, Field::CardDue, "tomorrow");
    type_in(&mut app, Field::CardEstimate, "2,5");
    type_in(&mut app, Field::CardTags, "#Writing, deep");
    let progress = list(&app, ListKind::Progress);
    send(
        &mut app,
        Message::CardList(Choice {
            id: Some(progress),
            label: String::new(),
        }),
    );
    send(&mut app, Message::CardSave);
    assert!(matches!(app.modal, Modal::None));
    let c = app.db.card(a).unwrap().unwrap();
    assert_eq!(
        (c.title.as_str(), c.estimate, c.tags.as_str()),
        ("Report", 2.5, "writing,deep")
    );
    assert_eq!(c.due, app.today.succ_opt());
    assert_eq!(c.list_id, progress);
    assert_eq!(app.in_progress.len(), 1);
}

#[test]
fn inline_add_and_search() {
    let mut app = app();
    let todo = list(&app, ListKind::Normal);
    send(&mut app, Message::CardAddTo(Some(todo)));
    type_in(&mut app, Field::NewCard, "  First  ");
    send(&mut app, Message::Submit(Field::NewCard));
    type_in(&mut app, Field::NewCard, "   ");
    send(&mut app, Message::Submit(Field::NewCard));
    assert_eq!(
        titles_in(&app, todo),
        ["First"],
        "blank titles are not added"
    );
    assert_eq!(app.adding_to, None, "a blank entry closes the field");
}

#[test]
fn lists_and_boards_through_prompts_and_confirmations() {
    let mut app = app();
    send(&mut app, Message::PromptOpen(PromptAction::NewList));
    type_in(&mut app, Field::Prompt, "Blocked");
    send(&mut app, Message::Submit(Field::Prompt));
    assert_eq!(app.lists.len(), 4);
    let blocked = app.lists.iter().find(|l| l.name == "Blocked").unwrap().id;

    send(&mut app, Message::Ask(ConfirmAction::DeleteList(blocked)));
    assert!(matches!(app.modal, Modal::Confirm { .. }));
    send(&mut app, Message::CloseModal);
    assert_eq!(app.lists.len(), 4, "cancelling keeps it");
    send(&mut app, Message::Ask(ConfirmAction::DeleteList(blocked)));
    send(&mut app, Message::ConfirmYes);
    assert_eq!(app.lists.len(), 3);

    let done = list(&app, ListKind::Done);
    send(&mut app, Message::Ask(ConfirmAction::DeleteList(done)));
    send(&mut app, Message::ConfirmYes);
    assert_eq!(app.lists.len(), 3, "fixed lists cannot be deleted");

    send(&mut app, Message::PromptOpen(PromptAction::NewBoard));
    type_in(&mut app, Field::Prompt, "Home");
    send(&mut app, Message::Submit(Field::Prompt));
    assert_eq!(app.boards.len(), 2);
    let home = app.board_id.unwrap();
    assert_eq!(
        app.boards.iter().find(|b| b.id == home).unwrap().name,
        "Home"
    );

    for b in app.boards.clone() {
        send(&mut app, Message::Ask(ConfirmAction::DeleteBoard(b.id)));
        send(&mut app, Message::ConfirmYes);
    }
    assert_eq!(app.boards.len(), 1, "there is always a board");
}

#[test]
fn calendar_items_are_validated_and_saved() {
    let mut app = app();
    let today = app.today;
    send(&mut app, Message::ItemNew(ItemKind::Appointment, today));
    send(&mut app, Message::ItemSave);
    assert!(toast_is_error(&app), "a title is required");

    type_in(&mut app, Field::ItemTitle, "Dentist");
    type_in(&mut app, Field::ItemTime, "25:99");
    send(&mut app, Message::ItemSave);
    assert!(matches!(app.modal, Modal::Item(_)) && app.items.is_empty());

    type_in(&mut app, Field::ItemTime, "14:30");
    type_in(&mut app, Field::ItemDuration, "45");
    send(&mut app, Message::ItemRecur(Recur::Weekly));
    type_in(&mut app, Field::ItemUntil, "2000-01-01");
    send(&mut app, Message::ItemSave);
    assert!(app.items.is_empty(), "an end before the start is refused");
    type_in(&mut app, Field::ItemUntil, "");
    send(&mut app, Message::ItemSave);
    assert!(matches!(app.modal, Modal::None));
    let i = &app.items[0];
    assert_eq!((i.recur, i.duration_min), (Recur::Weekly, Some(45)));
    assert_eq!(i.time, chrono::NaiveTime::from_hms_opt(14, 30, 0));

    // A holiday has no time, and shows up in the holiday set.
    send(&mut app, Message::ItemNew(ItemKind::Holiday, today));
    type_in(&mut app, Field::ItemTitle, "Day off");
    type_in(&mut app, Field::ItemTime, "9:00");
    send(&mut app, Message::ItemSave);
    let h = app
        .items
        .iter()
        .find(|i| i.kind == ItemKind::Holiday)
        .unwrap();
    assert_eq!(h.time, None);
    assert!(app.holidays.contains(&today));

    // A deadline never repeats and can be ticked off.
    send(&mut app, Message::ItemNew(ItemKind::Deadline, today));
    type_in(&mut app, Field::ItemTitle, "Taxes");
    send(&mut app, Message::ItemRecur(Recur::Daily));
    send(&mut app, Message::ItemSave);
    let d = app
        .items
        .iter()
        .find(|i| i.kind == ItemKind::Deadline)
        .unwrap()
        .clone();
    assert_eq!(d.recur, Recur::None);
    send(&mut app, Message::ItemDone(d.id, true));
    assert!(app.items.iter().find(|i| i.id == d.id).unwrap().done);

    send(&mut app, Message::ItemOpen(d.id));
    send(&mut app, Message::ItemDelete(d.id));
    assert_eq!(app.items.len(), 2);
}

#[test]
fn calendar_navigation() {
    let mut app = app();
    let month = app.cal_month;
    send(&mut app, Message::CalShift(1));
    assert_eq!(app.cal_month, dates::add_months(month, 1));
    send(&mut app, Message::Toggle(Toggle::CalWeek, true));
    send(&mut app, Message::CalToday);
    send(&mut app, Message::CalShift(-1));
    assert_eq!(app.cal_selected, app.today - Duration::days(7));
    send(&mut app, Message::Toggle(Toggle::CalFilter(3), false));
    assert_eq!(app.cal_filters, [true, true, true, false, true]);
}

fn new_habit(app: &mut App, name: &str) -> Id {
    send(app, Message::HabitNew);
    type_in(app, Field::HabitName, name);
    send(app, Message::HabitSave);
    assert!(
        matches!(app.modal, Modal::None),
        "habit should have saved: {:?}",
        app.toast
    );
    app.habits.iter().find(|h| h.name == name).unwrap().id
}

#[test]
fn habits_check_skip_and_streak() {
    let mut app = app();
    send(&mut app, Message::HabitNew);
    send(&mut app, Message::HabitSave);
    assert!(
        toast_is_error(&app) && app.habits.is_empty(),
        "a name is required"
    );
    send(&mut app, Message::CloseModal);

    let id = new_habit(&mut app, "Read");
    let today = app.today;
    assert!(app.streaks[&id].at_risk);
    send(&mut app, Message::HabitCheck(id, today, true));
    assert_eq!(app.streaks[&id].current, 1);
    assert!(!app.streaks[&id].at_risk);
    send(&mut app, Message::HabitCheck(id, today, false));
    assert_eq!(app.streaks[&id].current, 0);

    // Two skips a month by default; the third is refused.
    let mut h = app.habits[0].clone();
    h.created = today - Duration::days(40);
    app.db.save_habit(&h).unwrap();
    app.reload_habits();
    let month_start = dates::month_start(today);
    for offset in 0..3 {
        // Stay inside the current month where possible.
        let d = if today.day0() >= 2 {
            today - Duration::days(offset)
        } else {
            month_start + Duration::days(offset)
        };
        send(&mut app, Message::HabitSkip(id, d, true));
    }
    let skipped = app
        .stored_entries_of(id)
        .unwrap()
        .values()
        .filter(|e| e.skipped)
        .count();
    assert_eq!(skipped, 2);
    assert!(toast_is_error(&app));
}

#[test]
fn quantity_and_weekday_habits() {
    let mut app = app();
    send(&mut app, Message::HabitNew);
    type_in(&mut app, Field::HabitName, "Pages");
    send(&mut app, Message::HabitKindSet(HabitKind::Quantity));
    type_in(&mut app, Field::HabitTarget, "lots");
    send(&mut app, Message::HabitSave);
    assert!(app.habits.is_empty(), "the target must be a number");
    type_in(&mut app, Field::HabitTarget, "20");
    type_in(&mut app, Field::HabitUnit, "pages");
    send(&mut app, Message::HabitSched(SchedKind::Weekdays));
    for i in 0..7 {
        send(&mut app, Message::Toggle(Toggle::HabitWeekday(i), false));
    }
    send(&mut app, Message::HabitSave);
    assert!(app.habits.is_empty(), "at least one weekday is needed");
    for i in 0..7 {
        send(&mut app, Message::Toggle(Toggle::HabitWeekday(i), true));
    }
    send(&mut app, Message::HabitSave);
    let h = app.habits[0].clone();
    assert_eq!(h.schedule, Schedule::Daily, "all seven days is every day");
    assert_eq!((h.target, h.unit.as_str()), (20.0, "pages"));

    type_in(&mut app, Field::HabitQty(h.id), "12");
    send(&mut app, Message::Submit(Field::HabitQty(h.id)));
    assert_eq!(app.streaks[&h.id].current, 0, "12 of 20 is not done");
    type_in(&mut app, Field::HabitQty(h.id), "x");
    send(&mut app, Message::Submit(Field::HabitQty(h.id)));
    assert!(toast_is_error(&app));
    type_in(&mut app, Field::HabitQty(h.id), "20");
    send(&mut app, Message::Submit(Field::HabitQty(h.id)));
    assert_eq!(app.streaks[&h.id].current, 1);

    // Reopening the editor shows what was saved, and it can be archived.
    send(&mut app, Message::HabitOpen(h.id));
    let Modal::Habit(e) = &app.modal else {
        panic!("editor should be open")
    };
    assert_eq!((e.target.as_str(), e.sched), ("20", SchedKind::Daily));
    send(&mut app, Message::Toggle(Toggle::HabitArchived, true));
    send(&mut app, Message::HabitSave);
    assert!(app.habits[0].archived);
    send(&mut app, Message::Ask(ConfirmAction::DeleteHabit(h.id)));
    send(&mut app, Message::ConfirmYes);
    assert!(app.habits.is_empty());
}

#[test]
fn focus_habit_completes_from_logged_time_on_its_tag() {
    let mut app = app();
    let card = add_card(&mut app, ListKind::Progress, "Essay");
    let mut c = app.db.card(card).unwrap().unwrap();
    c.tags = "writing".into();
    app.db.update_card(&c).unwrap();
    let other = add_card(&mut app, ListKind::Progress, "Email");

    send(&mut app, Message::HabitNew);
    type_in(&mut app, Field::HabitName, "Deep writing");
    send(&mut app, Message::HabitKindSet(HabitKind::Focus));
    type_in(&mut app, Field::HabitTarget, "30");
    type_in(&mut app, Field::HabitTag, "#Writing");
    send(&mut app, Message::HabitSave);
    let id = app.habits[0].id;
    assert_eq!(app.habits[0].link_tag.as_deref(), Some("writing"));

    let log = |app: &mut App, card: Id, mins: &str| {
        send(
            app,
            Message::TimerCard(Choice {
                id: Some(card),
                label: String::new(),
            }),
        );
        type_in(app, Field::ManualMin, mins);
        send(app, Message::ManualAdd);
    };
    log(&mut app, other, "60");
    assert_eq!(
        app.streaks[&id].current, 0,
        "time on another tag does not count"
    );
    log(&mut app, card, "20");
    assert_eq!(app.streaks[&id].current, 0);
    log(&mut app, card, "15");
    assert_eq!(
        app.streaks[&id].current, 1,
        "35 minutes on #writing reaches 30"
    );
    assert_eq!(app.db.card(card).unwrap().unwrap().spent_s, 35 * 60);

    type_in(&mut app, Field::ManualMin, "-3");
    send(&mut app, Message::ManualAdd);
    assert!(toast_is_error(&app));
    assert_eq!(app.today_sessions.len(), 3);
}

#[test]
fn timer_messages_log_sessions() {
    let mut app = app();
    let card = add_card(&mut app, ListKind::Progress, "Essay");
    send(&mut app, Message::StartCard(card));
    assert!(app.timer.is_running());
    assert_eq!(app.view, View::Timer);
    assert_eq!(bridge_task(&app), Some("Essay".to_string()));

    send(&mut app, Message::TimerToggle);
    assert!(!app.timer.is_running());
    type_in(&mut app, Field::Reason, "phone");
    send(&mut app, Message::TimerToggle);
    assert_eq!(app.reason, "phone", "kept until the phase ends");

    // Pretend twenty seconds went by, then give up.
    let later = now() + Duration::seconds(20);
    let fx = app.timer.tick(later);
    app.apply(fx);
    let fx = app.timer.abandon(later, "meeting");
    app.apply(fx);
    assert!(app.timer.active.is_none());
    let s = app.today_sessions[0].clone();
    assert_eq!(
        (s.card_id, s.completed, s.interruptions),
        (Some(card), false, 2)
    );
    assert_eq!(app.completed_today(), 0);

    send(&mut app, Message::SessionDelete(s.id));
    assert!(app.today_sessions.is_empty());
}

fn bridge_task(app: &App) -> Option<String> {
    app.last_status.task.clone()
}

#[test]
fn deleting_the_active_card_detaches_the_timer() {
    let mut app = app();
    let card = add_card(&mut app, ListKind::Progress, "Essay");
    send(
        &mut app,
        Message::TimerCard(Choice {
            id: Some(card),
            label: String::new(),
        }),
    );
    send(&mut app, Message::Ask(ConfirmAction::DeleteCard(card)));
    send(&mut app, Message::ConfirmYes);
    assert_eq!(app.timer.card_id, None);
}

#[test]
fn keyboard_shortcuts() {
    let mut app = app();
    let key = |app: &mut App, k: Key, m: Modifiers, captured: bool| {
        send(app, Message::Key(k, m, captured))
    };
    key(&mut app, Key::Character("k".into()), Modifiers::CTRL, false);
    assert!(matches!(app.modal, Modal::Palette { .. }));
    key(
        &mut app,
        Key::Named(Named::Space),
        Modifiers::empty(),
        false,
    );
    assert!(
        app.timer.active.is_none(),
        "space does nothing while a dialog is open"
    );
    key(
        &mut app,
        Key::Named(Named::Escape),
        Modifiers::empty(),
        false,
    );
    assert!(matches!(app.modal, Modal::None));

    key(&mut app, Key::Named(Named::Space), Modifiers::empty(), true);
    assert!(
        app.timer.active.is_none(),
        "space typed into a field is not a shortcut"
    );
    key(
        &mut app,
        Key::Named(Named::Space),
        Modifiers::empty(),
        false,
    );
    assert!(app.timer.is_running());

    key(&mut app, Key::Character("4".into()), Modifiers::CTRL, false);
    assert_eq!(app.view, View::Calendar);
    key(&mut app, Key::Character("9".into()), Modifiers::CTRL, false);
    key(&mut app, Key::Character("0".into()), Modifiers::CTRL, false);
    assert_eq!(app.view, View::Calendar, "out-of-range numbers are ignored");

    app.zen = true;
    key(
        &mut app,
        Key::Named(Named::Escape),
        Modifiers::empty(),
        false,
    );
    assert!(!app.zen);
}

#[test]
fn command_palette() {
    let mut app = app();
    let card = add_card(&mut app, ListKind::Progress, "Essay");
    let first = |app: &App, q: &str| app.palette_actions(q).into_iter().next().map(|a| a.1);
    assert_eq!(first(&app, "go to board"), Some(Action::Go(View::Board)));
    assert_eq!(first(&app, "essay"), Some(Action::StartCard(card)));
    assert_eq!(first(&app, "nord"), Some(Action::Theme("nord".into())));
    assert_eq!(
        first(&app, "Buy milk"),
        Some(Action::Intake("Buy milk".into()))
    );
    let label = |app: &App, q: &str| app.palette_actions(q).pop().map(|a| a.0);
    assert_eq!(
        label(&app, "Buy milk").as_deref(),
        Some("Add Card: Buy milk")
    );
    assert_eq!(
        label(&app, "Dentist tomorrow 2pm").as_deref(),
        Some("Add Appointment: Dentist (tomorrow 2pm)")
    );
    assert!(app.palette_actions("").len() <= 9);

    app.modal = Modal::Palette {
        query: String::new(),
    };
    type_in(&mut app, Field::PaletteQuery, "Buy milk");
    send(&mut app, Message::Submit(Field::PaletteQuery));
    assert!(matches!(app.modal, Modal::None));
    assert!(app.cards.iter().any(|c| c.title == "Buy milk"));

    // The same box takes calendar items and notes.
    send(
        &mut app,
        Message::Run(Action::Intake("Dentist tomorrow 2pm for 45m".into())),
    );
    let appt = app.items.iter().find(|i| i.title == "Dentist").unwrap();
    assert_eq!(appt.date, app.today + Duration::days(1));
    assert_eq!(appt.duration_min, Some(45));
    send(
        &mut app,
        Message::Run(Action::Intake("note: gate code 4411".into())),
    );
    assert!(app.items.iter().any(|i| i.kind == ItemKind::Note));
    send(
        &mut app,
        Message::Run(Action::Intake("Thesis due fri #writing".into())),
    );
    // The date and the tag are taken out of the title.
    let card = app.cards.iter().find(|c| c.title == "Thesis").unwrap();
    assert!(card.due.is_some() && card.tags == "writing");
}

#[test]
fn changes_from_another_process_show_up() {
    let dir = std::env::temp_dir().join(format!("pomolog-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("t.db");
    let mut app = App::build(
        Db::open(&path).unwrap(),
        Config::default(),
        None,
        SystemInput::default(),
    );
    assert!(!app.sync_outside(), "nothing has happened yet");
    let today = app.today;
    send(&mut app, Message::ItemNew(ItemKind::Note, today));
    type_in(&mut app, Field::ItemTitle, "Mine");
    send(&mut app, Message::ItemSave);
    assert!(!app.sync_outside(), "its own writes are not news");

    // What `pomolog event` and `pomolog add` do while the app is running.
    let cli = Db::open(&path).unwrap();
    let text = ItemText {
        title: "From outside".into(),
        date: "tomorrow".into(),
        ..Default::default()
    };
    crate::cli::add_event(&cli, &text, false, today, false).unwrap();
    crate::cli::add_card(&cli, "Card from outside", None, None, &[], now()).unwrap();
    assert!(!app.items.iter().any(|i| i.title == "From outside"));
    assert!(app.sync_outside());
    assert!(app.items.iter().any(|i| i.title == "From outside"));
    assert!(app.cards.iter().any(|c| c.title == "Card from outside"));
    assert!(!app.sync_outside());
    drop(cli);
    drop(app);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn calendars_notes_and_zones() {
    let mut app = app();
    let today = app.today;
    assert_eq!(app.calendars.len(), 1);
    send(&mut app, Message::CalendarNew);
    send(&mut app, Message::CalendarSave);
    assert!(toast_is_error(&app), "a name is required");
    type_in(&mut app, Field::CalendarName, "personal");
    send(&mut app, Message::CalendarSave);
    assert!(matches!(app.modal, Modal::Calendar(_)), "the name is taken");
    type_in(&mut app, Field::CalendarName, "Work");
    type_in(&mut app, Field::CalendarTz, "Narnia");
    send(&mut app, Message::CalendarSave);
    assert_eq!(app.calendars.len(), 1, "an unknown zone is refused");
    type_in(&mut app, Field::CalendarTz, "eastern");
    send(&mut app, Message::CalendarColor(Some("#4a8fe7".into())));
    send(&mut app, Message::CalendarSave);
    let work = app.calendars[1].clone();
    assert_eq!(work.tz.as_deref(), Some("America/New_York"));
    assert_eq!(work.color.as_deref(), Some("#4a8fe7"));

    // An item put in that calendar takes its zone; a zone in the time wins.
    send(&mut app, Message::ItemNew(ItemKind::Appointment, today));
    type_in(&mut app, Field::ItemTitle, "Standup");
    send(&mut app, Message::ItemCalendar(work.clone()));
    type_in(&mut app, Field::ItemTime, "9:30am");
    send(&mut app, Message::ItemSave);
    let standup = app
        .items
        .iter()
        .find(|i| i.title == "Standup")
        .unwrap()
        .clone();
    assert_eq!(standup.calendar_id, work.id);
    assert_eq!(standup.tz.as_deref(), Some("America/New_York"));
    assert_eq!(standup.time, chrono::NaiveTime::from_hms_opt(9, 30, 0));
    send(&mut app, Message::ItemOpen(standup.id));
    type_in(&mut app, Field::ItemTime, "9:30 utc");
    send(&mut app, Message::ItemSave);
    assert_eq!(app.items[0].tz.as_deref(), Some("UTC"));
    send(&mut app, Message::ItemOpen(standup.id));
    type_in(&mut app, Field::ItemTz, "Mars");
    send(&mut app, Message::ItemSave);
    assert!(matches!(app.modal, Modal::Item(_)) && toast_is_error(&app));
    send(&mut app, Message::CloseModal);

    // A note keeps its text and has neither a time nor a zone.
    send(&mut app, Message::ItemNew(ItemKind::Note, today));
    type_in(&mut app, Field::ItemTitle, "Gate code");
    type_in(&mut app, Field::ItemTime, "9:00");
    send(
        &mut app,
        Message::ItemNotes(text_editor::Action::Edit(text_editor::Edit::Paste(
            std::sync::Arc::new("4411\nask for Sam".to_string()),
        ))),
    );
    send(&mut app, Message::ItemSave);
    let note = app.items.iter().find(|i| i.kind == ItemKind::Note).unwrap();
    assert_eq!(note.notes, "4411\nask for Sam");
    assert_eq!((note.time, note.tz.clone()), (None, None));

    // Hiding a calendar takes its items off the day; deleting removes them.
    send(&mut app, Message::ItemNew(ItemKind::Deadline, today));
    type_in(&mut app, Field::ItemTitle, "Ship it");
    send(&mut app, Message::ItemCalendar(work.clone()));
    send(&mut app, Message::ItemSave);
    let on_day = |app: &App| -> Vec<String> {
        app.day_items(today)
            .iter()
            .map(|o| o.item.title.clone())
            .collect()
    };
    assert!(on_day(&app).contains(&"Ship it".to_string()));
    send(&mut app, Message::CalendarVisible(work.id, false));
    assert_eq!(on_day(&app), ["Gate code"]);
    assert!(
        !app.db.calendars().unwrap()[1].visible,
        "and it is remembered"
    );
    send(&mut app, Message::CalendarVisible(work.id, true));
    send(&mut app, Message::CalendarOpen(work.id));
    send(
        &mut app,
        Message::Ask(ConfirmAction::DeleteCalendar(work.id)),
    );
    send(&mut app, Message::ConfirmYes);
    assert_eq!(app.calendars.len(), 1);
    assert!(app.items.iter().all(|i| i.calendar_id == DEFAULT_CALENDAR));
    assert_eq!(app.items.len(), 1);

    send(&mut app, Message::SetClock(crate::config::Clock::H12));
    let t = chrono::NaiveTime::from_hms_opt(21, 0, 0).unwrap();
    assert_eq!(app.time_text(t), "9:00 PM");
}

#[test]
fn vim_keys() {
    let mut app = app();
    let press = |app: &mut App, k: &str| {
        let (key, mods) = match k {
            "enter" => (Key::Named(Named::Enter), Modifiers::empty()),
            k if k.len() == 1 && k.chars().all(|c| c.is_ascii_uppercase()) => {
                (Key::Character(k.into()), Modifiers::SHIFT)
            }
            k => (Key::Character(k.into()), Modifiers::empty()),
        };
        send(app, Message::Key(key, mods, false));
    };
    // g then a letter goes to a view; a lone letter after that is not a chord.
    press(&mut app, "g");
    press(&mut app, "c");
    assert_eq!(app.view, View::Calendar);
    let today = app.today;
    press(&mut app, "l");
    press(&mut app, "j");
    assert_eq!(app.cal_selected, today + Duration::days(8));
    assert_eq!(app.cal_month, dates::month_start(app.cal_selected));
    press(&mut app, "k");
    press(&mut app, "h");
    assert_eq!(app.cal_selected, today);
    press(&mut app, "w");
    assert!(app.cal_week);
    press(&mut app, "m");
    press(&mut app, "L");
    assert_eq!(
        app.cal_month,
        dates::add_months(dates::month_start(today), 1)
    );
    press(&mut app, "t");
    press(&mut app, "n");
    assert!(matches!(&app.modal, Modal::Item(i) if i.kind == ItemKind::Note));
    press(&mut app, "j");
    assert_eq!(app.cal_selected, today, "keys do nothing behind a dialog");
    type_in(&mut app, Field::ItemTitle, "Remember");
    send(&mut app, Message::ItemSave);
    press(&mut app, "enter");
    assert!(matches!(&app.modal, Modal::Item(i) if i.title == "Remember"));
    send(&mut app, Message::CloseModal);
    // A letter typed into a field is just a letter.
    send(
        &mut app,
        Message::Key(Key::Character("l".into()), Modifiers::empty(), true),
    );
    assert_eq!(app.cal_selected, today);

    // Board: move between lists and cards, and move the card itself.
    let a = add_card(&mut app, ListKind::Normal, "a");
    let b = add_card(&mut app, ListKind::Normal, "b");
    let c = add_card(&mut app, ListKind::Progress, "c");
    press(&mut app, "g");
    press(&mut app, "b");
    assert_eq!(app.view, View::Board);
    press(&mut app, "j");
    assert_eq!(app.board_sel, Some(a));
    press(&mut app, "j");
    press(&mut app, "j");
    assert_eq!(app.board_sel, Some(b), "stops at the last card");
    press(&mut app, "K");
    assert_eq!(titles_in(&app, list(&app, ListKind::Normal)), ["b", "a"]);
    press(&mut app, "l");
    assert_eq!(app.board_sel, Some(c));
    press(&mut app, "l");
    assert_eq!(app.board_sel, None, "the Done list is empty");
    press(&mut app, "h");
    press(&mut app, "L");
    assert_eq!(titles_in(&app, list(&app, ListKind::Done)), ["c"]);
    assert!(app.db.card(c).unwrap().unwrap().completed_at.is_some());
    press(&mut app, "enter");
    assert!(matches!(&app.modal, Modal::Card(e) if e.id == c));
    send(&mut app, Message::CloseModal);
    press(&mut app, "o");
    assert_eq!(app.adding_to, Some(list(&app, ListKind::Done)));
    app.adding_to = None;

    // Habits.
    let read = new_habit(&mut app, "Read");
    press(&mut app, "g");
    press(&mut app, "h");
    press(&mut app, "j");
    assert_eq!(app.habit_sel, Some(read));
    press(&mut app, "x");
    assert_eq!(app.stored_entry(read, today).value, 1.0);
    press(&mut app, "x");
    assert_eq!(app.stored_entry(read, today).value, 0.0);

    // Palette, help, and the switch in the settings.
    press(&mut app, ":");
    assert!(matches!(app.modal, Modal::Palette { .. }));
    send(&mut app, Message::CloseModal);
    press(&mut app, "?");
    assert!(matches!(app.modal, Modal::Keys));
    send(&mut app, Message::CloseModal);
    send(&mut app, Message::Toggle(Toggle::Vim, false));
    press(&mut app, "g");
    press(&mut app, "t");
    assert_eq!(app.view, View::Habits, "switched off");
}

#[test]
fn commands_from_outside() {
    let mut app = app();
    send(
        &mut app,
        Message::External(Command::Add {
            title: "From CLI".into(),
            due: Some("+2d".into()),
            board: None,
        }),
    );
    let c = app.cards.iter().find(|c| c.title == "From CLI").unwrap();
    assert_eq!(c.due, Some(app.today + Duration::days(2)));

    send(
        &mut app,
        Message::External(Command::Add {
            title: "Odd".into(),
            due: Some("someday".into()),
            board: Some("nope".into()),
        }),
    );
    assert!(
        app.cards
            .iter()
            .any(|c| c.title == "Odd" && c.due.is_none())
    );
    assert!(toast_is_error(&app), "the unreadable date is reported");

    send(&mut app, Message::External(Command::Start));
    assert!(app.timer.is_running());
    send(&mut app, Message::External(Command::Pause));
    assert_eq!(app.last_status.state, "paused");
    send(&mut app, Message::External(Command::Abandon));
    assert_eq!(app.last_status.state, "idle");
    send(&mut app, Message::External(Command::Skip));
    assert_eq!(app.last_status.phase, "short");
}

#[test]
fn settings_fields_apply_only_when_valid() {
    let mut app = app();
    type_in(&mut app, Field::SetFocus, "50");
    assert_eq!(app.cfg.timer.focus_min, 50.0);
    assert_eq!(app.timer.cfg.focus_s, 3000);
    for bad in ["abc", "0", "-5", "100000", ""] {
        type_in(&mut app, Field::SetFocus, bad);
        assert_eq!(app.cfg.timer.focus_min, 50.0, "{bad:?}");
        assert_eq!(app.set.focus, bad, "the field still shows what was typed");
    }
    type_in(&mut app, Field::SetGoal, "0");
    assert_eq!(app.cfg.timer.daily_goal, 0);
    send(&mut app, Message::SetEveryN(0));
    assert_eq!(app.timer.cfg.long_every, 0);
    send(&mut app, Message::Toggle(Toggle::Flow, true));
    assert!(app.timer.cfg.flow);
    send(&mut app, Message::SetScale(1.234));
    assert_eq!(app.cfg.appearance.scale, 1.25);
}

#[test]
fn themes_and_overrides() {
    let mut app = app();
    let accent = app.look.c.accent;
    type_in(&mut app, Field::Override("accent"), "#ff880");
    assert_eq!(
        app.look.c.accent, accent,
        "half-typed colours are not applied"
    );
    type_in(&mut app, Field::Override("accent"), "#ff8800");
    assert_eq!(app.look.c.accent.to_hex(), "#ff8800");
    assert_eq!(app.look.c.focus.to_hex(), "#ff8800");
    send(&mut app, Message::OverrideClear("accent"));
    assert_eq!(app.look.c.accent, accent);

    send(&mut app, Message::SetTheme("paper".into()));
    assert!(!app.look.dark);
    assert_eq!(app.look.name, "paper");
    send(&mut app, Message::SetTheme("does-not-exist".into()));
    assert_eq!(app.look.name, "system");
    assert_eq!(app.look.problems.len(), 1);
    send(&mut app, Message::SetMode(config::Mode::Light));
    send(&mut app, Message::SetTheme("system".into()));
    assert!(!app.look.dark);
}

#[test]
fn wipe_leaves_a_usable_app() {
    let mut app = app();
    add_card(&mut app, ListKind::Normal, "a");
    new_habit(&mut app, "Read");
    send(&mut app, Message::Ask(ConfirmAction::WipeData));
    send(&mut app, Message::ConfirmYes);
    assert!(app.cards.is_empty() && app.habits.is_empty());
    assert_eq!(app.boards.len(), 1);
    assert_eq!(app.lists.len(), 3);
}

/// Builds every view and dialog at both layout widths. This catches panics
/// in view code (slicing, indexing, arithmetic) on empty and populated data.
#[test]
fn every_view_and_dialog_builds() {
    let w = window::Id::unique();
    let mut app = app();
    let all_views = |app: &mut App| {
        for size in [
            iced::Size::new(500.0, 600.0),
            iced::Size::new(1400.0, 900.0),
        ] {
            app.size = size;
            for week in [false, true] {
                app.cal_week = week;
                for v in View::ALL {
                    app.view = v;
                    for range in Range::ALL {
                        app.ana.range = range;
                        app.reload_analytics();
                        let _ = app.view(w);
                        if v != View::Analytics {
                            break;
                        }
                    }
                }
            }
            app.zen = true;
            let _ = app.view(w);
            app.zen = false;
        }
    };
    all_views(&mut app);

    // Populate, including awkward text.
    let card = add_card(
        &mut app,
        ListKind::Progress,
        "Ünïcödé 日本語 title that is rather long indeed",
    );
    let mut c = app.db.card(card).unwrap().unwrap();
    c.estimate = 2.0;
    c.due = Some(app.today - Duration::days(1));
    c.tags = "a,b,c,d,e".into();
    app.db.update_card(&c).unwrap();
    add_card(&mut app, ListKind::Done, "done");
    send(
        &mut app,
        Message::TimerCard(Choice {
            id: Some(card),
            label: String::new(),
        }),
    );
    type_in(&mut app, Field::ManualMin, "90");
    send(&mut app, Message::ManualAdd);
    let today = app.today;
    for kind in ItemKind::ALL {
        send(&mut app, Message::ItemNew(kind, today));
        type_in(&mut app, Field::ItemTitle, "Thing");
        type_in(&mut app, Field::ItemTime, "23:30");
        type_in(&mut app, Field::ItemDuration, "120");
        send(&mut app, Message::ItemSave);
    }
    let read = new_habit(&mut app, "Read");
    send(&mut app, Message::HabitCheck(read, today, true));
    send(&mut app, Message::HabitNew);
    type_in(&mut app, Field::HabitName, "Gym");
    send(&mut app, Message::HabitSched(SchedKind::PerWeek));
    send(&mut app, Message::HabitSave);
    all_views(&mut app);

    // Running, paused, flow and break states.
    send(&mut app, Message::TimerToggle);
    all_views(&mut app);
    send(&mut app, Message::TimerToggle);
    all_views(&mut app);
    send(&mut app, Message::TimerAbandon);
    send(&mut app, Message::Toggle(Toggle::Flow, true));
    send(&mut app, Message::TimerToggle);
    all_views(&mut app);
    send(&mut app, Message::TimerFinish);
    assert!(app.timer.phase().is_break());
    all_views(&mut app);

    // Dialogs.
    send(&mut app, Message::CardOpen(card));
    let _ = app.view(w);
    let item = app.items[0].id;
    send(&mut app, Message::ItemOpen(item));
    let _ = app.view(w);
    for kind in HabitKind::ALL {
        send(&mut app, Message::HabitOpen(read));
        send(&mut app, Message::HabitKindSet(kind));
        for s in SchedKind::ALL {
            send(&mut app, Message::HabitSched(s));
            let _ = app.view(w);
        }
    }
    let board = app.board_id.unwrap();
    send(
        &mut app,
        Message::PromptOpen(PromptAction::RenameBoard(board)),
    );
    let _ = app.view(w);
    send(&mut app, Message::Ask(ConfirmAction::WipeData));
    let _ = app.view(w);
    app.modal = Modal::Palette { query: "e".into() };
    let _ = app.view(w);
    app.modal = Modal::Keys;
    let _ = app.view(w);
    send(&mut app, Message::CalendarNew);
    type_in(&mut app, Field::CalendarName, "Work");
    type_in(&mut app, Field::CalendarTz, "Eastern");
    let _ = app.view(w);
    send(&mut app, Message::CalendarSave);
    let work = app.calendars[1].clone();
    send(&mut app, Message::CalendarOpen(work.id));
    let _ = app.view(w);
    // A zoned item in a coloured calendar, on a twelve-hour clock.
    send(&mut app, Message::ItemNew(ItemKind::Appointment, today));
    type_in(&mut app, Field::ItemTitle, "Zoned");
    type_in(&mut app, Field::ItemTime, "9pm Pacific");
    send(&mut app, Message::ItemCalendar(work));
    let _ = app.view(w);
    send(&mut app, Message::ItemSave);
    send(&mut app, Message::SetClock(crate::config::Clock::H12));
    app.board_sel = Some(card);
    app.habit_sel = Some(read);
    all_views(&mut app);
    app.modal = Modal::None;

    // A drag in progress and a toast.
    drag(&mut app, card);
    app.toast("Something happened", true);
    app.view = View::Board;
    let _ = app.view(w);
}
