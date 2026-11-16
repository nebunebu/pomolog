//! One JSON line in, one JSON line out, over a unix socket.

use crate::bridge::{self, Command, Status};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
}

/// Sends a command to a running instance. `Err` means none is running (or it
/// did not answer).
pub fn request(path: &Path, cmd: &Command) -> std::io::Result<Reply> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut line = serde_json::to_string(cmd)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(serde_json::from_str(&reply)?)
}

/// Starts listening on a background thread. A stale socket file left by a
/// crashed instance is replaced.
pub fn serve(path: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path)?;
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                // Each client is short-lived; handle it on its own thread so a
                // stuck one cannot block the rest.
                std::thread::spawn(move || handle(stream));
            }
        })?;
    Ok(())
}

fn handle(stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let mut line = String::new();
    let mut reader = BufReader::new(&stream);
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let reply = match serde_json::from_str::<Command>(&line) {
        Ok(Command::Status) => Reply {
            ok: true,
            error: None,
            status: Some(bridge::status()),
        },
        Ok(Command::Quit) => {
            // Answer first: the process may be gone before a later reply.
            let _ = (&stream).write_all(b"{\"ok\":true}\n");
            bridge::send(Command::Quit);
            return;
        }
        Ok(cmd) => {
            bridge::send(cmd);
            // Give the app a moment to act so the status reflects the command.
            std::thread::sleep(Duration::from_millis(120));
            Reply {
                ok: true,
                error: None,
                status: Some(bridge::status()),
            }
        }
        Err(e) => Reply {
            ok: false,
            error: Some(format!("bad request: {e}")),
            status: None,
        },
    };
    if let Ok(mut out) = serde_json::to_string(&reply) {
        out.push('\n');
        let _ = (&stream).write_all(out.as_bytes());
    }
}

pub fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_wire_format() {
        let c: Command = serde_json::from_str(r#"{"cmd":"toggle"}"#).unwrap();
        assert_eq!(c, Command::Toggle);
        let c: Command =
            serde_json::from_str(r#"{"cmd":"add","title":"Write","due":"fri"}"#).unwrap();
        assert_eq!(
            c,
            Command::Add {
                title: "Write".into(),
                due: Some("fri".into()),
                board: None
            }
        );
        assert!(serde_json::from_str::<Command>(r#"{"cmd":"explode"}"#).is_err());
    }

    #[test]
    fn serves_status_and_rejects_garbage() {
        let path = std::env::temp_dir().join(format!("pomolog-test-{}.sock", std::process::id()));
        serve(&path).unwrap();
        bridge::set_status(Status {
            state: "running".into(),
            text: "12:34".into(),
            ..Default::default()
        });
        let r = request(&path, &Command::Status).unwrap();
        assert!(r.ok);
        assert_eq!(r.status.unwrap().text, "12:34");

        let mut s = UnixStream::connect(&path).unwrap();
        s.write_all(b"not json\n").unwrap();
        let mut reply = String::new();
        BufReader::new(s).read_line(&mut reply).unwrap();
        let r: Reply = serde_json::from_str(&reply).unwrap();
        assert!(!r.ok && r.error.is_some());

        cleanup(&path);
        assert!(
            request(&path, &Command::Status).is_err(),
            "no instance, no answer"
        );
    }
}
