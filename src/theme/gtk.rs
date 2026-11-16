//! Reads named colours from GTK's user stylesheets, the same place
//! libadwaita apps get recoloured from.

use super::color::Rgb;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone)]
pub struct GtkTheme {
    pub colors: HashMap<String, Rgb>,
    /// Font family from settings.ini, without the size.
    pub font: Option<String>,
    /// `gtk-application-prefer-dark-theme`, when set.
    pub prefer_dark: Option<bool>,
    /// Every file that was read, for change detection.
    pub files: Vec<PathBuf>,
}

/// Loads `gtk-4.0`, falling back to `gtk-3.0`, under the given config home.
pub fn load(config_home: &Path) -> GtkTheme {
    let mut out = GtkTheme::default();
    for dir in ["gtk-4.0", "gtk-3.0"] {
        let dir = config_home.join(dir);
        if out.colors.is_empty() {
            let mut raw = HashMap::new();
            read_css(&dir.join("gtk.css"), &mut raw, &mut out.files, 0);
            out.colors = resolve_all(&raw);
        }
        let ini = dir.join("settings.ini");
        if let Ok(text) = std::fs::read_to_string(&ini) {
            out.files.push(ini);
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else {
                    continue;
                };
                match k.trim() {
                    "gtk-font-name" if out.font.is_none() => out.font = font_family(v),
                    "gtk-application-prefer-dark-theme" if out.prefer_dark.is_none() => {
                        out.prefer_dark = Some(matches!(v.trim(), "1" | "true" | "TRUE"));
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

/// "IBM Plex Sans 12" -> "IBM Plex Sans". Also handles "Family,12".
pub fn font_family(spec: &str) -> Option<String> {
    let s = spec.trim().trim_matches('"');
    let s = s.split(',').next().unwrap_or(s).trim();
    let s = match s.rsplit_once(' ') {
        Some((family, size)) if size.parse::<f32>().is_ok() => family.trim(),
        _ => s,
    };
    (!s.is_empty()).then(|| s.to_string())
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn read_css(path: &Path, raw: &mut HashMap<String, String>, files: &mut Vec<PathBuf>, depth: u32) {
    if depth > 8 || files.iter().any(|f| f == path) {
        return;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    files.push(path.to_path_buf());
    for stmt in parse_css(&text) {
        match stmt {
            Stmt::Define(name, value) => {
                raw.insert(name, value);
            }
            Stmt::Import(target) => {
                let target = path.parent().unwrap_or(Path::new(".")).join(target);
                read_css(&target, raw, files, depth + 1);
            }
        }
    }
}

enum Stmt {
    Define(String, String),
    Import(String),
}

/// The `@define-color` and `@import` statements of a stylesheet, in order.
fn parse_css(text: &str) -> Vec<Stmt> {
    let text = strip_comments(text);
    let mut out = Vec::new();
    for stmt in text.split(';') {
        // A statement may follow a rule block; only the tail matters.
        let stmt = stmt.rsplit('}').next().unwrap_or(stmt).trim();
        if let Some(rest) = stmt.strip_prefix("@define-color") {
            if let Some((name, value)) = rest.trim().split_once(char::is_whitespace) {
                out.push(Stmt::Define(
                    name.trim().to_string(),
                    value.trim().to_string(),
                ));
            }
        } else if let Some(rest) = stmt.strip_prefix("@import") {
            let rest = rest.trim();
            let rest = rest
                .strip_prefix("url(")
                .and_then(|r| r.strip_suffix(')'))
                .unwrap_or(rest);
            let target = rest.trim().trim_matches(['"', '\'']);
            if !target.is_empty() && !target.contains("://") {
                out.push(Stmt::Import(target.to_string()));
            }
        }
    }
    out
}

pub fn parse_str(css: &str) -> HashMap<String, Rgb> {
    // Later definitions replace earlier ones.
    let raw = parse_css(css)
        .into_iter()
        .filter_map(|s| {
            if let Stmt::Define(k, v) = s {
                Some((k, v))
            } else {
                None
            }
        })
        .collect();
    resolve_all(&raw)
}

fn resolve_all(raw: &HashMap<String, String>) -> HashMap<String, Rgb> {
    raw.keys()
        .filter_map(|k| Some((k.clone(), resolve(raw, &raw[k], 0)?)))
        .collect()
}

fn resolve(raw: &HashMap<String, String>, expr: &str, depth: u32) -> Option<Rgb> {
    if depth > 16 {
        return None;
    }
    let e = expr.trim();
    if let Some(name) = e.strip_prefix('@') {
        return resolve(raw, raw.get(name.trim())?, depth + 1);
    }
    if e.starts_with('#') {
        return Rgb::parse(e);
    }
    if let Some(open) = e.find('(')
        && e.ends_with(')')
    {
        let func = e[..open].trim();
        let args = split_args(&e[open + 1..e.len() - 1]);
        let col = |i: usize| resolve(raw, args.get(i)?, depth + 1);
        let num = |i: usize| parse_number(args.get(i)?);
        return match func {
            "rgb" | "rgba" => {
                let (r, g, b) = (num(0)?, num(1)?, num(2)?);
                // Fractions and 0-255 both occur in the wild.
                let scale = if r <= 1.0
                    && g <= 1.0
                    && b <= 1.0
                    && args[..3].iter().any(|a| a.contains('.'))
                {
                    1.0
                } else {
                    255.0
                };
                Some(Rgb {
                    r: (r / scale).clamp(0.0, 1.0),
                    g: (g / scale).clamp(0.0, 1.0),
                    b: (b / scale).clamp(0.0, 1.0),
                    a: if args.len() > 3 {
                        num(3)?.clamp(0.0, 1.0)
                    } else {
                        1.0
                    },
                })
            }
            "alpha" => Some(col(0)?.alpha(num(1)?)),
            "mix" => Some(col(0)?.mix(col(1)?, num(2)?)),
            "shade" => {
                let (c, f) = (col(0)?, num(1)?);
                Some(if f <= 1.0 {
                    Rgb {
                        a: c.a,
                        ..Rgb::BLACK
                    }
                    .mix(c, f)
                } else {
                    c.mix(
                        Rgb {
                            a: c.a,
                            ..Rgb::WHITE
                        },
                        (f - 1.0).min(1.0),
                    )
                })
            }
            "lighter" => Some(col(0)?.mix(Rgb::WHITE, 0.3)),
            "darker" => Some(col(0)?.mix(Rgb::BLACK, 0.3)),
            _ => None,
        };
    }
    match e.to_ascii_lowercase().as_str() {
        "white" => Some(Rgb::WHITE),
        "black" => Some(Rgb::BLACK),
        "transparent" => Some(Rgb::BLACK.alpha(0.0)),
        _ => None,
    }
}

fn parse_number(s: &str) -> Option<f32> {
    let s = s.trim();
    match s.strip_suffix('%') {
        Some(p) => p.trim().parse::<f32>().ok().map(|v| v / 100.0),
        None => s.parse().ok(),
    }
}

/// Splits on commas that are not inside parentheses.
fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut depth, mut cur) = (Vec::new(), 0, String::new());
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur).trim().to_string());
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_color_forms() {
        let c = parse_str(
            "/* a comment; with a semicolon */
             @define-color accent_bg_color #c4a7e7;
             @define-color window_bg_color #191724;
             @define-color headerbar_backdrop_color @window_bg_color;
             @define-color border rgba(0.121094, 0.113281, 0.179688, 0.7);
             @define-color shade rgba(0, 0, 0, 0.07);
             @define-color plain rgb(255, 128, 0);
             @define-color soft alpha(@accent_bg_color, 0.5);
             @define-color mid mix(@window_bg_color, white, 0.5);
             @define-color loop_a @loop_b;
             @define-color loop_b @loop_a;
             @define-color missing @nope;
             window { color: red; } @define-color after_rule #010203;
             @define-color accent_bg_color #ff0000;",
        );
        assert_eq!(
            c["accent_bg_color"].to_hex(),
            "#ff0000",
            "later definition wins"
        );
        assert_eq!(c["headerbar_backdrop_color"].to_hex(), "#191724");
        assert_eq!(c["border"].to_hex(), "#1f1d2eb3");
        assert_eq!(c["shade"].to_hex(), "#00000012");
        assert_eq!(c["plain"].to_hex(), "#ff8000");
        assert_eq!(c["soft"].to_hex(), "#ff000080");
        assert_eq!(c["mid"].to_hex(), "#8c8b92");
        assert_eq!(c["after_rule"].to_hex(), "#010203");
        assert!(!c.contains_key("loop_a") && !c.contains_key("missing"));
    }

    #[test]
    fn font_names() {
        assert_eq!(
            font_family("IBM Plex Sans 12").as_deref(),
            Some("IBM Plex Sans")
        );
        assert_eq!(
            font_family("\"IBM Plex Sans,12\"").as_deref(),
            Some("IBM Plex Sans")
        );
        assert_eq!(font_family("Inter").as_deref(), Some("Inter"));
        assert_eq!(font_family("  "), None);
    }

    #[test]
    fn follows_imports_and_falls_back_to_gtk3() {
        let dir = std::env::temp_dir().join(format!("pomolog-gtk-{}", std::process::id()));
        let g4 = dir.join("gtk-4.0");
        let g3 = dir.join("gtk-3.0");
        std::fs::create_dir_all(&g4).unwrap();
        std::fs::create_dir_all(&g3).unwrap();
        std::fs::write(
            g4.join("gtk.css"),
            "@import url(\"colors.css\");\n@import 'gone.css';",
        )
        .unwrap();
        std::fs::write(
            g4.join("colors.css"),
            "@define-color window_bg_color #101010;",
        )
        .unwrap();
        std::fs::write(g3.join("gtk.css"), "@define-color window_bg_color #303030;").unwrap();
        std::fs::write(
            g3.join("settings.ini"),
            "[Settings]\ngtk-font-name=Inter 11\ngtk-application-prefer-dark-theme=1\n",
        )
        .unwrap();

        let t = load(&dir);
        assert_eq!(t.colors["window_bg_color"].to_hex(), "#101010");
        assert_eq!(t.font.as_deref(), Some("Inter"));
        assert_eq!(t.prefer_dark, Some(true));
        assert_eq!(t.files.len(), 3);

        std::fs::remove_file(g4.join("gtk.css")).unwrap();
        assert_eq!(load(&dir).colors["window_bg_color"].to_hex(), "#303030");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
