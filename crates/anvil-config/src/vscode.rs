//! Импорт цветовых тем VS Code (`*.json` из папки расширения, формат JSONC).
//!
//! Берём `colors` (цвета интерфейса) и `tokenColors` (TextMate-скоупы) и
//! переводим в `ThemeDef`. Результат — обычный TOML в папке `themes/`.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::ConfigError;
use crate::color::Rgba;
use crate::settings::{SyntaxOverride, SyntaxStyleSpec};
use crate::theme::{BaseColors, ThemeDef};

/// Убирает `//` и `/* */` комментарии и висячие запятые — JSON в темах VS Code «нестрогий».
pub fn strip_jsonc(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' && i + 1 < b.len() {
                out.push_str(&src[i..i + 2]);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            let ch_len = utf8_len(c);
            out.push_str(&src[i..i + ch_len]);
            i += ch_len;
            continue;
        }
        if c == b'"' {
            in_str = true;
            out.push('"');
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else if c == b',' {
            let mut j = i + 1;
            while j < b.len() && (b[j] as char).is_ascii_whitespace() {
                j += 1;
            }
            if !matches!(b.get(j), Some(b'}') | Some(b']')) {
                out.push(',');
            }
            i += 1;
        } else {
            let ch_len = utf8_len(c);
            out.push_str(&src[i..i + ch_len]);
            i += ch_len;
        }
    }
    out
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// Какие скоупы TextMate смотреть для каждого нашего класса (по убыванию приоритета).
const SCOPES: &[(&str, &[&str])] = &[
    ("keyword", &["keyword.control", "keyword", "storage.type", "storage.modifier"]),
    ("function", &["entity.name.function", "support.function", "meta.function-call"]),
    ("type", &["entity.name.type", "entity.name.class", "support.type", "support.class", "storage.type"]),
    ("string", &["string.quoted", "string"]),
    ("number", &["constant.numeric"]),
    ("comment", &["comment.line", "comment"]),
    ("macro", &["entity.name.function.macro", "support.function.macro", "entity.name.function.preprocessor"]),
    ("attribute", &["entity.other.attribute-name", "meta.attribute"]),
    ("punctuation", &["punctuation.separator", "punctuation"]),
    ("operator", &["keyword.operator"]),
    ("variable", &["variable.other", "variable"]),
    ("parameter", &["variable.parameter"]),
    ("property", &["variable.other.property", "variable.other.member", "support.variable.property"]),
    ("constant", &["variable.other.constant", "constant.other", "constant"]),
    ("builtin", &["constant.language", "variable.language"]),
    ("tag", &["entity.name.tag"]),
    ("escape", &["constant.character.escape"]),
    ("module", &["entity.name.namespace", "entity.name.module"]),
    ("heading", &["markup.heading", "entity.name.section"]),
    ("link", &["markup.underline.link"]),
    ("emphasis", &["markup.italic"]),
];

struct Rule {
    scope: String,
    color: Option<Rgba>,
    italic: Option<bool>,
}

fn collect_rules(v: &Value) -> Vec<Rule> {
    let mut rules = Vec::new();
    let Some(arr) = v.get("tokenColors").and_then(Value::as_array) else { return rules };
    for item in arr {
        let settings = item.get("settings");
        let color = settings.and_then(|s| s.get("foreground")).and_then(Value::as_str).and_then(Rgba::parse);
        let italic = settings.and_then(|s| s.get("fontStyle")).and_then(Value::as_str).map(|f| f.contains("italic"));
        let scopes: Vec<String> = match item.get("scope") {
            Some(Value::String(s)) => s.split(',').map(|x| x.trim().to_string()).collect(),
            Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(|x| x.trim().to_string()).collect(),
            _ => Vec::new(),
        };
        for scope in scopes {
            // селекторы-потомки («meta.x string») упрощаем до последнего скоупа
            let last = scope.split_whitespace().last().unwrap_or_default().to_string();
            if !last.is_empty() {
                rules.push(Rule { scope: last, color, italic });
            }
        }
    }
    rules
}

/// Самое специфичное правило, чей скоуп является префиксом `probe`.
fn best_rule<'a>(rules: &'a [Rule], probe: &str) -> Option<&'a Rule> {
    rules.iter().filter(|r| r.color.is_some() && (probe == r.scope || probe.starts_with(&format!("{}.", r.scope)))).max_by_key(|r| r.scope.len())
}

pub fn import_vscode_theme(src: &str, fallback_name: &str) -> Result<ThemeDef, ConfigError> {
    let v: Value = serde_json::from_str(&strip_jsonc(src))?;
    let colors = v.get("colors").cloned().unwrap_or(Value::Null);
    let col = |keys: &[&str]| keys.iter().find_map(|k| colors.get(*k).and_then(Value::as_str).and_then(Rgba::parse));

    let dark = match v.get("type").and_then(Value::as_str) {
        Some(t) => !t.contains("light"),
        None => col(&["editor.background"]).map(|c| c.luminance() < 0.4).unwrap_or(true),
    };
    let defaults = BaseColors::default();
    let bg = col(&["editor.background"]).unwrap_or(defaults.bg);
    let text = col(&["editor.foreground", "foreground"]).unwrap_or(defaults.text);
    let base = BaseColors {
        bg,
        surface: col(&["sideBar.background", "activityBar.background", "editorGroupHeader.tabsBackground"]).unwrap_or(bg),
        popup: col(&["quickInput.background", "editorWidget.background", "dropdown.background"]).unwrap_or(bg),
        border: col(&["panel.border", "sideBar.border", "editorGroup.border", "contrastBorder"]).unwrap_or(text.with_alpha(0.07)),
        text,
        muted: col(&["descriptionForeground", "tab.inactiveForeground", "sideBarSectionHeader.foreground"]).unwrap_or(text.with_alpha(0.62)),
        faint: col(&["editorLineNumber.foreground", "disabledForeground"]).unwrap_or(text.with_alpha(0.38)),
        accent: col(&["button.background", "focusBorder", "activityBarBadge.background", "textLink.foreground"]).unwrap_or(defaults.accent),
        ok: col(&["gitDecoration.addedResourceForeground", "terminal.ansiGreen"]).unwrap_or(defaults.ok),
        warn: col(&["editorWarning.foreground", "terminal.ansiYellow"]).unwrap_or(defaults.warn),
        err: col(&["editorError.foreground", "errorForeground", "terminal.ansiRed"]).unwrap_or(defaults.err),
    };

    let mut ui = BTreeMap::new();
    let mut put = |key: &str, vs: &[&str]| {
        if let Some(c) = col(vs) {
            ui.insert(key.to_string(), c);
        }
    };
    put("titlebar", &["titleBar.activeBackground"]);
    put("tabs", &["editorGroupHeader.tabsBackground", "tab.inactiveBackground"]);
    put("tab_active", &["tab.activeBackground"]);
    put("statusbar", &["statusBar.background"]);
    put("panel", &["panel.background", "terminal.background"]);
    put("selection", &["editor.selectionBackground"]);
    put("current_line", &["editor.lineHighlightBackground"]);
    put("line_number_active", &["editorLineNumber.activeForeground"]);
    put("indent_guide", &["editorIndentGuide.background1", "editorIndentGuide.background"]);
    put("indent_guide_active", &["editorIndentGuide.activeBackground1", "editorIndentGuide.activeBackground"]);
    put("caret", &["editorCursor.foreground"]);
    put("find_match", &["editor.findMatchHighlightBackground"]);
    put("find_match_active", &["editor.findMatchBackground"]);
    put("bracket_match", &["editorBracketMatch.background"]);

    let rules = collect_rules(&v);
    let mut syntax = BTreeMap::new();
    for (class, probes) in SCOPES {
        if let Some(rule) = probes.iter().find_map(|p| best_rule(&rules, p)) {
            let spec = SyntaxStyleSpec { color: rule.color, italic: rule.italic, underline: None };
            let ov = if spec.italic.is_some() { SyntaxOverride::Style(spec) } else { SyntaxOverride::Color(rule.color.unwrap_or(text)) };
            syntax.insert((*class).to_string(), ov);
        }
    }

    let name = v.get("name").and_then(Value::as_str).unwrap_or(fallback_name).to_string();
    Ok(ThemeDef { name, dark, base, ui, syntax })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"{
        // тема с комментариями и висячими запятыми
        "name": "Sample Dark",
        "type": "dark",
        "colors": {
            "editor.background": "#1e1e2e",
            "editor.foreground": "#cdd6f4",
            "sideBar.background": "#181825",
            "button.background": "#89b4fa", /* акцент */
            "statusBar.background": "#11111b",
        },
        "tokenColors": [
            { "scope": ["keyword", "storage.type"], "settings": { "foreground": "#cba6f7" } },
            { "scope": "keyword.control", "settings": { "foreground": "#f38ba8" } },
            { "scope": "comment", "settings": { "foreground": "#6c7086", "fontStyle": "italic" } },
            { "scope": "string", "settings": { "foreground": "#a6e3a1" } },
            { "scope": "meta.block entity.name.function", "settings": { "foreground": "#89b4fa" } },
        ],
    }"##;

    #[test]
    fn imports_sample_theme() -> Result<(), ConfigError> {
        let t = import_vscode_theme(SAMPLE, "x")?;
        assert_eq!(t.name, "Sample Dark");
        assert!(t.dark);
        assert_eq!(t.base.bg, Rgba::rgb(0x1e, 0x1e, 0x2e));
        assert_eq!(t.base.accent, Rgba::rgb(0x89, 0xb4, 0xfa));
        assert_eq!(t.ui.get("statusbar"), Some(&Rgba::rgb(0x11, 0x11, 0x1b)));
        // keyword.control специфичнее keyword
        assert_eq!(t.syntax.get("keyword"), Some(&SyntaxOverride::Color(Rgba::rgb(0xf3, 0x8b, 0xa8))));
        assert!(matches!(t.syntax.get("comment"), Some(SyntaxOverride::Style(s)) if s.italic == Some(true)));
        assert_eq!(t.syntax.get("function"), Some(&SyntaxOverride::Color(Rgba::rgb(0x89, 0xb4, 0xfa))));
        // результат сериализуется в TOML и читается обратно
        let back: ThemeDef = toml::from_str(&t.to_toml())?;
        assert_eq!(back, t);
        Ok(())
    }

    #[test]
    fn jsonc_keeps_strings_intact() {
        let s = strip_jsonc(r#"{"a": "http://x//y", "b": "/* no */", }"#);
        assert_eq!(s, r#"{"a": "http://x//y", "b": "/* no */" }"#);
    }
}
