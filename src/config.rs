use crate::core::timer::TimerCfg;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const APP: &str = "pomolog";
pub const APP_TITLE: &str = "Pomolog";

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(fallback)
        })
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Where the GTK and Qt configuration lives. `POMOLOG_SYSTEM_CONFIG_HOME`
/// redirects it, which is useful for trying a theme without touching the
/// real one.
pub fn system_config_home() -> PathBuf {
    env_dir("POMOLOG_SYSTEM_CONFIG_HOME").unwrap_or_else(|| xdg("XDG_CONFIG_HOME", ".config"))
}

pub fn config_dir() -> PathBuf {
    env_dir("POMOLOG_CONFIG_DIR").unwrap_or_else(|| xdg("XDG_CONFIG_HOME", ".config").join(APP))
}

pub fn data_dir() -> PathBuf {
    env_dir("POMOLOG_DATA_DIR").unwrap_or_else(|| xdg("XDG_DATA_HOME", ".local/share").join(APP))
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn themes_dir() -> PathBuf {
    config_dir().join("themes")
}

pub fn db_path() -> PathBuf {
    data_dir().join("pomolog.db")
}

pub fn socket_path() -> PathBuf {
    env_dir("POMOLOG_SOCKET").unwrap_or_else(|| {
        env_dir("XDG_RUNTIME_DIR")
            .unwrap_or_else(std::env::temp_dir)
            .join("pomolog.sock")
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Follow the desktop; dark when it states no preference.
    #[default]
    System,
    Dark,
    Light,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::System, Mode::Dark, Mode::Light];
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Mode::System => "Follow system",
            Mode::Dark => "Dark",
            Mode::Light => "Light",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// GTK first, then Qt.
    #[default]
    Auto,
    Gtk,
    Qt,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::Auto, Source::Gtk, Source::Qt];
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Source::Auto => "Automatic",
            Source::Gtk => "GTK",
            Source::Qt => "Qt",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimerConf {
    pub focus_min: f64,
    pub short_break_min: f64,
    pub long_break_min: f64,
    pub long_break_every: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,
    /// Completed focus sessions to aim for each day. 0 turns the goal off.
    pub daily_goal: u32,
    pub flow: bool,
    pub flow_break_ratio: f64,
}

impl Default for TimerConf {
    fn default() -> Self {
        TimerConf {
            focus_min: 25.0,
            short_break_min: 5.0,
            long_break_min: 15.0,
            long_break_every: 4,
            auto_start_breaks: true,
            auto_start_focus: false,
            daily_goal: 8,
            flow: false,
            flow_break_ratio: 0.2,
        }
    }
}

impl TimerConf {
    pub fn to_cfg(&self) -> TimerCfg {
        let secs = |m: f64| ((m * 60.0).round() as i64).max(1);
        TimerCfg {
            focus_s: secs(self.focus_min),
            short_s: secs(self.short_break_min),
            long_s: secs(self.long_break_min),
            long_every: self.long_break_every,
            auto_start_breaks: self.auto_start_breaks,
            auto_start_focus: self.auto_start_focus,
            flow: self.flow,
            flow_break_ratio: self.flow_break_ratio.clamp(0.05, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// "system", a bundled preset, or the name of a file in the themes folder.
    pub theme: String,
    pub mode: Mode,
    pub source: Source,
    /// Font family. Empty means the system font.
    pub font: String,
    /// Corner radius in pixels. Unset means the theme's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    pub compact: bool,
    /// Size of the whole interface: 1.0 is normal.
    pub scale: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            theme: "system".into(),
            mode: Mode::System,
            source: Source::Auto,
            font: String::new(),
            radius: None,
            compact: false,
            scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notifications {
    pub enabled: bool,
    /// Summarise today's items when the app starts or the day changes.
    pub daily_summary: bool,
    /// Hour (0-23) to warn about habits still open. 24 or more turns it off.
    pub habit_reminder_hour: u32,
    /// Minutes of warning before a timed appointment. 0 turns it off.
    pub appointment_lead_min: u32,
}

impl Default for Notifications {
    fn default() -> Self {
        Notifications {
            enabled: true,
            daily_summary: true,
            habit_reminder_hour: 20,
            appointment_lead_min: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub timer: TimerConf,
    pub appearance: Appearance,
    pub notifications: Notifications,
    /// Colour overrides applied on top of the chosen theme, e.g.
    /// `accent = "#ff8800"`.
    pub overrides: BTreeMap<String, String>,
}

impl Config {
    /// A missing file gives the defaults. A broken file also gives the
    /// defaults, along with the reason, so the app still starts.
    pub fn load() -> (Config, Option<String>) {
        match std::fs::read_to_string(config_path()) {
            Ok(text) => match toml::from_str(&text) {
                Ok(c) => (c, None),
                Err(e) => (
                    Config::default(),
                    Some(format!("config.toml could not be read: {e}")),
                ),
            },
            Err(_) => (Config::default(), None),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        // A config managed by home-manager is a read-only link into the store.
        std::fs::write(&path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_partial_files_use_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c, Config::default());
        let c: Config = toml::from_str(
            "[timer]\nfocus_min = 50\n[appearance]\nmode = \"light\"\nsource = \"qt\"\n[overrides]\naccent = \"#ff8800\"\n",
        )
        .unwrap();
        assert_eq!(c.timer.focus_min, 50.0);
        assert_eq!(c.timer.short_break_min, 5.0);
        assert_eq!(
            (c.appearance.mode, c.appearance.source),
            (Mode::Light, Source::Qt)
        );
        assert_eq!(c.overrides["accent"], "#ff8800");
    }

    #[test]
    fn round_trips_through_toml() {
        let mut c = Config::default();
        c.appearance.radius = Some(4.0);
        c.overrides.insert("focus".into(), "#112233".into());
        let text = toml::to_string_pretty(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), c);
    }

    #[test]
    fn timer_conversion() {
        let t = TimerConf {
            focus_min: 0.1,
            flow_break_ratio: 9.0,
            ..Default::default()
        }
        .to_cfg();
        assert_eq!(t.focus_s, 6);
        assert_eq!(t.flow_break_ratio, 1.0);
    }
}
