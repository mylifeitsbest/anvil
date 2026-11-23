//! Цвета для отрисовки и плавная смена темы.

use anvil_config::{ResolvedTheme, Rgba, Settings};
use eframe::egui::Color32;

use crate::anim::{Spec, ease};

pub fn c32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), c.a())
}

pub fn with_alpha(c: Color32, k: f32) -> Color32 {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * k.clamp(0.0, 1.0)).round() as u8)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let a = a.to_srgba_unmultiplied();
    let b = b.to_srgba_unmultiplied();
    let l = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(l(0), l(1), l(2), l(3))
}

#[derive(Clone, Copy, Debug)]
pub struct SynStyle {
    pub color: Color32,
    pub italic: bool,
    pub underline: bool,
}

macro_rules! colors {
    ($($f:ident),* $(,)?) => {
        /// Палитра в готовом для egui виде.
        #[derive(Clone, Debug)]
        pub struct Colors {
            $(pub $f: Color32,)*
            pub dark: bool,
            pub syntax: Vec<SynStyle>,
            pub brackets: Vec<Color32>,
            pub rainbow: Vec<Color32>,
            pub tags: Vec<(String, Color32)>,
        }

        impl Colors {
            pub fn new(t: &ResolvedTheme, s: &Settings) -> Self {
                let p = &t.palette;
                let op = s.window.opacity.clamp(0.3, 1.0);
                let surface = |c: Rgba| if op < 0.999 { c32(c.with_alpha(op)) } else { c32(c) };
                let mut out = Self {
                    $($f: c32(p.$f),)*
                    dark: t.dark,
                    syntax: t.syntax.iter().map(|st| SynStyle { color: c32(st.color), italic: st.italic, underline: st.underline }).collect(),
                    // на светлом фоне пастельные скобки теряются — затемняем
                    brackets: s.editor.bracket_colors.iter().map(|c| if t.dark { c32(*c) } else { c32(c.lerp(Rgba::BLACK, 0.38)) }).collect(),
                    rainbow: s.editor.rainbow_indent_colors.iter().map(|c| c32(*c)).collect(),
                    tags: s.editor.comment_tag_colors.iter().map(|(k, v)| (k.clone(), c32(*v))).collect(),
                };
                out.bg = surface(p.bg);
                out.sidebar = surface(p.sidebar);
                out.titlebar = surface(p.titlebar);
                out.tabs = surface(p.tabs);
                out.tab_active = surface(p.tab_active);
                out.statusbar = surface(p.statusbar);
                out.panel = surface(p.panel);
                if let Some(cc) = s.cursor.color {
                    out.caret = c32(cc);
                }
                // длинные метки (FIXME) проверяем раньше коротких (!)
                out.tags.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
                out
            }
        }
    };
}

colors!(
    bg,
    sidebar,
    titlebar,
    tabs,
    tab_active,
    statusbar,
    panel,
    popup,
    border,
    border_strong,
    text,
    text_muted,
    text_faint,
    hover,
    selection,
    current_line,
    indent_guide,
    indent_guide_active,
    line_number,
    line_number_active,
    accent,
    on_accent,
    caret,
    ok,
    warn,
    err,
    scrim,
    shadow,
    minimap_slider,
    find_match,
    find_match_active,
    bracket_match,
);

impl Colors {
    pub fn syn(&self, class: u8) -> SynStyle {
        self.syntax.get(class as usize).copied().unwrap_or(SynStyle { color: self.text, italic: false, underline: false })
    }
}

/// Текущая тема и переход между темами.
pub struct ThemeState {
    pub current: ResolvedTheme,
    from: ResolvedTheme,
    to: ResolvedTheme,
    start: f64,
    spec: Spec,
}

impl ThemeState {
    pub fn new(t: ResolvedTheme) -> Self {
        Self { current: t.clone(), from: t.clone(), to: t, start: 0.0, spec: Spec::instant() }
    }

    pub fn set(&mut self, t: ResolvedTheme, now: f64, spec: Spec) {
        if t == self.to {
            return;
        }
        self.from = self.current.clone();
        self.to = t;
        self.start = now;
        self.spec = spec;
        if spec.dur <= 0.0 {
            self.current = self.to.clone();
        }
    }

    /// Возвращает true, пока идёт переход.
    pub fn tick(&mut self, now: f64) -> bool {
        if self.current == self.to {
            return false;
        }
        let k = if self.spec.dur <= 0.0 { 1.0 } else { ((now - self.start) as f32 / self.spec.dur).clamp(0.0, 1.0) };
        if k >= 1.0 {
            self.current = self.to.clone();
            false
        } else {
            self.current = self.from.lerp(&self.to, ease(self.spec.easing, k));
            true
        }
    }
}
