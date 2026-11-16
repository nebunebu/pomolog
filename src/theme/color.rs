/// A colour with components in 0.0..=1.0. Kept separate from the GUI
/// toolkit's type so theme loading can be tested on its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0.0, 0.0, 0.0);
    pub const WHITE: Rgb = Rgb::new(1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32) -> Rgb {
        Rgb { r, g, b, a: 1.0 }
    }

    pub const fn hex(v: u32) -> Rgb {
        Rgb::new(
            ((v >> 16) & 0xff) as f32 / 255.0,
            ((v >> 8) & 0xff) as f32 / 255.0,
            (v & 0xff) as f32 / 255.0,
        )
    }

    /// Parses `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`.
    pub fn parse(s: &str) -> Option<Rgb> {
        let h = s.trim().strip_prefix('#')?;
        if !h.is_ascii() {
            return None;
        }
        let n = |i: usize, len: usize| u8::from_str_radix(&h[i..i + len], 16).ok();
        let (r, g, b, a) = match h.len() {
            3 | 4 => {
                let d = |i| n(i, 1).map(|v| v * 17);
                (d(0)?, d(1)?, d(2)?, if h.len() == 4 { d(3)? } else { 255 })
            }
            6 | 8 => (
                n(0, 2)?,
                n(2, 2)?,
                n(4, 2)?,
                if h.len() == 8 { n(6, 2)? } else { 255 },
            ),
            _ => return None,
        };
        Some(Rgb {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        })
    }

    /// Qt writes alpha first: `#aarrggbb`.
    pub fn parse_qt(s: &str) -> Option<Rgb> {
        let h = s.trim().strip_prefix('#')?;
        if h.len() == 8 && h.is_ascii() {
            Rgb::parse(&format!("#{}{}", &h[2..], &h[..2]))
        } else {
            Rgb::parse(s)
        }
    }

    pub fn to_hex(self) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        if self.a >= 0.999 {
            format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
        } else {
            format!(
                "#{:02x}{:02x}{:02x}{:02x}",
                c(self.r),
                c(self.g),
                c(self.b),
                c(self.a)
            )
        }
    }

    pub fn alpha(self, a: f32) -> Rgb {
        Rgb {
            a: (self.a * a).clamp(0.0, 1.0),
            ..self
        }
    }

    /// Linear blend: 0.0 gives `self`, 1.0 gives `other`.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Rgb {
            r: l(self.r, other.r),
            g: l(self.g, other.g),
            b: l(self.b, other.b),
            a: l(self.a, other.a),
        }
    }

    /// Flattens a translucent colour onto an opaque background.
    pub fn over(self, bg: Rgb) -> Rgb {
        Rgb {
            a: 1.0,
            ..bg.mix(Rgb { a: 1.0, ..self }, self.a)
        }
    }

    /// Perceived brightness, 0.0 (black) to 1.0 (white).
    pub fn luminance(self) -> f32 {
        let lin = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    pub fn is_dark(self) -> bool {
        self.luminance() < 0.2
    }

    pub fn contrast(self, other: Rgb) -> f32 {
        let (a, b) = (self.luminance() + 0.05, other.luminance() + 0.05);
        if a > b { a / b } else { b / a }
    }

    /// Black or white, whichever reads better on this colour.
    pub fn readable(self) -> Rgb {
        if self.contrast(Rgb::WHITE) >= self.contrast(Rgb::hex(0x16161a)) {
            Rgb::WHITE
        } else {
            Rgb::hex(0x16161a)
        }
    }

    pub fn to_hsl(self) -> (f32, f32, f32) {
        let (max, min) = (
            self.r.max(self.g).max(self.b),
            self.r.min(self.g).min(self.b),
        );
        let l = (max + min) / 2.0;
        let d = max - min;
        if d < 1e-6 {
            return (0.0, 0.0, l);
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == self.r {
            ((self.g - self.b) / d).rem_euclid(6.0)
        } else if max == self.g {
            (self.b - self.r) / d + 2.0
        } else {
            (self.r - self.g) / d + 4.0
        };
        (h * 60.0, s, l)
    }

    pub fn from_hsl(h: f32, s: f32, l: f32) -> Rgb {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hp = h.rem_euclid(360.0) / 60.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let (r, g, b) = match hp as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        Rgb::new(r + m, g + m, b + m)
    }

    /// Same lightness and saturation, hue turned by `deg`.
    pub fn rotate(self, deg: f32) -> Rgb {
        let (h, s, l) = self.to_hsl();
        Rgb {
            a: self.a,
            ..Rgb::from_hsl(h + deg, s, l)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!(Rgb::parse("#fff"), Some(Rgb::WHITE));
        assert_eq!(Rgb::parse("#c4a7e7").unwrap().to_hex(), "#c4a7e7");
        assert_eq!(Rgb::parse("#00000080").unwrap().to_hex(), "#00000080");
        assert_eq!(Rgb::parse("c4a7e7"), None);
        assert_eq!(Rgb::parse("#c4a7e"), None);
        assert_eq!(Rgb::parse("#zzzzzz"), None);
        assert_eq!(Rgb::parse("#ééé"), None);
    }

    #[test]
    fn qt_alpha_first() {
        assert_eq!(Rgb::parse_qt("#ff191724").unwrap().to_hex(), "#191724");
        assert_eq!(Rgb::parse_qt("#191724").unwrap().to_hex(), "#191724");
    }

    #[test]
    fn hsl_round_trip() {
        for hex in [0xc4a7e7, 0x31748f, 0xeb6f92, 0x808080, 0x000000] {
            let c = Rgb::hex(hex);
            let (h, s, l) = c.to_hsl();
            assert_eq!(Rgb::from_hsl(h, s, l).to_hex(), c.to_hex());
        }
        assert_eq!(Rgb::hex(0xff0000).rotate(120.0).to_hex(), "#00ff00");
    }

    #[test]
    fn brightness() {
        assert!(Rgb::hex(0x191724).is_dark());
        assert!(!Rgb::hex(0xfaf4ed).is_dark());
        assert_eq!(Rgb::hex(0xc4a7e7).readable().to_hex(), "#16161a");
        assert_eq!(Rgb::hex(0x1a5fb4).readable(), Rgb::WHITE);
    }
}
