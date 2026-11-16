use clap::{Parser, Subcommand};
use pomolog::bridge::Command;
use pomolog::core::{dates, db::Db};
use pomolog::ui::App;
use pomolog::{config, ipc};
use std::process::ExitCode;

/// Pomodoro timer with kanban, calendar, habits and analytics.
///
/// Without a command this opens the window, or raises it if Pomolog is
/// already running. The commands control the running instance.
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
        /// 2026-12-01, today, tomorrow, a weekday name, +3d or +2w
        #[arg(long)]
        due: Option<String>,
        /// Board name; defaults to the board last opened
        #[arg(long)]
        board: Option<String>,
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
        Cmd::Add { title, due, board } => (Command::Add { title, due, board }, false),
    };
    // Catch mistakes here, where they can be reported, instead of in the app.
    if let Command::Add {
        due: Some(text), ..
    } = &command
        && dates::parse_date(text, chrono::Local::now().date_naive()).is_none()
    {
        eprintln!(
            "pomolog: could not read the due date \"{text}\" (try 2026-12-01, tomorrow, fri, +3d)"
        );
        return ExitCode::FAILURE;
    }
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
        Ok(reply) => {
            eprintln!(
                "pomolog: {}",
                reply.error.unwrap_or_else(|| "request failed".into())
            );
            ExitCode::FAILURE
        }
        // Not running. Adding a card still works, straight into the database.
        Err(_) => match command {
            Command::Add { title, due, board } => {
                add_offline(&title, due.as_deref(), board.as_deref())
            }
            Command::Status if json => {
                println!("{{\"state\":\"stopped\"}}");
                ExitCode::FAILURE
            }
            _ => {
                eprintln!("pomolog is not running");
                ExitCode::FAILURE
            }
        },
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

fn add_offline(title: &str, due: Option<&str>, board: Option<&str>) -> ExitCode {
    let run = || -> Result<String, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("the title is empty".into());
        }
        let now = chrono::Local::now().naive_local();
        let due = match due {
            Some(text) => Some(
                dates::parse_date(text, now.date())
                    .ok_or_else(|| format!("could not read the due date \"{text}\""))?,
            ),
            None => None,
        };
        let db = Db::open(&config::db_path()).map_err(|e| e.to_string())?;
        let mut boards = db.boards().map_err(|e| e.to_string())?;
        if boards.is_empty() {
            db.add_board("Tasks").map_err(|e| e.to_string())?;
            boards = db.boards().map_err(|e| e.to_string())?;
        }
        let chosen = match board {
            Some(name) => boards
                .iter()
                .find(|b| b.name.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| format!("there is no board called \"{name}\""))?,
            None => {
                let last: Option<i64> = db.meta("board").and_then(|v| v.parse().ok());
                boards
                    .iter()
                    .find(|b| Some(b.id) == last)
                    .unwrap_or(&boards[0])
            }
        };
        let list = db.lists(chosen.id).map_err(|e| e.to_string())?;
        let list = list.first().ok_or("the board has no lists")?;
        let id = db
            .add_card(list.id, title, now)
            .map_err(|e| e.to_string())?;
        if let (Some(d), Ok(Some(mut card))) = (due, db.card(id)) {
            card.due = Some(d);
            db.update_card(&card).map_err(|e| e.to_string())?;
        }
        Ok(format!("added to {} / {}", chosen.name, list.name))
    };
    match run() {
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
