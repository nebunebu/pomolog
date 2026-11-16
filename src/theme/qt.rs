//! Reads Qt colours from qt6ct/qt5ct colour schemes and KDE's kdeglobals.

use super::color::Rgb;
use super::gtk::font_family;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct QtTheme {
    pub window: Option<Rgb>,
    pub window_text: Option<Rgb>,
    pub base: Option<Rgb>,
    pub button: Option<Rgb>,
    pub highlight: Option<Rgb>,
    pub highlighted_text: Option<Rgb>,
    pub positive: Option<Rgb>,
    pub neutral: Option<Rgb>,
    pub negative: Option<Rgb>,
    pub font: Option<String>,
    pub files: Vec<PathBuf>,
}

impl QtTheme {
    pub fn is_empty(&self) -> bool {
        self.window.is_none() || self.window_text.is_none()
    }
}

type Ini = HashMap<String, HashMap<String, String>>;

fn parse_ini(text: &str) -> Ini {
    let mut out = Ini::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
        } else if let Some((k, v)) = line.split_once('=') {
            out.entry(section.clone())
                .or_default()
                .insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

fn kde_color(ini: &Ini, section: &str, key: &str) -> Option<Rgb> {
    let v = ini.get(section)?.get(key)?;
    let mut parts = v.split(',').map(|p| p.trim().parse::<f32>());
    let (r, g, b) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    Some(Rgb::new(r / 255.0, g / 255.0, b / 255.0))
}

/// Fills in whatever the text provides without overwriting earlier values.
fn apply(theme: &mut QtTheme, ini: &Ini) {
    // qt6ct lists the palette roles in QPalette::ColorRole order.
    if let Some(list) = ini.get("ColorScheme").and_then(|s| s.get("active_colors")) {
        let c: Vec<Option<Rgb>> = list.split(',').map(Rgb::parse_qt).collect();
        let role = |i: usize| c.get(i).copied().flatten();
        theme.window_text = theme.window_text.or(role(0));
        theme.button = theme.button.or(role(1));
        theme.base = theme.base.or(role(9));
        theme.window = theme.window.or(role(10));
        theme.highlight = theme.highlight.or(role(12));
        theme.highlighted_text = theme.highlighted_text.or(role(13));
    }
    let k = |s, key| kde_color(ini, s, key);
    theme.window = theme.window.or(k("Colors:Window", "BackgroundNormal"));
    theme.window_text = theme.window_text.or(k("Colors:Window", "ForegroundNormal"));
    theme.base = theme.base.or(k("Colors:View", "BackgroundNormal"));
    theme.button = theme.button.or(k("Colors:Button", "BackgroundNormal"));
    theme.highlight = theme
        .highlight
        .or(k("Colors:Selection", "BackgroundNormal"));
    theme.highlighted_text = theme
        .highlighted_text
        .or(k("Colors:Selection", "ForegroundNormal"));
    theme.positive = theme.positive.or(k("Colors:View", "ForegroundPositive"));
    theme.neutral = theme.neutral.or(k("Colors:View", "ForegroundNeutral"));
    theme.negative = theme.negative.or(k("Colors:View", "ForegroundNegative"));
}

pub fn parse_str(text: &str) -> QtTheme {
    let mut t = QtTheme::default();
    apply(&mut t, &parse_ini(text));
    t
}

fn newest_conf(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "conf" || x == "colors"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// Looks at qt6ct, then qt5ct, then kdeglobals under the given config home.
pub fn load(config_home: &Path) -> QtTheme {
    let mut theme = QtTheme::default();
    for ct in ["qt6ct", "qt5ct"] {
        let dir = config_home.join(ct);
        let conf = dir.join(format!("{ct}.conf"));
        let mut scheme = None;
        if let Ok(text) = std::fs::read_to_string(&conf) {
            theme.files.push(conf);
            let ini = parse_ini(&text);
            scheme = ini
                .get("Appearance")
                .and_then(|a| a.get("color_scheme_path"))
                .map(|p| PathBuf::from(p.trim_matches('"')))
                .filter(|p| p.is_file());
            if theme.font.is_none() {
                theme.font = ini
                    .get("Fonts")
                    .and_then(|f| f.get("general"))
                    .and_then(|f| font_family(f));
            }
        }
        // Without an explicit scheme, take the most recently written one.
        if let Some(path) = scheme.or_else(|| newest_conf(&dir.join("colors")))
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            theme.files.push(path);
            apply(&mut theme, &parse_ini(&text));
        }
    }
    let kde = config_home.join("kdeglobals");
    if let Ok(text) = std::fs::read_to_string(&kde) {
        theme.files.push(kde);
        let ini = parse_ini(&text);
        apply(&mut theme, &ini);
        if theme.font.is_none() {
            theme.font = ini
                .get("General")
                .and_then(|g| g.get("font"))
                .and_then(|f| font_family(f));
        }
    }
    theme
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEME: &str = "[ColorScheme]
active_colors=#e0def4, #1f1d2e, #1f1d2e, #908caa, #26233a, #26233a, #e0def4, #e0def4, #e0def4, #191724, #1a1825, #000000, #c4a7e7, #191724, #f6c177
disabled_colors=#ff000000

[Colors:View]
BackgroundNormal=1,2,3
ForegroundNegative=235,111,146
ForegroundNeutral=246,193,119
ForegroundPositive=49,116,143
";

    #[test]
    fn reads_qt6ct_and_kde_sections() {
        let t = parse_str(SCHEME);
        assert_eq!(t.window.unwrap().to_hex(), "#1a1825");
        assert_eq!(t.window_text.unwrap().to_hex(), "#e0def4");
        assert_eq!(
            t.base.unwrap().to_hex(),
            "#191724",
            "qt6ct wins over the KDE section"
        );
        assert_eq!(t.highlight.unwrap().to_hex(), "#c4a7e7");
        assert_eq!(t.negative.unwrap().to_hex(), "#eb6f92");
        assert!(!t.is_empty());
    }

    #[test]
    fn kdeglobals_only() {
        let t = parse_str(
            "[Colors:Window]\nBackgroundNormal=239,240,241\nForegroundNormal=35,38,41\n\
             [Colors:Selection]\nBackgroundNormal=61,174,233\n",
        );
        assert_eq!(t.window.unwrap().to_hex(), "#eff0f1");
        assert_eq!(t.highlight.unwrap().to_hex(), "#3daee9");
        assert_eq!(t.positive, None);
    }

    #[test]
    fn loads_newest_scheme_and_font() {
        let dir = std::env::temp_dir().join(format!("pomolog-qt-{}", std::process::id()));
        let colors = dir.join("qt6ct/colors");
        std::fs::create_dir_all(&colors).unwrap();
        std::fs::write(
            dir.join("qt6ct/qt6ct.conf"),
            "[Fonts]\ngeneral=\"IBM Plex Sans,12\"\n",
        )
        .unwrap();
        std::fs::write(colors.join("matugen.conf"), SCHEME).unwrap();
        let t = load(&dir);
        assert_eq!(t.font.as_deref(), Some("IBM Plex Sans"));
        assert_eq!(t.highlight.unwrap().to_hex(), "#c4a7e7");
        assert_eq!(t.files.len(), 2);
        assert!(load(&dir.join("nothing-here")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
