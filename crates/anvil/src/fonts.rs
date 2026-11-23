//! Шрифты: берём системные по имени (или путь к файлу), иначе встроенные в egui.
//! Ничего не качается и не встраивается в бинарник.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anvil_config::Settings;
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

pub const EDITOR: &str = "anvil-editor";
pub const UI: &str = "anvil-ui";
pub const UI_BOLD: &str = "anvil-ui-bold";

const KNOWN: &[(&str, &[&str])] = &[
    ("cascadia code", &["CascadiaCode.ttf", "CascadiaCode-Regular.ttf", "CascadiaCode-Regular.otf"]),
    ("cascadia mono", &["CascadiaMono.ttf", "CascadiaMono-Regular.ttf"]),
    ("jetbrains mono", &["JetBrainsMono-Regular.ttf", "JetBrainsMono[wght].ttf", "JetBrainsMonoNL-Regular.ttf"]),
    ("fira code", &["FiraCode-Regular.ttf", "FiraCode-VF.ttf", "FiraCode[wght].ttf"]),
    ("source code pro", &["SourceCodePro-Regular.ttf", "SourceCodePro-Regular.otf"]),
    ("consolas", &["consola.ttf"]),
    ("hack", &["Hack-Regular.ttf"]),
    ("dejavu sans mono", &["DejaVuSansMono.ttf"]),
    ("ubuntu mono", &["UbuntuMono-R.ttf", "UbuntuMono-Regular.ttf"]),
    ("liberation mono", &["LiberationMono-Regular.ttf"]),
    ("sf mono", &["SFNSMono.ttf", "SF-Mono-Regular.otf"]),
    ("segoe ui variable", &["SegUIVar.ttf"]),
    ("segoe ui", &["segoeui.ttf"]),
    ("segoe ui semibold", &["seguisb.ttf"]),
    ("inter", &["Inter-Regular.ttf", "InterVariable.ttf", "Inter.ttf", "Inter-Regular.otf"]),
    ("inter semibold", &["Inter-SemiBold.ttf", "Inter-SemiBold.otf"]),
    ("sf pro", &["SFNS.ttf", "SF-Pro-Text-Regular.otf"]),
    ("arial", &["arial.ttf", "Arial.ttf"]),
    ("dejavu sans", &["DejaVuSans.ttf"]),
    ("ubuntu", &["Ubuntu-R.ttf", "Ubuntu-Regular.ttf"]),
    ("noto sans", &["NotoSans-Regular.ttf"]),
    ("cantarell", &["Cantarell-Regular.otf", "Cantarell-VF.otf"]),
];

const AUTO_MONO: &[&str] = &["cascadia code", "jetbrains mono", "fira code", "sf mono", "consolas", "dejavu sans mono", "ubuntu mono"];
const AUTO_UI: &[&str] = &["segoe ui variable", "inter", "segoe ui", "sf pro", "ubuntu", "noto sans", "cantarell", "dejavu sans"];
const AUTO_BOLD: &[&str] = &["segoe ui semibold", "inter semibold"];

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
        v.push(win.join("Fonts"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
        }
    } else if cfg!(target_os = "macos") {
        v.push(PathBuf::from("/System/Library/Fonts"));
        v.push(PathBuf::from("/Library/Fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join("Library/Fonts"));
        }
    } else {
        v.push(PathBuf::from("/usr/share/fonts"));
        v.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(&h).join(".local/share/fonts"));
            v.push(PathBuf::from(h).join(".fonts"));
        }
    }
    v
}

fn find_file(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir()
            && let Some(f) = find_file(&p, name, depth - 1)
        {
            return Some(f);
        }
    }
    None
}

/// Путь к шрифту по имени семейства или прямой путь.
pub fn locate(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.extension().is_some() && p.is_file() {
        return Some(p.to_path_buf());
    }
    let key = name.trim().to_lowercase();
    let squashed: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    let mut candidates: Vec<String> = KNOWN.iter().find(|(k, _)| *k == key).map(|(_, f)| f.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
    candidates.extend([format!("{squashed}-Regular.ttf"), format!("{squashed}.ttf"), format!("{squashed}-Regular.otf"), format!("{squashed}.otf")]);
    let depth = if cfg!(any(windows, target_os = "macos")) { 0 } else { 3 };
    for dir in font_dirs() {
        for c in &candidates {
            if let Some(f) = find_file(&dir, c, depth) {
                return Some(f);
            }
        }
    }
    None
}

fn load_first(names: &[&str]) -> Option<(String, Vec<u8>)> {
    names.iter().find_map(|n| locate(n).and_then(|p| std::fs::read(&p).ok()).map(|b| ((*n).to_string(), b)))
}

/// Какие шрифты в итоге подключены — для строки состояния и тестов.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontReport {
    pub editor: String,
    pub ui: String,
}

/// Ключ, по которому видно, что шрифты надо перезагрузить.
pub fn fonts_key(s: &Settings) -> String {
    format!("{}|{}", s.editor.font, s.ui.font)
}

pub fn install(ctx: &egui::Context, s: &Settings) -> FontReport {
    let mut defs = FontDefinitions::default();
    let builtin_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let builtin_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut report = FontReport { editor: "встроенный".into(), ui: "встроенный".into() };

    let pick = |wanted: &str, auto: &[&str]| -> Option<(String, Vec<u8>)> {
        if wanted.trim().is_empty() || wanted.eq_ignore_ascii_case("auto") { load_first(auto) } else { load_first(&[wanted]).or_else(|| load_first(auto)) }
    };

    let mut mono_chain = Vec::new();
    if let Some((name, bytes)) = pick(&s.editor.font, AUTO_MONO) {
        defs.font_data.insert(EDITOR.into(), Arc::new(FontData::from_owned(bytes)));
        mono_chain.push(EDITOR.to_string());
        report.editor = name;
    }
    mono_chain.extend(builtin_mono.iter().cloned());

    let mut ui_chain = Vec::new();
    if let Some((name, bytes)) = pick(&s.ui.font, AUTO_UI) {
        defs.font_data.insert(UI.into(), Arc::new(FontData::from_owned(bytes)));
        ui_chain.push(UI.to_string());
        report.ui = name;
    }
    ui_chain.extend(builtin_prop.iter().cloned());

    let mut bold_chain = Vec::new();
    if let Some((_, bytes)) = load_first(AUTO_BOLD) {
        defs.font_data.insert(UI_BOLD.into(), Arc::new(FontData::from_owned(bytes)));
        bold_chain.push(UI_BOLD.to_string());
    }
    bold_chain.extend(ui_chain.iter().cloned());

    defs.families.insert(FontFamily::Name(EDITOR.into()), mono_chain.clone());
    defs.families.insert(FontFamily::Monospace, mono_chain);
    defs.families.insert(FontFamily::Name(UI.into()), ui_chain.clone());
    defs.families.insert(FontFamily::Proportional, ui_chain);
    defs.families.insert(FontFamily::Name(UI_BOLD.into()), bold_chain);
    ctx.set_fonts(defs);
    report
}

pub fn editor_family() -> FontFamily {
    FontFamily::Name(EDITOR.into())
}

pub fn ui_family() -> FontFamily {
    FontFamily::Name(UI.into())
}

pub fn bold_family() -> FontFamily {
    FontFamily::Name(UI_BOLD.into())
}
