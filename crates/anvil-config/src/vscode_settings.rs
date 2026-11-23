//! Перенос из VS Code: пользовательский `settings.json` и темы из установленных расширений.

use std::path::PathBuf;

use serde_json::Value;

use crate::ConfigError;
use crate::vscode::strip_jsonc;

/// Значение настройки для записи в `settings.toml`.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Str(String),
    Num(f64),
    Bool(bool),
}

/// Одна перенесённая настройка: путь в `settings.toml` и значение.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub path: Vec<&'static str>,
    pub value: SettingValue,
    /// Откуда взято — для отчёта пользователю.
    pub from: &'static str,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

/// Где лежит `settings.json` VS Code и его форков на этой машине.
pub fn settings_candidates() -> Vec<PathBuf> {
    let base: Option<PathBuf> = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".config")))
    };
    let Some(b) = base else { return Vec::new() };
    ["Code", "Code - Insiders", "VSCodium", "Cursor", "Windsurf"].iter().map(|p| b.join(p).join("User").join("settings.json")).collect()
}

/// Папки с установленными расширениями.
pub fn extension_dirs() -> Vec<PathBuf> {
    home()
        .map(|h| [".vscode", ".vscode-insiders", ".vscode-oss", ".cursor", ".windsurf"].iter().map(|d| h.join(d).join("extensions")).collect())
        .unwrap_or_default()
}

/// Все темы из установленных расширений: (название, путь к JSON).
pub fn find_themes() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for dir in extension_dirs() {
        let Ok(exts) = std::fs::read_dir(&dir) else { continue };
        for ext in exts.flatten() {
            let Ok(text) = std::fs::read_to_string(ext.path().join("package.json")) else { continue };
            let Ok(v) = serde_json::from_str::<Value>(&strip_jsonc(&text)) else { continue };
            let Some(themes) = v.pointer("/contributes/themes").and_then(Value::as_array) else { continue };
            for t in themes {
                let (Some(label), Some(path)) = (t.get("label").and_then(Value::as_str), t.get("path").and_then(Value::as_str)) else { continue };
                let full = ext.path().join(path.trim_start_matches("./"));
                if full.is_file() {
                    out.push((label.to_string(), full));
                }
            }
        }
    }
    out.sort_by_key(|a| a.0.to_lowercase());
    out.dedup_by(|a, b| a.1 == b.1);
    out
}

/// Переводит пользовательский `settings.json` VS Code в ключи Anvil.
pub fn import_settings(src: &str) -> Result<Vec<Imported>, ConfigError> {
    use SettingValue::{Bool, Num, Str};
    let v: Value = serde_json::from_str(&strip_jsonc(src))?;
    let get = |k: &str| v.get(k);
    let mut out: Vec<Imported> = Vec::new();
    let mut put = |path: &[&'static str], value: SettingValue, from: &'static str| out.push(Imported { path: path.to_vec(), value, from });

    if let Some(f) = get("editor.fontFamily").and_then(Value::as_str) {
        let first = f.split(',').next().unwrap_or_default().trim().trim_matches(['\'', '"']).to_string();
        if !first.is_empty() && !first.eq_ignore_ascii_case("monospace") {
            put(&["editor", "font"], Str(first), "editor.fontFamily");
        }
    }
    let font_size = get("editor.fontSize").and_then(Value::as_f64);
    if let Some(s) = font_size {
        put(&["editor", "font_size"], Num(s), "editor.fontSize");
    }
    if let Some(lh) = get("editor.lineHeight").and_then(Value::as_f64).filter(|x| *x > 0.0) {
        // в VS Code значения < 8 — множитель, иначе пиксели
        let mult = if lh < 8.0 { lh } else { lh / font_size.unwrap_or(14.0) };
        put(&["editor", "line_height"], Num((mult * 100.0).round() / 100.0), "editor.lineHeight");
    }
    if let Some(ls) = get("editor.letterSpacing").and_then(Value::as_f64) {
        put(&["editor", "letter_spacing"], Num(ls), "editor.letterSpacing");
    }
    if let Some(t) = get("editor.tabSize").and_then(Value::as_f64) {
        put(&["editor", "tab_size"], Num(t), "editor.tabSize");
    }
    if let Some(b) = get("editor.insertSpaces").and_then(Value::as_bool) {
        put(&["editor", "insert_spaces"], Bool(b), "editor.insertSpaces");
    }
    if let Some(s) = get("editor.cursorStyle").and_then(Value::as_str) {
        let style = match s {
            "block" => "block",
            "underline" | "underline-thin" => "underline",
            "block-outline" => "hollow",
            _ => "line",
        };
        put(&["cursor", "style"], Str(style.into()), "editor.cursorStyle");
    }
    if let Some(w) = get("editor.cursorWidth").and_then(Value::as_f64).filter(|w| *w > 0.0) {
        put(&["cursor", "width"], Num(w), "editor.cursorWidth");
    }
    if let Some(s) = get("editor.cursorBlinking").and_then(Value::as_str) {
        let b = match s {
            "smooth" | "phase" | "expand" | "solid" => s,
            _ => "blink",
        };
        put(&["cursor", "blink"], Str(b.into()), "editor.cursorBlinking");
    }
    match get("editor.cursorSmoothCaretAnimation") {
        Some(Value::Bool(b)) => put(&["cursor", "smooth"], Bool(*b), "editor.cursorSmoothCaretAnimation"),
        Some(Value::String(s)) => put(&["cursor", "smooth"], Bool(s != "off"), "editor.cursorSmoothCaretAnimation"),
        _ => {}
    }
    if let Some(b) = get("editor.minimap.enabled").and_then(Value::as_bool) {
        put(&["editor", "minimap", "enabled"], Bool(b), "editor.minimap.enabled");
    }
    if let Some(s) = get("editor.renderWhitespace").and_then(Value::as_str) {
        let w = match s {
            "all" | "none" => s,
            _ => "boundary",
        };
        put(&["editor", "render_whitespace"], Str(w.into()), "editor.renderWhitespace");
    }
    if let Some(s) = get("editor.lineNumbers").and_then(Value::as_str) {
        let l = match s {
            "off" | "relative" => s,
            _ => "on",
        };
        put(&["editor", "line_numbers"], Str(l.into()), "editor.lineNumbers");
    }
    if let Some(s) = get("editor.renderLineHighlight").and_then(Value::as_str) {
        let c = match s {
            "none" => "none",
            "gutter" => "gutter",
            "line" => "border",
            _ => "fill",
        };
        put(&["editor", "current_line"], Str(c.into()), "editor.renderLineHighlight");
    }
    let flags: &[(&'static str, &[&'static str])] = &[
        ("editor.bracketPairColorization.enabled", &["editor", "bracket_colorization"]),
        ("editor.guides.indentation", &["editor", "indent_guides"]),
        ("editor.smoothScrolling", &["animations", "smooth_scroll"]),
        ("editor.scrollBeyondLastLine", &["editor", "scroll_beyond_last_line"]),
        ("files.trimTrailingWhitespace", &["files", "trim_trailing_whitespace"]),
        ("files.insertFinalNewline", &["files", "insert_final_newline"]),
    ];
    for (k, path) in flags {
        if let Some(b) = get(k).and_then(Value::as_bool) {
            put(path, Bool(b), k);
        }
    }
    if let Some(s) = get("editor.autoClosingBrackets").and_then(Value::as_str) {
        put(&["editor", "auto_close_brackets"], Bool(s != "never"), "editor.autoClosingBrackets");
    }
    if let Some(s) = get("editor.autoClosingQuotes").and_then(Value::as_str) {
        put(&["editor", "auto_close_quotes"], Bool(s != "never"), "editor.autoClosingQuotes");
    }
    if let Some(s) = get("workbench.sideBar.location").and_then(Value::as_str) {
        put(&["ui", "sidebar"], Str(if s == "right" { "right".into() } else { "left".into() }), "workbench.sideBar.location");
    }
    if let Some(s) = get("files.autoSave").and_then(Value::as_str) {
        let a = match s {
            "afterDelay" => "delay",
            "onFocusChange" | "onWindowChange" => "focus",
            _ => "off",
        };
        put(&["files", "auto_save"], Str(a.into()), "files.autoSave");
    }
    if let Some(d) = get("files.autoSaveDelay").and_then(Value::as_f64) {
        put(&["files", "auto_save_delay_ms"], Num(d), "files.autoSaveDelay");
    }
    if let Some(z) = get("window.zoomLevel").and_then(Value::as_f64) {
        put(&["ui", "scale"], Num((1.2f64.powf(z) * 100.0).round() / 100.0), "window.zoomLevel");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_vscode_settings() -> Result<(), ConfigError> {
        let src = r#"{
            // мои настройки
            "editor.fontFamily": "'JetBrains Mono', Consolas, monospace",
            "editor.fontSize": 15,
            "editor.lineHeight": 24,
            "editor.cursorStyle": "block",
            "editor.cursorBlinking": "phase",
            "editor.cursorSmoothCaretAnimation": "on",
            "editor.minimap.enabled": false,
            "editor.smoothScrolling": true,
            "workbench.sideBar.location": "right",
            "files.autoSave": "afterDelay",
            "window.zoomLevel": 1,
        }"#;
        let out = import_settings(src)?;
        let find = |p: &[&str]| out.iter().find(|i| i.path == p).map(|i| i.value.clone());
        assert_eq!(find(&["editor", "font"]), Some(SettingValue::Str("JetBrains Mono".into())));
        assert_eq!(find(&["editor", "line_height"]), Some(SettingValue::Num(1.6)));
        assert_eq!(find(&["cursor", "style"]), Some(SettingValue::Str("block".into())));
        assert_eq!(find(&["cursor", "blink"]), Some(SettingValue::Str("phase".into())));
        assert_eq!(find(&["cursor", "smooth"]), Some(SettingValue::Bool(true)));
        assert_eq!(find(&["editor", "minimap", "enabled"]), Some(SettingValue::Bool(false)));
        assert_eq!(find(&["animations", "smooth_scroll"]), Some(SettingValue::Bool(true)));
        assert_eq!(find(&["ui", "sidebar"]), Some(SettingValue::Str("right".into())));
        assert_eq!(find(&["files", "auto_save"]), Some(SettingValue::Str("delay".into())));
        assert_eq!(find(&["ui", "scale"]), Some(SettingValue::Num(1.2)));
        Ok(())
    }
}
