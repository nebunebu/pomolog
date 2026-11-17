//! Short audio cues for timer events. A single player thread owns the audio
//! device and plays cues one after another, so a phase ending and the next
//! one auto-starting are heard as two separate sounds. A missing or busy
//! device never stalls the interface: cues are simply dropped.

use crate::config::Sounds;
use rodio::{Decoder, OutputStreamBuilder, Sink, Source, buffer::SamplesBuffer};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    FocusEnd,
    BreakEnd,
    AutoStart,
    Warning,
}

struct Request {
    cue: Cue,
    file: Option<PathBuf>,
    volume: f32,
}

const RATE: u32 = 44_100;
/// Custom files are cut off after this long so a stray album does not play.
const MAX_FILE: Duration = Duration::from_secs(10);

/// Plays `cue` if sounds and that cue are switched on.
pub fn play(cue: Cue, cfg: &Sounds) {
    if !cfg.enabled {
        return;
    }
    let (on, file) = match cue {
        Cue::FocusEnd => (cfg.focus_end, &cfg.focus_end_file),
        Cue::BreakEnd => (cfg.break_end, &cfg.break_end_file),
        Cue::AutoStart => (cfg.auto_start, &cfg.auto_start_file),
        Cue::Warning => (cfg.minute_warning, &cfg.minute_warning_file),
    };
    if on {
        preview(cue, file, cfg.volume);
    }
}

/// Plays `cue` regardless of the on/off switches (for the settings test button).
pub fn preview(cue: Cue, file: &str, volume: f32) {
    let file = (!file.trim().is_empty()).then(|| expand(file.trim()));
    if let Some(tx) = player() {
        let _ = tx.send(Request {
            cue,
            file,
            volume: volume.clamp(0.0, 1.0),
        });
    }
}

fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(rest))
            .unwrap_or_else(|| PathBuf::from(path)),
        None => PathBuf::from(path),
    }
}

fn player() -> Option<&'static Sender<Request>> {
    static PLAYER: OnceLock<Option<Sender<Request>>> = OnceLock::new();
    PLAYER
        .get_or_init(|| {
            let (tx, rx) = channel::<Request>();
            std::thread::Builder::new()
                .name("pomolog-sound".into())
                .spawn(move || {
                    let mut stream = match OutputStreamBuilder::open_default_stream() {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("pomolog: no audio output, sounds disabled: {e}");
                            // Keep draining so senders never notice.
                            for _ in rx {}
                            return;
                        }
                    };
                    stream.log_on_drop(false);
                    let sink = Sink::connect_new(stream.mixer());
                    for req in rx {
                        if let Some(src) = req.file.as_deref().and_then(decode) {
                            sink.append(src.take_duration(MAX_FILE).amplify(req.volume));
                        } else {
                            sink.append(chime(req.cue).amplify(req.volume));
                        }
                    }
                })
                .ok()
                .map(|_| tx)
        })
        .as_ref()
}

fn decode(path: &std::path::Path) -> Option<Decoder<std::io::BufReader<std::fs::File>>> {
    let file = std::fs::File::open(path)
        .map_err(|e| eprintln!("pomolog: sound file {}: {e}", path.display()))
        .ok()?;
    Decoder::try_from(file)
        .map_err(|e| eprintln!("pomolog: sound file {}: {e}", path.display()))
        .ok()
}

/// A note: frequency (Hz), start (s), length (s), loudness.
type Note = (f32, f32, f32, f32);

fn notes(cue: Cue) -> &'static [Note] {
    const C5: f32 = 523.25;
    const E5: f32 = 659.25;
    const G5: f32 = 783.99;
    const C6: f32 = 1046.5;
    match cue {
        // Rising: done, take a break.
        Cue::FocusEnd => &[
            (C5, 0.00, 0.9, 0.5),
            (E5, 0.16, 0.9, 0.5),
            (G5, 0.32, 1.2, 0.5),
        ],
        // Falling: back to work.
        Cue::BreakEnd => &[(G5, 0.00, 0.9, 0.5), (C5, 0.20, 1.2, 0.5)],
        Cue::AutoStart => &[(C6, 0.00, 0.35, 0.3)],
        Cue::Warning => &[(E5, 0.00, 0.12, 0.2), (E5, 0.18, 0.12, 0.2)],
    }
}

/// Renders a cue into a mono buffer: softly struck bell-ish tones with a
/// short attack and exponential decay, plus a little second harmonic.
fn render(cue: Cue) -> Vec<f32> {
    let notes = notes(cue);
    let total = notes.iter().map(|n| n.1 + n.2).fold(0.0, f32::max) + 0.05;
    let mut buf = vec![0.0f32; (total * RATE as f32) as usize];
    let tau = std::f32::consts::TAU;
    for &(freq, start, len, gain) in notes {
        let s0 = (start * RATE as f32) as usize;
        let n = (len * RATE as f32) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.005).min(1.0);
            // Decay to about -40 dB by the end of the note, then a tiny fade.
            let decay = (-4.6 * t / len).exp();
            let tail = ((n - i) as f32 / (0.01 * RATE as f32)).min(1.0);
            let wave = (tau * freq * t).sin() + 0.25 * (tau * 2.0 * freq * t).sin();
            if let Some(s) = buf.get_mut(s0 + i) {
                *s += gain * attack * decay * tail * wave / 1.25;
            }
        }
    }
    for s in &mut buf {
        *s = s.clamp(-1.0, 1.0);
    }
    buf
}

fn chime(cue: Cue) -> SamplesBuffer {
    SamplesBuffer::new(1, RATE, render(cue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chimes_are_short_and_in_range() {
        for cue in [Cue::FocusEnd, Cue::BreakEnd, Cue::AutoStart, Cue::Warning] {
            let b = render(cue);
            assert!(!b.is_empty());
            assert!(b.len() < RATE as usize * 3);
            assert!(b.iter().all(|s| s.abs() <= 1.0));
            assert!(b.iter().any(|s| s.abs() > 0.05));
        }
    }

    #[test]
    fn tilde_expands() {
        if let Some(h) = std::env::var_os("HOME") {
            assert_eq!(expand("~/a.ogg"), PathBuf::from(h).join("a.ogg"));
        }
        assert_eq!(expand("/x.wav"), PathBuf::from("/x.wav"));
    }
}
