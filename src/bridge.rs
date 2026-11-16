//! The link between the GUI and the things outside it: the tray and the
//! command-line socket. They send commands in and read a status snapshot out.

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Command {
    Toggle,
    Start,
    Pause,
    Skip,
    Abandon,
    Show {
        /// Optionally switch to a view: today, timer, board, calendar,
        /// habits, analytics or settings.
        #[serde(default)]
        view: Option<String>,
    },
    Quit,
    Status,
    Add {
        title: String,
        #[serde(default)]
        due: Option<String>,
        #[serde(default)]
        board: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Status {
    /// "idle", "running" or "paused".
    pub state: String,
    /// "focus", "short" or "long".
    pub phase: String,
    pub remaining_s: i64,
    pub elapsed_s: i64,
    pub planned_s: i64,
    pub progress: f32,
    pub open_ended: bool,
    pub task: Option<String>,
    pub completed_today: u32,
    pub daily_goal: u32,
    /// The time as shown in the app, e.g. "24:59".
    pub text: String,
}

struct Channel {
    tx: UnboundedSender<Command>,
    rx: Mutex<Option<UnboundedReceiver<Command>>>,
}

fn channel() -> &'static Channel {
    static CH: OnceLock<Channel> = OnceLock::new();
    CH.get_or_init(|| {
        let (tx, rx) = unbounded_channel();
        Channel {
            tx,
            rx: Mutex::new(Some(rx)),
        }
    })
}

pub fn send(cmd: Command) {
    let _ = channel().tx.send(cmd);
}

/// The receiving end. Only the first caller gets it.
pub fn take_receiver() -> Option<UnboundedReceiver<Command>> {
    channel().rx.lock().ok()?.take()
}

static STATUS: Mutex<Option<Status>> = Mutex::new(None);

pub fn set_status(s: Status) {
    if let Ok(mut g) = STATUS.lock() {
        *g = Some(s);
    }
}

pub fn status() -> Status {
    STATUS
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}
