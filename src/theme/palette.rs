//! Turns system colours, theme files and overrides into the palette the UI
//! draws with.

use super::color::Rgb;
use super::gtk::GtkTheme;
use super::portal::Scheme;
use super::qt::QtTheme;
use crate::config::{Appearance, Mode, Source};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

macro_rules! colors {
    ($($(#[$doc:meta])* $f:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Colors { $($(#[$doc])* pub $f: Rgb),* }

        impl Colors {
            pub const KEYS: &'static [&'static str] = &[$(stringify!($f)),*];

            pub fn get(&self, key: &str) -> Option<Rgb> {
                match key { $(stringify!($f) => Some(self.$f),)* _ => None }
            }

            pub fn set(&mut self, key: &str, value: Rgb) -> bool {
                match key { $(stringify!($f) => { self.$f = value; true })* _ => false }
            }
        }
    };
}

colors! {
    /// Window background.
    bg,
    /// Cards, panels and the sidebar.
    surface,
    /// Raised or hovered surfaces.
    surface2,
    /// Pressed surfaces and track backgrounds.
    surface3,
    text,
    text_dim,
    accent,
    on_accent,
    focus,
    short_break,
    long_break,
    holiday,
    appointment,
    deadline,
    habit,
    success,
    warning,
    danger,
    outline,
}

/// The few colours everything else can be derived from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed {
    pub bg: Rgb,
    pub surface: Option<Rgb>,
    pub text: Rgb,
    pub accent: Rgb,
    pub on_accent: Option<Rgb>,
    pub success: Option<Rgb>,
    pub warning: Option<Rgb>,
    pub danger: Option<Rgb>,
}

impl Seed {
    pub fn builtin(dark: bool) -> Seed {
        if dark {
            Seed {
                bg: Rgb::hex(0x1c1c20),
                surface: Some(Rgb::hex(0x26262b)),
                text: Rgb::hex(0xeeeef0),
                accent: Rgb::hex(0x78aeed),
                on_accent: None,
                success: None,
                warning: None,
                danger: None,
            }
        } else {
            Seed {
                bg: Rgb::hex(0xf6f5f4),
                surface: Some(Rgb::hex(0xffffff)),
                text: Rgb::hex(0x1e1e22),
                accent: Rgb::hex(0x1c71d8),
                on_accent: None,
                success: None,
                warning: None,
                danger: None,
            }
        }
    }

    pub fn derive(&self) -> Colors {
        let dark = self.bg.is_dark();
        let (bg, text) = (self.bg, self.text);
        // A fixed-lightness tone so derived colours sit well on the background.
        let tone = |hue: f32| Rgb::from_hsl(hue, 0.55, if dark { 0.68 } else { 0.40 });
        let surface = match self.surface {
            Some(s) if s.contrast(bg) > 1.03 => s,
            _ => bg.mix(text, if dark { 0.06 } else { 0.04 }),
        };
        let success = self.success.unwrap_or(tone(150.0));
        let warning = self.warning.unwrap_or(tone(38.0));
        let danger = self.danger.unwrap_or(tone(352.0));
        Colors {
            bg,
            surface,
            surface2: surface.mix(text, 0.07),
            surface3: surface.mix(text, 0.14),
            text,
            text_dim: text.mix(bg, 0.40),
            accent: self.accent,
            on_accent: self.on_accent.unwrap_or(self.accent.readable()),
            focus: self.accent,
            short_break: success,
            long_break: tone(190.0),
            holiday: warning,
            appointment: tone(215.0),
            deadline: danger,
            habit: tone(140.0),
            success,
            warning,
            danger,
            outline: bg.mix(text, 0.16),
        }
    }

    fn set(&mut self, key: &str, v: Rgb) {
        match key {
            "bg" => self.bg = v,
            "surface" => self.surface = Some(v),
            "text" => self.text = v,
            "accent" => {
                self.accent = v;
                // The old foreground was chosen for the old accent.
                self.on_accent = None;
            }
            "on_accent" => self.on_accent = Some(v),
            "success" => self.success = Some(v),
            "warning" => self.warning = Some(v),
            "danger" => self.danger = Some(v),
            _ => {}
        }
    }
}

/// Derives a palette from `seed`, letting each layer change seed colours
/// (which re-derives what depends on them) and pin individual colours.
/// Later layers win. Unknown keys and unparsable colours are reported.
pub fn build(mut seed: Seed, layers: &[&BTreeMap<String, String>]) -> (Colors, Vec<String>) {
    let mut pinned: BTreeMap<&str, Rgb> = BTreeMap::new();
    let mut problems = Vec::new();
    for layer in layers {
        for (k, v) in layer.iter() {
            match (Colors::KEYS.contains(&k.as_str()), Rgb::parse(v)) {
                (true, Some(c)) => {
                    pinned.insert(k, c);
                }
                (false, _) => problems.push(format!("unknown colour \"{k}\"")),
                (true, None) => problems.push(format!("\"{v}\" is not a colour (for {k})")),
            }
        }
    }
    for (k, v) in &pinned {
        seed.set(k, *v);
    }
    let mut colors = seed.derive();
    for (k, v) in &pinned {
        colors.set(k, *v);
    }
    (colors, problems)
}

pub fn seed_from_gtk(g: &GtkTheme) -> Option<Seed> {
    let c = |names: &[&str]| names.iter().find_map(|n| g.colors.get(*n).copied());
    let bg = c(&["window_bg_color", "theme_bg_color"])?;
    let bg = Rgb { a: 1.0, ..bg };
    let text = c(&["window_fg_color", "theme_fg_color", "theme_text_color"])?.over(bg);
    let accent = c(&["accent_bg_color", "accent_color", "theme_selected_bg_color"]);
    Some(Seed {
        bg,
        surface: c(&[
            "card_bg_color",
            "sidebar_bg_color",
            "headerbar_bg_color",
            "theme_base_color",
        ])
        .map(|s| s.over(bg)),
        text,
        accent: accent.unwrap_or(Seed::builtin(bg.is_dark()).accent),
        on_accent: c(&["accent_fg_color", "theme_selected_fg_color"]).filter(|_| accent.is_some()),
        success: c(&["success_bg_color", "success_color"]),
        warning: c(&["warning_bg_color", "warning_color"]),
        danger: c(&[
            "destructive_bg_color",
            "destructive_color",
            "error_bg_color",
            "error_color",
        ]),
    })
}

pub fn seed_from_qt(q: &QtTheme) -> Option<Seed> {
    let (bg, text) = (q.window?, q.window_text?);
    let bg = Rgb { a: 1.0, ..bg };
    Some(Seed {
        bg,
        surface: q.button.or(q.base).map(|s| s.over(bg)),
        text: text.over(bg),
        accent: q.highlight.unwrap_or(Seed::builtin(bg.is_dark()).accent),
        on_accent: q.highlighted_text.filter(|_| q.highlight.is_some()),
        success: q.positive,
        warning: q.neutral,
        danger: q.negative,
    })
}

/// Everything read from the desktop, before the user's choices are applied.
#[derive(Debug, Clone, Default)]
pub struct SystemInput {
    pub gtk: GtkTheme,
    pub qt: QtTheme,
    pub scheme: Option<Scheme>,
}

impl SystemInput {
    pub fn read(config_home: &Path) -> SystemInput {
        SystemInput {
            gtk: super::gtk::load(config_home),
            qt: super::qt::load(config_home),
            scheme: super::portal::color_scheme(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct System {
    pub seed: Seed,
    pub font: Option<String>,
    /// Where the colours came from, for display in settings.
    pub origin: &'static str,
}

pub fn system(input: &SystemInput, source: Source, mode: Mode) -> System {
    let gtk = seed_from_gtk(&input.gtk).map(|s| (s, "GTK"));
    let qt = seed_from_qt(&input.qt).map(|s| (s, "Qt"));
    let found = match source {
        Source::Gtk => gtk,
        Source::Qt => qt,
        Source::Auto => gtk.or(qt),
    };
    let font = match source {
        Source::Qt => input.qt.font.clone().or(input.gtk.font.clone()),
        _ => input.gtk.font.clone().or(input.qt.font.clone()),
    };
    let want_dark = match mode {
        Mode::Dark => Some(true),
        Mode::Light => Some(false),
        Mode::System => match input.scheme {
            Some(Scheme::Dark) => Some(true),
            Some(Scheme::Light) => Some(false),
            _ => input.gtk.prefer_dark.filter(|d| *d),
        },
    };
    match (found, want_dark) {
        // The toolkit colours match what was asked for, or nothing was asked.
        (Some((seed, origin)), want) if want.is_none_or(|d| d == seed.bg.is_dark()) => {
            System { seed, font, origin }
        }
        // They are the wrong brightness: keep the accent, replace the rest.
        (Some((seed, _)), Some(dark)) => System {
            seed: Seed {
                accent: seed.accent,
                ..Seed::builtin(dark)
            },
            font,
            origin: "built-in, with the system accent",
        },
        (_, want) => System {
            seed: Seed::builtin(want.unwrap_or(true)),
            font,
            origin: "built-in",
        },
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ThemeFile {
    pub name: Option<String>,
    /// "system" to start from the desktop's colours.
    pub inherit: Option<String>,
    /// For themes that do not inherit: which built-in base fills the gaps.
    pub mode: Option<Mode>,
    pub radius: Option<f32>,
    pub font: Option<String>,
    pub colors: BTreeMap<String, String>,
}

pub const PRESETS: &[(&str, &str)] = &[
    (
        "rose-pine",
        include_str!("../../assets/themes/rose-pine.toml"),
    ),
    ("gruvbox", include_str!("../../assets/themes/gruvbox.toml")),
    ("nord", include_str!("../../assets/themes/nord.toml")),
    ("paper", include_str!("../../assets/themes/paper.toml")),
];

/// "system", then the files in the user's themes folder, then presets not
/// shadowed by a file of the same name.
pub fn available(themes_dir: &Path) -> Vec<String> {
    let mut names = vec!["system".to_string()];
    let mut user: Vec<String> = std::fs::read_dir(themes_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    user.sort();
    names.extend(user);
    for (n, _) in PRESETS {
        if !names.iter().any(|x| x == n) {
            names.push(n.to_string());
        }
    }
    names
}

pub fn load_file(name: &str, themes_dir: &Path) -> Result<ThemeFile, String> {
    let path = themes_dir.join(format!("{name}.toml"));
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => PRESETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| t.to_string())
            .ok_or_else(|| format!("theme \"{name}\" was not found"))?,
    };
    toml::from_str(&text).map_err(|e| format!("theme \"{name}\": {e}"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub c: Colors,
    pub dark: bool,
    pub radius: f32,
    pub font: Option<String>,
    pub compact: bool,
    pub origin: String,
    /// Things that were wrong in the theme or overrides, shown in settings.
    pub problems: Vec<String>,
}

pub const DEFAULT_RADIUS: f32 = 10.0;

pub fn resolve(
    input: &SystemInput,
    app: &Appearance,
    overrides: &BTreeMap<String, String>,
    themes_dir: &Path,
) -> Theme {
    let sys = system(input, app.source, app.mode);
    let mut problems = Vec::new();
    let mut file = ThemeFile::default();
    let mut name = "system".to_string();
    if app.theme != "system" && !app.theme.is_empty() {
        match load_file(&app.theme, themes_dir) {
            Ok(f) => {
                name = app.theme.clone();
                file = f;
            }
            Err(e) => problems.push(format!("{e}; using the system theme")),
        }
    }
    let inherits = name == "system" || file.inherit.as_deref() == Some("system");
    let (seed, origin) = if inherits {
        (sys.seed, sys.origin.to_string())
    } else {
        let dark = match (app.mode, file.mode) {
            (Mode::Dark, _) | (Mode::System, Some(Mode::Dark) | Some(Mode::System) | None) => true,
            (Mode::Light, _) | (Mode::System, Some(Mode::Light)) => false,
        };
        // A forced mode only changes the fallback base; a file that sets its
        // own background keeps it.
        (Seed::builtin(dark), "theme file".to_string())
    };
    let (c, mut p) = build(seed, &[&file.colors, overrides]);
    problems.append(&mut p);
    let font = Some(app.font.trim().to_string())
        .filter(|f| !f.is_empty())
        .or(file.font.clone())
        .or(sys.font);
    Theme {
        name,
        dark: c.bg.is_dark(),
        c,
        radius: app
            .radius
            .or(file.radius)
            .unwrap_or(DEFAULT_RADIUS)
            .clamp(0.0, 24.0),
        font,
        compact: app.compact,
        origin,
        problems,
    }
}

/// A value that changes when any file the theme depends on changes.
pub fn fingerprint(input: &SystemInput, extra: &[PathBuf]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut stat = |p: &Path| {
        p.hash(&mut h);
        if let Ok(m) = std::fs::metadata(p) {
            m.len().hash(&mut h);
            m.modified().ok().hash(&mut h);
        }
    };
    for p in input.gtk.files.iter().chain(&input.qt.files).chain(extra) {
        stat(p);
        if p.is_dir() {
            for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
                stat(&e.path());
            }
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gtk(css: &str) -> GtkTheme {
        GtkTheme {
            colors: super::super::gtk::parse_str(css),
            ..Default::default()
        }
    }

    const ROSE: &str =
        "@define-color window_bg_color #191724; @define-color window_fg_color #e0def4;
        @define-color card_bg_color #1f1d2e; @define-color accent_bg_color #c4a7e7;
        @define-color accent_fg_color #191724; @define-color success_color #31748f;
        @define-color warning_color #f6c177; @define-color destructive_color #eb6f92;";

    fn input(css: &str) -> SystemInput {
        SystemInput {
            gtk: gtk(css),
            ..Default::default()
        }
    }

    fn none() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn follows_gtk_colours() {
        let t = resolve(
            &input(ROSE),
            &Appearance::default(),
            &none(),
            Path::new("/nonexistent"),
        );
        assert_eq!(t.name, "system");
        assert_eq!(t.origin, "GTK");
        assert!(t.dark);
        assert_eq!(t.c.bg.to_hex(), "#191724");
        assert_eq!(t.c.surface.to_hex(), "#1f1d2e");
        assert_eq!(t.c.accent.to_hex(), "#c4a7e7");
        assert_eq!(t.c.focus, t.c.accent);
        assert_eq!(t.c.on_accent.to_hex(), "#191724");
        assert_eq!(t.c.deadline.to_hex(), "#eb6f92");
        assert_eq!(t.c.holiday.to_hex(), "#f6c177");
        assert!(t.problems.is_empty());
    }

    #[test]
    fn dark_by_default_with_nothing_to_read() {
        let t = resolve(
            &SystemInput::default(),
            &Appearance::default(),
            &none(),
            Path::new("/nonexistent"),
        );
        assert!(t.dark);
        assert_eq!(t.origin, "built-in");
    }

    #[test]
    fn portal_and_mode_decide_brightness() {
        let mut i = input(ROSE);
        i.scheme = Some(Scheme::Light);
        let t = resolve(
            &i,
            &Appearance::default(),
            &none(),
            Path::new("/nonexistent"),
        );
        assert!(!t.dark, "portal asks for light, GTK colours are dark");
        assert_eq!(t.c.accent.to_hex(), "#c4a7e7", "accent is kept");

        let forced = Appearance {
            mode: Mode::Dark,
            ..Default::default()
        };
        assert_eq!(
            resolve(&i, &forced, &none(), Path::new("/nonexistent"))
                .c
                .bg
                .to_hex(),
            "#191724"
        );

        i.scheme = Some(Scheme::NoPreference);
        assert!(
            resolve(
                &i,
                &Appearance::default(),
                &none(),
                Path::new("/nonexistent")
            )
            .dark
        );
    }

    #[test]
    fn qt_source() {
        let mut i = input(ROSE);
        i.qt = super::super::qt::parse_str(
            "[Colors:Window]\nBackgroundNormal=239,240,241\nForegroundNormal=35,38,41\n\
             [Colors:Selection]\nBackgroundNormal=61,174,233\n",
        );
        let app = Appearance {
            source: Source::Qt,
            ..Default::default()
        };
        let t = resolve(&i, &app, &none(), Path::new("/nonexistent"));
        assert_eq!(
            (t.origin.as_str(), t.c.bg.to_hex().as_str()),
            ("Qt", "#eff0f1")
        );
        assert_eq!(t.c.accent.to_hex(), "#3daee9");
        let auto = resolve(
            &i,
            &Appearance::default(),
            &none(),
            Path::new("/nonexistent"),
        );
        assert_eq!(auto.origin, "GTK", "automatic prefers GTK");
        i.gtk = GtkTheme::default();
        assert_eq!(
            resolve(
                &i,
                &Appearance::default(),
                &none(),
                Path::new("/nonexistent")
            )
            .origin,
            "Qt"
        );
    }

    #[test]
    fn overrides_rederive_and_pin() {
        let t = resolve(
            &input(ROSE),
            &Appearance::default(),
            &map(&[
                ("accent", "#ff8800"),
                ("habit", "#00ff00"),
                ("nope", "#000"),
                ("text", "red"),
            ]),
            Path::new("/nonexistent"),
        );
        assert_eq!(t.c.accent.to_hex(), "#ff8800");
        assert_eq!(t.c.focus.to_hex(), "#ff8800", "focus follows the accent");
        assert_eq!(
            t.c.on_accent,
            Rgb::hex(0xff8800).readable(),
            "old on_accent no longer applies"
        );
        assert_eq!(t.c.habit.to_hex(), "#00ff00");
        assert_eq!(t.c.text.to_hex(), "#e0def4", "a bad value is ignored");
        assert_eq!(t.problems.len(), 2);
    }

    #[test]
    fn presets_parse_and_are_complete_enough() {
        for (name, _) in PRESETS {
            let app = Appearance {
                theme: name.to_string(),
                ..Default::default()
            };
            let t = resolve(
                &SystemInput::default(),
                &app,
                &none(),
                Path::new("/nonexistent"),
            );
            assert!(t.problems.is_empty(), "{name}: {:?}", t.problems);
            assert_eq!(&t.name, name);
            assert!(t.c.text.contrast(t.c.bg) > 7.0, "{name} text contrast");
            assert!(
                t.c.on_accent.contrast(t.c.accent) > 4.0,
                "{name} accent contrast"
            );
        }
    }

    #[test]
    fn theme_files_standalone_and_inheriting() {
        let dir = std::env::temp_dir().join(format!("pomolog-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mine.toml"),
            "radius = 2\n[colors]\nbg = \"#000000\"\ntext = \"#ffffff\"\naccent = \"#00ffaa\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("tweak.toml"),
            "inherit = \"system\"\n[colors]\nfocus = \"#123456\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("broken.toml"), "colors = 3").unwrap();
        assert_eq!(available(&dir)[..4], ["system", "broken", "mine", "tweak"]);

        let app = |n: &str| Appearance {
            theme: n.into(),
            ..Default::default()
        };
        let mine = resolve(&input(ROSE), &app("mine"), &none(), &dir);
        assert_eq!(
            mine.c.bg.to_hex(),
            "#000000",
            "standalone ignores the system"
        );
        assert_eq!(mine.c.focus.to_hex(), "#00ffaa");
        assert_eq!(mine.radius, 2.0);

        let tweak = resolve(&input(ROSE), &app("tweak"), &none(), &dir);
        assert_eq!(tweak.c.bg.to_hex(), "#191724");
        assert_eq!(tweak.c.focus.to_hex(), "#123456");
        assert_eq!(tweak.c.accent.to_hex(), "#c4a7e7");

        for bad in ["broken", "missing"] {
            let t = resolve(&input(ROSE), &app(bad), &none(), &dir);
            assert_eq!(t.name, "system");
            assert_eq!(t.problems.len(), 1);
        }

        let before = fingerprint(&SystemInput::default(), std::slice::from_ref(&dir));
        std::fs::write(
            dir.join("tweak.toml"),
            "inherit = \"system\"\n[colors]\nfocus = \"#654321\"\n# changed\n",
        )
        .unwrap();
        assert_ne!(
            before,
            fingerprint(&SystemInput::default(), std::slice::from_ref(&dir))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
