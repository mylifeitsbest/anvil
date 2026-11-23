//! Темы: описание в TOML (`ThemeDef`), разрешённая палитра (`Palette`) и стили подсветки.
//!
//! Тема задаёт небольшой набор базовых цветов, остальное выводится из них.
//! Любой ключ палитры можно переопределить в теме (`[ui]`) или в настройках (`[colors]`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::settings::{Settings, SyntaxOverride};

/// Классы подсветки. Индекс в этом массиве — номер класса в `ResolvedTheme::syntax`.
pub const SYNTAX_KEYS: &[&str] = &[
    "text",
    "keyword",
    "function",
    "type",
    "string",
    "number",
    "comment",
    "macro",
    "attribute",
    "punctuation",
    "operator",
    "variable",
    "parameter",
    "property",
    "constant",
    "builtin",
    "label",
    "tag",
    "escape",
    "module",
    "heading",
    "link",
    "emphasis",
];

/// Если класс не задан в теме — откуда взять цвет.
fn syntax_fallback(key: &str) -> &'static str {
    match key {
        "parameter" | "property" => "variable",
        "constant" | "escape" => "number",
        "builtin" | "tag" | "heading" => "keyword",
        "label" => "attribute",
        "module" => "type",
        "link" => "function",
        "emphasis" => "string",
        "operator" => "punctuation",
        _ => "text",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BaseColors {
    pub bg: Rgba,
    pub surface: Rgba,
    pub popup: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub faint: Rgba,
    pub accent: Rgba,
    pub ok: Rgba,
    pub warn: Rgba,
    pub err: Rgba,
}

impl Default for BaseColors {
    fn default() -> Self {
        Self {
            bg: Rgba::rgb(15, 15, 18),
            surface: Rgba::rgb(11, 11, 14),
            popup: Rgba::rgb(23, 23, 28),
            border: Rgba([255, 255, 255, 15]),
            text: Rgba::rgb(231, 231, 235),
            muted: Rgba::rgb(142, 142, 154),
            faint: Rgba::rgb(85, 85, 95),
            accent: Rgba::rgb(255, 122, 69),
            ok: Rgba::rgb(74, 210, 149),
            warn: Rgba::rgb(245, 192, 74),
            err: Rgba::rgb(255, 92, 92),
        }
    }
}

/// Тема в том виде, в каком она лежит в `themes/*.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeDef {
    pub name: String,
    pub dark: bool,
    pub base: BaseColors,
    /// Переопределение любых ключей палитры.
    pub ui: BTreeMap<String, Rgba>,
    pub syntax: BTreeMap<String, SyntaxOverride>,
}

impl Default for ThemeDef {
    fn default() -> Self {
        Self { name: "Без названия".into(), dark: true, base: BaseColors::default(), ui: BTreeMap::new(), syntax: BTreeMap::new() }
    }
}

impl ThemeDef {
    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

macro_rules! palette {
    ($($field:ident),* $(,)?) => {
        /// Разрешённые цвета интерфейса.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Palette { $(pub $field: Rgba,)* }

        impl Palette {
            /// Все ключи, которые можно указать в `[colors]`.
            pub const KEYS: &'static [&'static str] = &[$(stringify!($field)),*];

            pub fn set(&mut self, key: &str, c: Rgba) -> bool {
                match key { $(stringify!($field) => { self.$field = c; true })* _ => false }
            }

            pub fn get(&self, key: &str) -> Option<Rgba> {
                match key { $(stringify!($field) => Some(self.$field),)* _ => None }
            }

            pub fn lerp(&self, o: &Self, t: f32) -> Self {
                Self { $($field: self.$field.lerp(o.$field, t),)* }
            }
        }
    };
}

palette!(
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

impl Palette {
    pub fn derive(b: &BaseColors, dark: bool) -> Self {
        let t = b.text;
        Self {
            bg: b.bg,
            sidebar: b.surface,
            titlebar: b.surface,
            tabs: b.surface,
            tab_active: b.bg,
            statusbar: b.surface,
            panel: b.surface,
            popup: b.popup,
            border: b.border,
            border_strong: t.with_alpha(if dark { 0.11 } else { 0.14 }),
            text: t,
            text_muted: b.muted,
            text_faint: b.faint,
            hover: t.with_alpha(0.05),
            selection: b.accent.with_alpha(if dark { 0.26 } else { 0.22 }),
            current_line: t.with_alpha(if dark { 0.03 } else { 0.035 }),
            indent_guide: t.with_alpha(if dark { 0.055 } else { 0.08 }),
            indent_guide_active: t.with_alpha(if dark { 0.16 } else { 0.22 }),
            line_number: b.faint,
            line_number_active: b.muted,
            accent: b.accent,
            on_accent: b.accent.readable_on(),
            caret: b.accent,
            ok: b.ok,
            warn: b.warn,
            err: b.err,
            scrim: Rgba([0, 0, 0, if dark { 90 } else { 30 }]),
            shadow: Rgba([0, 0, 0, if dark { 150 } else { 50 }]),
            minimap_slider: t.with_alpha(if dark { 0.06 } else { 0.08 }),
            find_match: b.warn.with_alpha(0.25),
            find_match_active: b.warn.with_alpha(0.55),
            bracket_match: t.with_alpha(0.14),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyntaxStyle {
    pub color: Rgba,
    pub italic: bool,
    pub underline: bool,
}

impl SyntaxStyle {
    pub fn lerp(&self, o: &Self, t: f32) -> Self {
        Self {
            color: self.color.lerp(o.color, t),
            italic: if t < 0.5 { self.italic } else { o.italic },
            underline: if t < 0.5 { self.underline } else { o.underline },
        }
    }
}

/// Тема после применения всех настроек — то, чем рисует интерфейс.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub palette: Palette,
    pub syntax: Vec<SyntaxStyle>,
}

impl ResolvedTheme {
    pub fn lerp(&self, o: &Self, t: f32) -> Self {
        Self {
            id: o.id.clone(),
            name: o.name.clone(),
            dark: if t < 0.5 { self.dark } else { o.dark },
            palette: self.palette.lerp(&o.palette, t),
            syntax: self.syntax.iter().zip(&o.syntax).map(|(a, b)| a.lerp(b, t)).collect(),
        }
    }

    pub fn syntax_index(key: &str) -> Option<usize> {
        SYNTAX_KEYS.iter().position(|k| *k == key)
    }
}

fn apply_override(style: &mut SyntaxStyle, ov: &SyntaxOverride) {
    match ov {
        SyntaxOverride::Color(c) => style.color = *c,
        SyntaxOverride::Style(s) => {
            if let Some(c) = s.color {
                style.color = c;
            }
            if let Some(i) = s.italic {
                style.italic = i;
            }
            if let Some(u) = s.underline {
                style.underline = u;
            }
        }
    }
}

/// Сводит тему и настройки в готовую палитру. Возвращает также список нераспознанных ключей.
pub fn resolve(id: &str, def: &ThemeDef, settings: &Settings) -> (ResolvedTheme, Vec<String>) {
    let mut warnings = Vec::new();
    let mut base = def.base;
    if let Some(a) = settings.theme.accent {
        base.accent = a;
    }
    let mut p = Palette::derive(&base, def.dark);
    for (k, c) in &def.ui {
        if !p.set(k, *c) {
            warnings.push(format!("тема {id}: неизвестный ключ ui.{k}"));
        }
    }
    if settings.theme.accent.is_some() {
        p.accent = base.accent;
        p.caret = base.accent;
        p.on_accent = base.accent.readable_on();
        p.selection = base.accent.with_alpha(if def.dark { 0.26 } else { 0.22 });
    }
    for (k, c) in &settings.colors {
        if !p.set(k, *c) {
            warnings.push(format!("[colors]: неизвестный ключ «{k}». Доступны: {}", Palette::KEYS.join(", ")));
        }
    }
    if let Some(tint) = settings.theme.tint {
        let s = settings.theme.tint_strength.clamp(0.0, 1.0);
        p.titlebar = p.titlebar.over(tint.with_alpha(s));
        p.statusbar = p.statusbar.over(tint.with_alpha(s));
    }
    let contrast = settings.theme.contrast.clamp(0.5, 1.5);
    if (contrast - 1.0).abs() > 0.001 {
        let push = |c: Rgba| -> Rgba {
            let mut out = c;
            for i in 0..3 {
                let v = p.bg.0[i] as f32 + (c.0[i] as f32 - p.bg.0[i] as f32) * contrast;
                out.0[i] = v.clamp(0.0, 255.0).round() as u8;
            }
            out
        };
        p.text = push(p.text);
        p.text_muted = push(p.text_muted);
        p.text_faint = push(p.text_faint);
    }

    // подсветка: сначала явные цвета темы, потом запасные варианты, потом настройки пользователя
    let mut syntax: Vec<SyntaxStyle> = SYNTAX_KEYS.iter().map(|_| SyntaxStyle { color: p.text, italic: false, underline: false }).collect();
    let mut defined = vec![false; SYNTAX_KEYS.len()];
    for (k, ov) in &def.syntax {
        match ResolvedTheme::syntax_index(k) {
            Some(i) => {
                apply_override(&mut syntax[i], ov);
                defined[i] = true;
            }
            None => warnings.push(format!("тема {id}: неизвестный класс подсветки «{k}»")),
        }
    }
    for i in 0..SYNTAX_KEYS.len() {
        if !defined[i] {
            let mut key = SYNTAX_KEYS[i];
            for _ in 0..3 {
                key = syntax_fallback(key);
                if let Some(j) = ResolvedTheme::syntax_index(key)
                    && (defined[j] || key == "text")
                {
                    syntax[i] = syntax[j];
                    break;
                }
            }
        }
    }
    if let Some(i) = ResolvedTheme::syntax_index("text")
        && !defined[i]
    {
        syntax[i].color = p.text;
    }
    if let Some(i) = ResolvedTheme::syntax_index("comment")
        && !defined[i]
    {
        syntax[i] = SyntaxStyle { color: p.text_faint, italic: true, underline: false };
    }
    for (k, ov) in &settings.syntax {
        match ResolvedTheme::syntax_index(k) {
            Some(i) => apply_override(&mut syntax[i], ov),
            None => warnings.push(format!("[syntax]: неизвестный класс «{k}». Доступны: {}", SYNTAX_KEYS.join(", "))),
        }
    }
    (ResolvedTheme { id: id.to_string(), name: def.name.clone(), dark: def.dark, palette: p, syntax }, warnings)
}

// ---------------------------------------------------------------- реестр тем

const BUILTIN: &[(&str, &str)] = &[
    ("anvil-dark", include_str!("../themes/anvil-dark.toml")),
    ("anvil-light", include_str!("../themes/anvil-light.toml")),
    ("midnight", include_str!("../themes/midnight.toml")),
    ("paper", include_str!("../themes/paper.toml")),
    ("graphite", include_str!("../themes/graphite.toml")),
    ("oled", include_str!("../themes/oled.toml")),
];

#[derive(Debug, Clone)]
pub struct ThemeEntry {
    pub id: String,
    pub def: ThemeDef,
    pub builtin: bool,
}

/// Встроенные темы плюс пользовательские из папки `themes/`.
#[derive(Debug, Clone, Default)]
pub struct ThemeRegistry {
    pub themes: Vec<ThemeEntry>,
    pub warnings: Vec<String>,
}

impl ThemeRegistry {
    pub fn load(user_dir: Option<&Path>) -> Self {
        let mut reg = Self::default();
        for (id, src) in BUILTIN {
            match toml::from_str::<ThemeDef>(src) {
                Ok(def) => reg.themes.push(ThemeEntry { id: (*id).to_string(), def, builtin: true }),
                Err(e) => reg.warnings.push(format!("встроенная тема {id}: {e}")),
            }
        }
        if let Some(dir) = user_dir
            && let Ok(rd) = std::fs::read_dir(dir)
        {
            let mut files: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
            files.sort();
            for path in files {
                let id = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| toml::from_str::<ThemeDef>(&s).map_err(|e| e.to_string())) {
                    Ok(def) => {
                        reg.themes.retain(|t| t.id != id);
                        reg.themes.push(ThemeEntry { id, def, builtin: false });
                    }
                    Err(e) => reg.warnings.push(format!("тема {}: {e}", path.display())),
                }
            }
        }
        reg
    }

    pub fn get(&self, id: &str) -> Option<&ThemeEntry> {
        self.themes.iter().find(|t| t.id == id)
    }

    /// Тема по имени из настроек; если такой нет — `anvil-dark`.
    pub fn resolve(&self, settings: &Settings) -> (ResolvedTheme, Vec<String>) {
        let wanted = settings.theme.name.as_str();
        let entry = self.get(wanted).or_else(|| self.get("anvil-dark"));
        let mut warnings = Vec::new();
        if self.get(wanted).is_none() {
            warnings.push(format!("тема «{wanted}» не найдена, использую anvil-dark"));
        }
        let fallback = ThemeDef::default();
        let (id, def) = entry.map(|e| (e.id.as_str(), &e.def)).unwrap_or(("anvil-dark", &fallback));
        let (theme, mut w) = resolve(id, def, settings);
        warnings.append(&mut w);
        (theme, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_parse_and_resolve_cleanly() {
        let reg = ThemeRegistry::load(None);
        assert!(reg.warnings.is_empty(), "{:?}", reg.warnings);
        assert_eq!(reg.themes.len(), BUILTIN.len());
        for t in &reg.themes {
            let mut s = Settings::default();
            s.theme.name = t.id.clone();
            let (_, w) = reg.resolve(&s);
            assert!(w.is_empty(), "{}: {w:?}", t.id);
        }
    }

    #[test]
    fn accent_and_color_overrides() {
        let reg = ThemeRegistry::load(None);
        let mut s = Settings::default();
        let violet = Rgba::rgb(139, 123, 255);
        s.theme.accent = Some(violet);
        s.colors.insert("sidebar".into(), Rgba::rgb(1, 2, 3));
        s.colors.insert("nope".into(), Rgba::rgb(1, 2, 3));
        let (t, w) = reg.resolve(&s);
        assert_eq!(t.palette.accent, violet);
        assert_eq!(t.palette.caret, violet);
        assert_eq!(t.palette.sidebar, Rgba::rgb(1, 2, 3));
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn syntax_fallbacks_fill_every_class() {
        let def = ThemeDef::default();
        let (t, _) = resolve("x", &def, &Settings::default());
        assert_eq!(t.syntax.len(), SYNTAX_KEYS.len());
    }
}
