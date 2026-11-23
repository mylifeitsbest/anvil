//! Цвет в формате `#rrggbb` / `#rrggbbaa` с (де)сериализацией из строки.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Цвет sRGB с альфой (не premultiplied).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);
    pub const WHITE: Self = Self([255, 255, 255, 255]);
    pub const BLACK: Self = Self([0, 0, 0, 255]);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 255])
    }

    pub fn r(self) -> u8 {
        self.0[0]
    }
    pub fn g(self) -> u8 {
        self.0[1]
    }
    pub fn b(self) -> u8 {
        self.0[2]
    }
    pub fn a(self) -> u8 {
        self.0[3]
    }

    /// Разбор `#rgb`, `#rrggbb`, `#rrggbbaa` (решётка необязательна).
    pub fn parse(s: &str) -> Option<Self> {
        let h = s.trim().trim_start_matches('#');
        let hex = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        match h.len() {
            3 => {
                let mut out = [0u8; 4];
                for (i, ch) in h.chars().enumerate() {
                    let v = ch.to_digit(16)? as u8;
                    out[i] = v * 17;
                }
                out[3] = 255;
                Some(Self(out))
            }
            6 => Some(Self([hex(0)?, hex(2)?, hex(4)?, 255])),
            8 => Some(Self([hex(0)?, hex(2)?, hex(4)?, hex(6)?])),
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        let [r, g, b, a] = self.0;
        if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
    }

    /// Та же краска с умноженной прозрачностью.
    pub fn with_alpha(self, k: f32) -> Self {
        let [r, g, b, a] = self.0;
        Self([r, g, b, (a as f32 * k.clamp(0.0, 1.0)).round() as u8])
    }

    /// Линейная интерполяция в sRGB (для анимации смены темы этого достаточно).
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mut out = [0u8; 4];
        for (i, o) in out.iter_mut().enumerate() {
            let a = self.0[i] as f32;
            let b = other.0[i] as f32;
            *o = (a + (b - a) * t).round() as u8;
        }
        Self(out)
    }

    /// Наложение `top` поверх `self` (оба непрозрачные на выходе, если `self` непрозрачный).
    pub fn over(self, top: Self) -> Self {
        let ta = top.a() as f32 / 255.0;
        let mut out = self;
        for i in 0..3 {
            out.0[i] = (self.0[i] as f32 * (1.0 - ta) + top.0[i] as f32 * ta).round() as u8;
        }
        out
    }

    /// Относительная яркость по WCAG.
    pub fn luminance(self) -> f32 {
        let f = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.039_28 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * f(self.r()) + 0.7152 * f(self.g()) + 0.0722 * f(self.b())
    }

    /// Белый или почти чёрный — что контрастнее на этом фоне.
    pub fn readable_on(self) -> Self {
        let l = self.luminance();
        let white = 1.05 / (l + 0.05);
        let black = (l + 0.05) / 0.05;
        if white >= black * 0.75 { Self::WHITE } else { Self::rgb(20, 17, 16) }
    }
}

impl fmt::Debug for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Rgba {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgba::parse(&s).ok_or_else(|| serde::de::Error::custom(format!("неверный цвет «{s}», ожидается #rrggbb или #rrggbbaa")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_forms() {
        assert_eq!(Rgba::parse("#ff7a45"), Some(Rgba([255, 122, 69, 255])));
        assert_eq!(Rgba::parse("ff7a4580"), Some(Rgba([255, 122, 69, 128])));
        assert_eq!(Rgba::parse("#fff"), Some(Rgba::WHITE));
        assert_eq!(Rgba::parse("#zzzzzz"), None);
        assert_eq!(Rgba::parse("#12345"), None);
    }

    #[test]
    fn hex_roundtrip() {
        for s in ["#0f0f12", "#ffffff0f", "#8b7bff"] {
            assert_eq!(Rgba::parse(s).map(Rgba::to_hex).as_deref(), Some(s));
        }
    }

    #[test]
    fn lerp_ends() {
        let a = Rgba::rgb(0, 0, 0);
        let b = Rgba::rgb(200, 100, 50);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5), Rgba::rgb(100, 50, 25));
    }
}
