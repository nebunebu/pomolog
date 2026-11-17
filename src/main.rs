use clap::{Args, Parser, Subcommand};
use pomolog::bridge::Command;
use pomolog::core::{db::Db, model::ItemText};
use pomolog::ui::App;
use pomolog::{cli, config, ipc};
use std::process::ExitCode;

/// Pomodoro timer with kanban, calendar, habits and analytics.
///
/// Without a command this opens the window, or raises it if Pomolog is
/// already running. The timer commands control the running instance; the
/// commands that add or change data work whether or not it is running.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start, pause or resume the timer
    Toggle,
    /// Start the timer (or resume it)
    Start,
    /// Pause the timer
    Pause,
    /// End the current phase and move to the next
    Skip,
    /// Give up on the current focus session
    Abandon,
    /// Show the window, optionally on a given view
    Show {
        /// today, timer, board, calendar, habits, analytics or settings
        view: Option<String>,
    },
    /// Quit the running instance
    Quit,
    /// Print the timer state
    Status {
        /// Machine-readable output, for bars and scripts
        #[arg(long)]
        json: bool,
    },
    /// Add a card to the first list of a board
    Add {
        title: String,
        /// 2026-12-01, today, tomorrow, a weekday name, +3d, +2w or Jan 5
        #[arg(long)]
        due: Option<String>,
        /// Board name; defaults to the board last opened
        #[arg(long)]
        board: Option<String>,
        /// A tag for the card; can be given more than once
        #[arg(long = "tag")]
        tags: Vec<String>,
    },
    /// Add anything from one line of text
    ///
    /// A time makes an appointment ("Dentist fri 2pm for 45m"), a date alone
    /// a deadline ("Tax return Apr 15"), "due" or "by" a card with a due date
    /// ("Report due fri @work #writing"), "note:" a calendar note, and
    /// anything else a card. @name picks the board or the calendar.
    In {
        #[arg(required = true)]
        text: Vec<String>,
    },
    /// Add a calendar item, or list, change, remove and import them
    Event(Box<EventArgs>),
    /// Add a note to a day of the calendar
    Note {
        title: String,
        /// Defaults to today
        #[arg(long)]
        date: Option<String>,
        /// The text of the note
        #[arg(long)]
        body: Option<String>,
        /// Calendar name; defaults to the first calendar
        #[arg(long)]
        calendar: Option<String>,
    },
    /// List, add and remove calendars
    Calendar {
        #[command(subcommand)]
        cmd: CalendarCmd,
    },
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
struct EventArgs {
    #[command(subcommand)]
    cmd: Option<EventCmd>,
    /// What to call the new item
    title: Option<String>,
    #[command(flatten)]
    fields: ItemFields,
    /// Add it even if the same item is already there
    #[arg(long)]
    allow_duplicate: bool,
}

/// The fields of a calendar item, as text.
#[derive(Args, Default)]
struct ItemFields {
    /// 2026-12-01, tomorrow, a weekday name, +3d, +2w, Jan 5 or 1/5 (month/day)
    #[arg(long)]
    date: Option<String>,
    /// 21:00, 9pm or 9:30 PM, optionally with a zone: "9:00 PM Eastern"
    #[arg(long)]
    time: Option<String>,
    /// Length in minutes
    #[arg(long)]
    duration: Option<i64>,
    /// appointment (default), deadline, holiday or note
    #[arg(long)]
    kind: Option<String>,
    /// none, daily, weekly, monthly or yearly
    #[arg(long)]
    repeat: Option<String>,
    /// Last day of a repeating item
    #[arg(long)]
    until: Option<String>,
    /// Description shown with the item
    #[arg(long)]
    notes: Option<String>,
    /// Calendar name; defaults to the first calendar
    #[arg(long)]
    calendar: Option<String>,
    /// Zone the time is in: America/New_York, Eastern, UTC. Local if left out
    #[arg(long)]
    tz: Option<String>,
    /// Your own id for the item: adding it again updates it instead
    #[arg(long)]
    source: Option<String>,
}

impl ItemFields {
    fn text(self, title: Option<String>) -> ItemText {
        ItemText {
            title: title.unwrap_or_default(),
            date: self.date.unwrap_or_default(),
            time: self.time,
            duration: self.duration,
            kind: self.kind,
            recur: self.repeat,
            until: self.until,
            notes: self.notes,
            calendar: self.calendar,
            tz: self.tz,
            source: self.source,
        }
    }
}

#[derive(Subcommand)]
enum EventCmd {
    /// List items, one line per occurrence, with their ids
    List {
        /// First day; defaults to today
        #[arg(long)]
        from: Option<String>,
        /// Last day; defaults to 30 days after the first
        #[arg(long)]
        to: Option<String>,
        /// Only this calendar
        #[arg(long)]
        calendar: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Change an item; only the fields given are changed ("none" clears one)
    Edit {
        /// The id shown by `event list`
        id: i64,
        #[arg(long)]
        title: Option<String>,
        #[command(flatten)]
        fields: ItemFields,
    },
    /// Remove an item (and all its repeats)
    Rm {
        /// The id shown by `event list`
        id: i64,
    },
    /// Add many items from a file, or from standard input with "-"
    ///
    /// The file is an .ics calendar, or one JSON object per line with the
    /// same fields as the options here: {"title": "Club", "date": "Jan 5"}.
    /// The options given set what an entry leaves out. Importing the same
    /// file again adds nothing.
    Import {
        file: String,
        #[command(flatten)]
        defaults: ItemFields,
        /// Add entries even if the same item is already there
        #[arg(long)]
        allow_duplicate: bool,
    },
}

#[derive(Subcommand)]
enum CalendarCmd {
    /// List the calendars
    List,
    /// Add a calendar
    Add {
        name: String,
        /// A colour for its items, e.g. "#4a8fe7"
        #[arg(long)]
        color: Option<String>,
        /// Zone that new items in it are written in
        #[arg(long)]
        tz: Option<String>,
    },
    /// Remove a calendar
    Rm {
        name: String,
        /// Remove it even if it still has items; they go with it
        #[arg(long)]
        force: bool,
    },
}

const VIEWS: [&str; 7] = [
    "today",
    "timer",
    "board",
    "calendar",
    "habits",
    "analytics",
    "settings",
];

fn main() -> ExitCode {
    let cli = Cli::parse();
    let socket = config::socket_path();
    let Some(cmd) = cli.cmd else {
        return launch(&socket);
    };
    let (command, json) = match cmd {
        Cmd::Toggle => (Command::Toggle, false),
        Cmd::Start => (Command::Start, false),
        Cmd::Pause => (Command::Pause, false),
        Cmd::Skip => (Command::Skip, false),
        Cmd::Abandon => (Command::Abandon, false),
        Cmd::Show { view } => (Command::Show { view }, false),
        Cmd::Quit => (Command::Quit, false),
        Cmd::Status { json } => (Command::Status, json),
        data => return report(data_command(data)),
    };
    if let Command::Show { view: Some(name) } = &command
        && !VIEWS.iter().any(|v| v.eq_ignore_ascii_case(name.trim()))
    {
        eprintln!(
            "pomolog: there is no view called \"{name}\" (views: {})",
            VIEWS.join(", ")
        );
        return ExitCode::FAILURE;
    }
    match ipc::request(&socket, &command) {
        Ok(reply) if reply.ok => {
            match (&command, reply.status) {
                (Command::Status, Some(s)) if json => {
                    println!("{}", serde_json::to_string(&s).unwrap_or_default());
                }
                (Command::Status, Some(s)) => {
                    let task = s.task.map(|t| format!(" · {t}")).unwrap_or_default();
                    println!("{} {} ({}){task}", s.phase, s.text, s.state);
                }
                _ => {}
            }
            ExitCode::SUCCESS
        }
        Ok(reply) if reply.unknown_command() => {
            eprintln!(
                "pomolog: the running Pomolog is older than this command and does not know it; \
                 restart it (`pomolog quit`, then `pomolog`) to use it"
            );
            ExitCode::FAILURE
        }
        Ok(reply) => {
            eprintln!(
                "pomolog: {}",
                reply.error.unwrap_or_else(|| "request failed".into())
            );
            ExitCode::FAILURE
        }
        Err(_) => {
            if matches!(command, Command::Status) && json {
                println!("{{\"state\":\"stopped\"}}");
            } else {
                eprintln!("pomolog is not running");
            }
            ExitCode::FAILURE
        }
    }
}

fn report(result: Result<String, String>) -> ExitCode {
    match result {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("pomolog: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The commands that read or change data. They go to the database directly;
/// a running instance picks the change up on its own.
fn data_command(cmd: Cmd) -> Result<String, String> {
    let mut db = Db::open(&config::db_path())
        .map_err(|e| format!("could not open {}: {e}", config::db_path().display()))?;
    let now = chrono::Local::now().naive_local();
    let today = now.date();
    let twelve = config::Config::load().0.appearance.clock == config::Clock::H12;
    match cmd {
        Cmd::Add {
            title,
            due,
            board,
            tags,
        } => cli::add_card(&db, &title, due.as_deref(), board.as_deref(), &tags, now),
        Cmd::In { text } => cli::intake_line(&db, &text.join(" "), now, twelve),
        Cmd::Note {
            title,
            date,
            body,
            calendar,
        } => {
            let text = ItemText {
                title,
                date: date.unwrap_or_default(),
                kind: Some("note".into()),
                notes: body,
                calendar,
                ..Default::default()
            };
            cli::add_event(&db, &text, false, today, twelve)
        }
        Cmd::Event(args) => match args.cmd {
            None => {
                let Some(title) = args.title else {
                    return Err(
                        "give a title to add an item, or one of: list, edit, rm, import \
                         (see `pomolog event --help`)"
                            .into(),
                    );
                };
                let text = args.fields.text(Some(title));
                cli::add_event(&db, &text, args.allow_duplicate, today, twelve)
            }
            Some(EventCmd::List {
                from,
                to,
                calendar,
                json,
            }) => cli::list_events(
                &db,
                from.as_deref(),
                to.as_deref(),
                calendar.as_deref(),
                json,
                today,
                twelve,
            ),
            Some(EventCmd::Edit { id, title, fields }) => {
                cli::edit_event(&db, id, &fields.text(title), today, twelve)
            }
            Some(EventCmd::Rm { id }) => cli::remove_event(&db, id),
            Some(EventCmd::Import {
                file,
                defaults,
                allow_duplicate,
            }) => {
                let text = if file == "-" {
                    std::io::read_to_string(std::io::stdin())
                        .map_err(|e| format!("could not read standard input: {e}"))?
                } else {
                    std::fs::read_to_string(&file)
                        .map_err(|e| format!("could not read {file}: {e}"))?
                };
                cli::import_events(&db, &text, &defaults.text(None), allow_duplicate, today)
            }
        },
        Cmd::Calendar { cmd } => match cmd {
            CalendarCmd::List => cli::list_calendars(&db),
            CalendarCmd::Add { name, color, tz } => {
                cli::add_calendar(&db, &name, color.as_deref(), tz.as_deref())
            }
            CalendarCmd::Rm { name, force } => cli::remove_calendar(&mut db, &name, force),
        },
        // The timer and window commands are handled by the caller.
        _ => Err("that command needs the running app".into()),
    }
}

fn launch(socket: &std::path::Path) -> ExitCode {
    if ipc::request(socket, &Command::Show { view: None }).is_ok() {
        eprintln!("pomolog is already running; showing its window");
        return ExitCode::SUCCESS;
    }
    if let Err(e) = ipc::serve(socket) {
        eprintln!(
            "pomolog: command socket unavailable ({e}); the CLI will not reach this instance"
        );
    }
    let daemon = iced::daemon(App::boot, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .scale_factor(App::scale)
        .antialiasing(true);
    let daemon = match App::startup_font() {
        Some(font) => daemon.default_font(font),
        None => daemon,
    };
    let result = daemon.run();
    ipc::cleanup(socket);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pomolog: {e}");
            ExitCode::FAILURE
        }
    }
}
