//! System tray icon (StatusNotifierItem): a progress ring, the countdown in
//! the tooltip, and a small menu.

use crate::bridge::{self, Command, Status};
use crate::config::{APP, APP_TITLE};
use crate::theme::Rgb;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip};

#[derive(Debug, Clone, PartialEq)]
pub struct TrayColors {
    pub focus: Rgb,
    pub short_break: Rgb,
    pub long_break: Rgb,
    pub idle: Rgb,
}

pub struct PomoTray {
    pub status: Status,
    pub colors: TrayColors,
}

impl PomoTray {
    fn color(&self) -> Rgb {
        match (self.status.state.as_str(), self.status.phase.as_str()) {
            ("idle", _) => self.colors.idle,
            (_, "short") => self.colors.short_break,
            (_, "long") => self.colors.long_break,
            _ => self.colors.focus,
        }
    }

    fn describe(&self) -> String {
        let s = &self.status;
        let phase = match s.phase.as_str() {
            "short" => "Short break",
            "long" => "Long break",
            _ => "Focus",
        };
        match s.state.as_str() {
            "running" => format!("{phase} · {}", s.text),
            "paused" => format!("{phase} paused · {}", s.text),
            _ => format!("Ready · {phase} next"),
        }
    }
}

impl ksni::Tray for PomoTray {
    fn id(&self) -> String {
        APP.into()
    }

    fn title(&self) -> String {
        format!("{APP_TITLE} — {}", self.describe())
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let s = &self.status;
        let progress = if s.state == "idle" { 0.0 } else { s.progress };
        [22, 32, 48]
            .into_iter()
            .map(|size| Icon {
                width: size,
                height: size,
                data: ring_icon(size as usize, progress, self.color(), s.state == "paused"),
            })
            .collect()
    }

    fn tool_tip(&self) -> ToolTip {
        let s = &self.status;
        let mut lines = Vec::new();
        if let Some(task) = &s.task {
            lines.push(task.clone());
        }
        if s.daily_goal > 0 {
            lines.push(format!(
                "{} of {} sessions today",
                s.completed_today, s.daily_goal
            ));
        } else {
            lines.push(format!("{} sessions today", s.completed_today));
        }
        ToolTip {
            icon_name: String::new(),
            icon_pixmap: Vec::new(),
            title: self.describe(),
            description: lines.join("\n"),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        bridge::send(Command::Show { view: None });
    }

    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        bridge::send(Command::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let item = |label: &str, cmd: Command, enabled: bool| -> MenuItem<Self> {
            StandardItem {
                label: label.into(),
                enabled,
                activate: Box::new(move |_| bridge::send(cmd.clone())),
                ..Default::default()
            }
            .into()
        };
        let active = self.status.state != "idle";
        let toggle = match self.status.state.as_str() {
            "running" => "Pause",
            "paused" => "Resume",
            _ => "Start",
        };
        vec![
            item(toggle, Command::Toggle, true),
            item("Skip", Command::Skip, true),
            item("Abandon", Command::Abandon, active),
            MenuItem::Separator,
            item("Show window", Command::Show { view: None }, true),
            item("Quit", Command::Quit, true),
        ]
    }
}

/// Starts the tray. Returns None when no tray host is available.
pub fn spawn(colors: TrayColors) -> Option<Handle<PomoTray>> {
    PomoTray {
        status: bridge::status(),
        colors,
    }
    .spawn()
    .ok()
}

/// A ring with `progress` (0..=1) drawn clockwise from the top, as ARGB32
/// in network byte order.
pub fn ring_icon(size: usize, progress: f32, color: Rgb, paused: bool) -> Vec<u8> {
    const SS: usize = 3; // supersampling per axis
    let c = size as f32 / 2.0;
    let outer = c - size as f32 * 0.06;
    let inner = outer - (size as f32 * 0.17).max(2.5);
    let sweep = progress.clamp(0.0, 1.0) * std::f32::consts::TAU;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            // Coverage of the lit arc and of the dim track.
            let (mut arc, mut track) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32 - c;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32 - c;
                    let r = (px * px + py * py).sqrt();
                    if r < inner || r > outer {
                        continue;
                    }
                    // Angle from 12 o'clock, clockwise.
                    let a = px.atan2(-py).rem_euclid(std::f32::consts::TAU);
                    if a <= sweep && sweep > 0.0 {
                        arc += 1;
                    } else {
                        track += 1;
                    }
                }
            }
            let n = (SS * SS) as f32;
            let lit = if paused { 0.55 } else { 1.0 };
            let alpha = (arc as f32 / n) * lit + (track as f32 / n) * 0.28;
            let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            data.extend_from_slice(&[ch(alpha), ch(color.r), ch(color.g), ch(color.b)]);
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha_at(data: &[u8], size: usize, x: usize, y: usize) -> u8 {
        data[(y * size + x) * 4]
    }

    #[test]
    fn ring_shape_and_progress() {
        let size = 32;
        let red = Rgb::new(1.0, 0.0, 0.0);
        let quarter = ring_icon(size, 0.25, red, false);
        assert_eq!(quarter.len(), size * size * 4);
        assert_eq!(alpha_at(&quarter, size, 16, 16), 0, "centre is empty");
        assert_eq!(alpha_at(&quarter, size, 0, 0), 0, "corner is empty");
        // Just right of 12 o'clock is lit, 6 o'clock is only track.
        assert_eq!(alpha_at(&quarter, size, 18, 4), 255);
        let dim = alpha_at(&quarter, size, 16, 27);
        assert!(dim > 40 && dim < 100, "track alpha was {dim}");
        assert_eq!(
            &quarter[(4 * size + 18) * 4 + 1..(4 * size + 18) * 4 + 4],
            &[255, 0, 0]
        );

        let empty = ring_icon(size, 0.0, red, false);
        assert!(
            alpha_at(&empty, size, 18, 4) < 100,
            "no progress, no lit arc"
        );
        let full = ring_icon(size, 1.0, red, false);
        assert_eq!(alpha_at(&full, size, 16, 27), 255);
        let paused = ring_icon(size, 1.0, red, true);
        assert!(alpha_at(&paused, size, 16, 27) < 200);
    }
}
