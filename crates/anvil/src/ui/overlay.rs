//! Оверлеи: палитра команд (с живым предпросмотром тем) и панель поиска/замены.

use std::path::PathBuf;

use anvil_config::{Motion, Rgba, vscode_settings};
use anvil_core::{SearchOptions, search};
use eframe::egui::{self, Align2, Color32, CursorIcon, Event, Id, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use toml_edit::Value;

use super::{Icon, LineInput, cr, fuzzy, icon, paint_highlighted, shadow, text, text_width};
use crate::app::{AnvilApp, Focus};
use crate::commands::{COMMANDS, Cmd, Pick};
use crate::theme::{c32, mix, with_alpha};

#[derive(Clone, Debug, PartialEq)]
pub enum PalMode {
    Commands,
    Files,
    Themes {
        original: String,
    },
    Accent {
        original: Option<Rgba>,
    },
    GotoLine,
    Pick(Pick),
    Rename {
        doc: u64,
        at: usize,
        old: String,
    },
    Locations {
        title: String,
    },
    /// Совместная работа: ввести ссылку-приглашение.
    Join,
    /// Совместная работа: участники, приглашения, файлы сессии.
    Session,
}

impl PalMode {
    pub fn kind(&self) -> u8 {
        match self {
            PalMode::Commands => 0,
            PalMode::Files => 1,
            PalMode::Themes { .. } => 2,
            PalMode::Accent { .. } => 3,
            PalMode::GotoLine => 4,
            PalMode::Pick(_) => 5,
            PalMode::Rename { .. } => 6,
            PalMode::Locations { .. } => 7,
            PalMode::Join => 8,
            PalMode::Session => 9,
        }
    }
}

#[derive(Clone, Debug)]
pub enum PalAction {
    Cmd(Cmd),
    Open(PathBuf),
    Theme(String),
    Accent(Option<Rgba>),
    Set(Vec<&'static str>, Value),
    Goto(usize),
    ImportTheme(PathBuf),
    Location(PathBuf, anvil_lsp::lsp_types::Position, anvil_lsp::Enc),
    Rename(u64, usize),
    Collab(CollabAct),
    None,
}

#[derive(Clone, Debug)]
pub enum CollabAct {
    Join,
    CopyEdit,
    CopyRead,
    Peer(u32),
    Open(u64),
    Leave,
}

#[derive(Clone, Debug)]
pub struct PalItem {
    pub label: String,
    pub detail: String,
    pub hint: String,
    pub hl: Vec<usize>,
    pub action: PalAction,
    pub swatch: Vec<Color32>,
    pub current: bool,
}

pub struct Palette {
    pub mode: PalMode,
    pub input: LineInput,
    pub items: Vec<PalItem>,
    pub sel: usize,
    pub first: usize,
    pub opened: f64,
    pub closing: Option<f64>,
    pub previewed: Option<usize>,
    /// Готовый список (ссылки, определения) — фильтруется вводом.
    pub fixed: Vec<PalItem>,
}

impl Palette {
    pub fn new(mode: PalMode, now: f64) -> Self {
        Self { mode, input: LineInput::default(), items: Vec::new(), sel: 0, first: 0, opened: now, closing: None, previewed: None, fixed: Vec::new() }
    }

    pub fn mode_kind(&self) -> u8 {
        self.mode.kind()
    }
}

#[derive(Default)]
pub struct FindState {
    pub open: bool,
    pub replace: bool,
    pub query: LineInput,
    pub replacement: LineInput,
    pub opts: SearchOptions,
    pub opened: f64,
    pub error: Option<String>,
    pub current: usize,
}

const ACCENTS: &[(&str, &str)] = &[
    ("Ember — раскалённый", "#ff7a45"),
    ("Violet", "#8b7bff"),
    ("Cyan", "#2fc6f6"),
    ("Lime", "#9fd84a"),
    ("Rose", "#ff5c93"),
    ("Gold", "#f5c04a"),
    ("Ocean", "#3d8bfd"),
    ("Mint", "#3ddc97"),
    ("Mono", "#e6e6e6"),
];

fn pick_options(p: Pick) -> (&'static [&'static str], &'static str, &'static [(&'static str, &'static str)]) {
    // (путь в settings.toml, заголовок, [(значение, подпись)])
    match p {
        Pick::TypingEffect => (
            &["animations", "typing", "effect"],
            "Эффект при вводе",
            &[("none", "Без эффекта"), ("sparks", "Искры (как Power Mode)"), ("pop", "Кольцо-всплеск"), ("glow", "Мягкая вспышка")],
        ),
        Pick::CursorStyle => {
            (&["cursor", "style"], "Стиль курсора", &[("line", "Линия"), ("block", "Блок"), ("underline", "Подчёркивание"), ("hollow", "Контур")])
        }
        Pick::CursorBlink => (
            &["cursor", "blink"],
            "Мигание курсора",
            &[("smooth", "Плавное"), ("blink", "Классическое"), ("phase", "Фаза"), ("expand", "Дыхание"), ("solid", "Не мигать")],
        ),
        Pick::Density => (&["ui", "density"], "Плотность интерфейса", &[("compact", "Компактно"), ("comfortable", "Обычно"), ("spacious", "Просторно")]),
        Pick::Easing => (
            &["animations", "easing"],
            "Кривая анимаций",
            &[
                ("out-cubic", "Out Cubic — мягко"),
                ("out-quint", "Out Quint — быстрый старт"),
                ("in-out-cubic", "In-Out Cubic"),
                ("out-back", "Out Back — с перелётом"),
                ("spring", "Пружина"),
                ("linear", "Линейно"),
            ],
        ),
        Pick::TabStyle => (&["ui", "tab_style"], "Стиль вкладок", &[("underline", "Подчёркивание"), ("pill", "Пилюля"), ("classic", "Классика")]),
        Pick::LineNumbers => (&["editor", "line_numbers"], "Номера строк", &[("on", "Обычные"), ("relative", "Относительные"), ("off", "Скрыть")]),
        Pick::FileIcons => (&["ui", "file_icons"], "Значки файлов", &[("dots", "Цветные точки"), ("letters", "Буквы расширения"), ("none", "Без значков")]),
        Pick::Keymap => {
            (&["keymap", "preset"], "Горячие клавиши", &[("vscode", "VS Code"), ("jetbrains", "JetBrains (IntelliJ, PyCharm…)"), ("sublime", "Sublime Text")])
        }
        Pick::AnimSpeed => (
            &["animations", "speed"],
            "Скорость анимаций",
            &[
                ("0.5", "0.5× — медленно"),
                ("0.75", "0.75×"),
                ("1.0", "1× — по умолчанию"),
                ("1.5", "1.5×"),
                ("2.0", "2× — быстро"),
                ("3.0", "3× — почти мгновенно"),
            ],
        ),
        Pick::ImportVscodeTheme => (&[], "Тема из установленных расширений VS Code", &[]),
    }
}

fn v<T: serde::Serialize>(x: &T) -> String {
    toml::Value::try_from(x).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

impl AnvilApp {
    pub(crate) fn current_pick_value(&self, p: Pick) -> String {
        let s = &self.settings;
        match p {
            Pick::TypingEffect => v(&s.animations.typing.effect),
            Pick::CursorStyle => v(&s.cursor.style),
            Pick::CursorBlink => v(&s.cursor.blink),
            Pick::Density => v(&s.ui.density),
            Pick::Easing => v(&s.animations.easing),
            Pick::TabStyle => v(&s.ui.tab_style),
            Pick::LineNumbers => v(&s.editor.line_numbers),
            Pick::FileIcons => v(&s.ui.file_icons),
            Pick::Keymap => v(&s.keymap.preset),
            Pick::AnimSpeed => format!("{:.1}", s.animations.speed),
            Pick::ImportVscodeTheme => String::new(),
        }
    }

    pub(crate) fn refresh_palette(&mut self) {
        let Some(pal) = self.palette.as_ref() else { return };
        let q = pal.input.text.clone();
        let mode = pal.mode.clone();
        let mut items: Vec<(i32, PalItem)> = Vec::new();
        let item = |label: String, detail: String, hint: String, action: PalAction| PalItem {
            label,
            detail,
            hint,
            hl: Vec::new(),
            action,
            swatch: Vec::new(),
            current: false,
        };
        match &mode {
            PalMode::Commands => {
                for ci in COMMANDS {
                    if let Some((score, hl)) = fuzzy(q.trim_start_matches('>'), ci.title) {
                        let mut it = item(ci.title.to_string(), String::new(), self.keymap.label(ci.cmd).unwrap_or_default(), PalAction::Cmd(ci.cmd));
                        it.hl = hl;
                        items.push((score, it));
                    }
                }
            }
            PalMode::Files => {
                if let Some(rest) = q.strip_prefix('>') {
                    let p = self.palette.as_mut().map(|p| {
                        p.mode = PalMode::Commands;
                        p.input.set(rest);
                    });
                    let _ = p;
                    return self.refresh_palette();
                }
                if let Some(rest) = q.strip_prefix(':') {
                    let n: usize = rest.trim().parse().unwrap_or(0);
                    if n > 0 {
                        items.push((0, item(format!("Перейти к строке {n}"), String::new(), String::new(), PalAction::Goto(n))));
                    }
                } else {
                    let open: Vec<PathBuf> = self.docs.iter().filter_map(|d| d.path.clone()).collect();
                    for f in self.ws.files() {
                        let rel = self.ws.relative(&f);
                        let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let base_start = rel.chars().count() - name.chars().count();
                        if let Some((mut score, hl)) = fuzzy(&q, &rel) {
                            score += hl.iter().filter(|i| **i >= base_start).count() as i32 * 3;
                            if open.contains(&f) {
                                score += 6;
                            }
                            let mut it = item(name, rel, String::new(), PalAction::Open(f.clone()));
                            it.hl = hl.into_iter().filter(|i| *i >= base_start).map(|i| i - base_start).collect();
                            items.push((score, it));
                        }
                    }
                    if self.ws.root.is_none() {
                        for (i, f) in open.iter().enumerate() {
                            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            if let Some((score, hl)) = fuzzy(&q, &name) {
                                let mut it = item(name, f.to_string_lossy().to_string(), String::new(), PalAction::Open(f.clone()));
                                it.hl = hl;
                                items.push((score - i as i32, it));
                            }
                        }
                    }
                }
            }
            PalMode::Themes { original } => {
                for t in &self.theme_reg.themes {
                    if let Some((score, hl)) = fuzzy(&q, &t.def.name) {
                        let b = &t.def.base;
                        let kw = t.def.syntax.get("keyword").and_then(|o| match o {
                            anvil_config::SyntaxOverride::Color(c) => Some(*c),
                            anvil_config::SyntaxOverride::Style(s) => s.color,
                        });
                        let mut it = item(
                            t.def.name.clone(),
                            if t.builtin { "встроенная".into() } else { format!("themes/{}.toml", t.id) },
                            String::new(),
                            PalAction::Theme(t.id.clone()),
                        );
                        it.hl = hl;
                        it.swatch = vec![c32(b.bg), c32(b.popup), c32(b.accent), c32(kw.unwrap_or(b.text))];
                        it.current = &t.id == original;
                        items.push((score, it));
                    }
                }
            }
            PalMode::Accent { original } => {
                if let Some(c) = Rgba::parse(&q).filter(|_| q.trim().len() >= 4) {
                    let mut it = item(format!("Свой цвет {}", c.to_hex()), String::new(), String::new(), PalAction::Accent(Some(c)));
                    it.swatch = vec![c32(c)];
                    items.push((1000, it));
                }
                let mut it = item("Из темы".into(), "акцент, заданный темой".into(), String::new(), PalAction::Accent(None));
                it.current = original.is_none();
                if let Some((score, hl)) = fuzzy(&q, "Из темы") {
                    it.hl = hl;
                    items.push((score, it));
                }
                for (name, hex) in ACCENTS {
                    let Some(c) = Rgba::parse(hex) else { continue };
                    if let Some((score, hl)) = fuzzy(&q, name) {
                        let mut it = item((*name).to_string(), (*hex).to_string(), String::new(), PalAction::Accent(Some(c)));
                        it.hl = hl;
                        it.swatch = vec![c32(c)];
                        it.current = *original == Some(c);
                        items.push((score, it));
                    }
                }
            }
            PalMode::GotoLine => {
                let total = self.doc().map(|d| d.ed.rope().len_lines()).unwrap_or(0);
                let n: usize = q.trim().trim_start_matches(':').parse().unwrap_or(0);
                if n > 0 {
                    items.push((0, item(format!("Перейти к строке {n}"), format!("из {total}"), String::new(), PalAction::Goto(n))));
                } else {
                    items.push((0, item(format!("Введите номер строки (1–{total})"), String::new(), String::new(), PalAction::None)));
                }
            }
            PalMode::Rename { doc, at, old } => {
                let new = q.trim();
                let label = if new.is_empty() || new == old {
                    format!("Введите новое имя для «{old}»")
                } else {
                    format!("Переименовать «{old}» → «{new}»")
                };
                let action = if new.is_empty() || new == old { PalAction::None } else { PalAction::Rename(*doc, *at) };
                items.push((0, item(label, "во всех файлах проекта".into(), "Enter".into(), action)));
            }
            PalMode::Join => {
                let link = q.trim();
                let (label, detail, action) = match anvil_collab::Invite::parse(link) {
                    Ok(inv) => (
                        format!("Присоединиться к сессии на {}", inv.host),
                        "TLS 1.3 · сертификат сверяется по отпечатку из ссылки".to_string(),
                        PalAction::Collab(CollabAct::Join),
                    ),
                    Err(_) if link.is_empty() => ("Вставьте ссылку-приглашение (Ctrl+V)".into(), "anvil://хост:порт/токен?fp=…".into(), PalAction::None),
                    Err(e) => (format!("Ссылка не подходит: {e}"), String::new(), PalAction::None),
                };
                items.push((0, item(label, detail, "Enter".into(), action)));
            }
            PalMode::Session => {
                if let Some(sess) = &self.collab.session {
                    let live = sess.phase == crate::collab::Phase::Live;
                    let mut list: Vec<PalItem> = Vec::new();
                    if !live {
                        list.push(item("Подключение…".into(), sess.relay_label.clone(), String::new(), PalAction::None));
                    }
                    if sess.invites.is_some() {
                        list.push(item(
                            "Скопировать ссылку для редактирования".into(),
                            "с правом правки".into(),
                            String::new(),
                            PalAction::Collab(CollabAct::CopyEdit),
                        ));
                        list.push(item(
                            "Скопировать ссылку только для чтения".into(),
                            "смотрит без правок".into(),
                            String::new(),
                            PalAction::Collab(CollabAct::CopyRead),
                        ));
                    }
                    let me = crate::collab::my_name(&self.settings.collab);
                    let mut it = item(format!("{me} (вы)"), sess.role.label().to_string(), String::new(), PalAction::None);
                    it.swatch = vec![crate::collab::color32(crate::collab::my_color(&self.settings.collab, &me))];
                    list.push(it);
                    for pv in sess.peers.values() {
                        let where_ = pv.doc.and_then(|d| sess.docs.get(&d)).map(|d| d.path.clone()).unwrap_or_else(|| "не в общем файле".into());
                        let mut it = item(
                            pv.info.name.clone(),
                            format!("{} · {where_}", pv.info.role.label()),
                            "перейти".into(),
                            PalAction::Collab(CollabAct::Peer(pv.info.id)),
                        );
                        it.swatch = vec![crate::collab::color32(pv.info.color)];
                        list.push(it);
                    }
                    if !sess.host {
                        for (id, d) in &sess.docs {
                            list.push(item(d.path.clone(), "файл сессии".into(), "открыть".into(), PalAction::Collab(CollabAct::Open(*id))));
                        }
                    }
                    let leave = if sess.host { "Завершить сессию" } else { "Покинуть сессию" };
                    let detail = if sess.host {
                        format!("{} · участники отключатся", sess.relay_label)
                    } else {
                        "открытые файлы останутся копиями".into()
                    };
                    list.push(item(leave.into(), detail, String::new(), PalAction::Collab(CollabAct::Leave)));
                    for (i, it) in list.into_iter().enumerate() {
                        let hay = format!("{} {}", it.label, it.detail);
                        if let Some((score, hl)) = fuzzy(&q, &hay) {
                            let hl = if q.is_empty() { Vec::new() } else { hl.into_iter().filter(|x| *x < it.label.chars().count()).collect() };
                            items.push((score * 1000 - i as i32, PalItem { hl, ..it }));
                        }
                    }
                }
            }
            PalMode::Locations { .. } => {
                let fixed = self.palette.as_ref().map(|p| p.fixed.clone()).unwrap_or_default();
                for (i, it) in fixed.into_iter().enumerate() {
                    let hay = format!("{} {}", it.label, it.detail);
                    if let Some((score, _)) = fuzzy(&q, &hay) {
                        let hl = fuzzy(&q, &it.label).map(|x| x.1).unwrap_or_default();
                        items.push((score * 1000 - i as i32, PalItem { hl, ..it }));
                    }
                }
            }
            PalMode::Pick(Pick::ImportVscodeTheme) => {
                let themes = vscode_settings::find_themes();
                if themes.is_empty() {
                    items.push((
                        0,
                        item("Темы VS Code не найдены".into(), "можно выбрать .json вручную".into(), String::new(), PalAction::Cmd(Cmd::ThemeImportFile)),
                    ));
                }
                for (label, path) in themes {
                    if let Some((score, hl)) = fuzzy(&q, &label) {
                        let ext = path.ancestors().nth(2).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let mut it = item(label, ext, String::new(), PalAction::ImportTheme(path));
                        it.hl = hl;
                        items.push((score, it));
                    }
                }
            }
            PalMode::Pick(p) => {
                let (path, _, opts) = pick_options(*p);
                let cur = self.current_pick_value(*p);
                for (val, label) in opts {
                    if let Some((score, hl)) = fuzzy(&q, label) {
                        let v = if *p == Pick::AnimSpeed { Value::from(val.parse::<f64>().unwrap_or(1.0)) } else { Value::from(*val) };
                        let mut it = item((*label).to_string(), (*val).to_string(), String::new(), PalAction::Set(path.to_vec(), v));
                        it.hl = hl;
                        it.current = cur == *val || (*p == Pick::AnimSpeed && cur.parse::<f64>().ok() == val.parse::<f64>().ok());
                        items.push((score, it));
                    }
                }
            }
        }
        let keep_order = (matches!(mode, PalMode::Themes { .. } | PalMode::Accent { .. } | PalMode::Pick(_)) && q.is_empty())
            || matches!(mode, PalMode::Rename { .. } | PalMode::Join | PalMode::Session);
        if !keep_order {
            items.sort_by_key(|x| std::cmp::Reverse(x.0));
        }
        items.truncate(300);
        let current_idx = items.iter().position(|(_, it)| it.current);
        if let Some(p) = self.palette.as_mut() {
            p.items = items.into_iter().map(|(_, it)| it).collect();
            if q.is_empty() {
                p.sel = current_idx.unwrap_or(0);
            } else {
                p.sel = 0;
            }
            p.sel = p.sel.min(p.items.len().saturating_sub(1));
        }
        self.preview_palette();
    }

    /// Живой предпросмотр темы/акцента при перемещении по списку.
    fn preview_palette(&mut self) {
        let Some(p) = self.palette.as_mut() else { return };
        if p.previewed == Some(p.sel) {
            return;
        }
        p.previewed = Some(p.sel);
        let action = p.items.get(p.sel).map(|i| i.action.clone());
        let mut s = self.settings.clone();
        match action {
            Some(PalAction::Theme(id)) => s.theme.name = id,
            Some(PalAction::Accent(a)) => s.theme.accent = a,
            _ => return,
        }
        let (rt, _) = self.theme_reg.resolve(&s);
        let spec = self.spec(&self.settings.animations.theme.clone());
        self.theme.set(rt, self.now, spec);
    }

    pub(crate) fn close_palette(&mut self, committed: bool) {
        let Some(p) = self.palette.as_mut() else { return };
        if p.closing.is_some() {
            return;
        }
        p.closing = Some(self.now);
        let previewing = matches!(p.mode, PalMode::Themes { .. } | PalMode::Accent { .. });
        if self.focus == Focus::Palette {
            self.focus = Focus::Editor;
        }
        if previewing && !committed {
            // вернуть исходную тему
            let (rt, _) = self.theme_reg.resolve(&self.settings);
            let spec = self.spec(&self.settings.animations.theme.clone());
            self.theme.set(rt, self.now, spec);
        }
    }

    pub(crate) fn run_pal_action(&mut self, a: PalAction) {
        self.close_palette(!matches!(a, PalAction::None));
        match a {
            PalAction::Cmd(c) => self.exec(c),
            PalAction::Open(p) => {
                self.open_path(&p);
            }
            PalAction::Theme(id) => self.set_setting(&["theme", "name"], Some(Value::from(id.as_str()))),
            PalAction::Accent(a) => self.set_setting(&["theme", "accent"], a.map(|c| Value::from(c.to_hex().as_str()))),
            PalAction::Set(path, v) => self.set_setting(&path, Some(v)),
            PalAction::Goto(n) => {
                if let Some(d) = self.doc_mut() {
                    let rope = d.ed.rope();
                    let l = n.saturating_sub(1).min(rope.len_lines().saturating_sub(1));
                    let pos = rope.line_to_char(l) + anvil_core::text::leading_ws(rope, l).chars().count();
                    d.ed.set_cursor(pos);
                    d.view.reveal = true;
                    d.view.center = true;
                }
            }
            PalAction::ImportTheme(p) => self.import_theme_file(&p),
            PalAction::Location(p, pos, enc) => self.goto_location(&p, pos, enc),
            PalAction::Rename(doc, at) => {
                let name = self.palette.as_ref().map(|p| p.input.text.trim().to_string()).unwrap_or_default();
                self.lsp_rename(doc, at, &name);
            }
            PalAction::Collab(act) => match act {
                CollabAct::Join => {
                    let link = self.palette.as_ref().map(|p| p.input.text.trim().to_string()).unwrap_or_default();
                    self.collab_join(&link);
                }
                CollabAct::CopyEdit => self.collab_copy_invite(false),
                CollabAct::CopyRead => self.collab_copy_invite(true),
                CollabAct::Peer(id) => self.collab_goto_peer(id),
                CollabAct::Open(id) => self.collab_open_doc(id),
                CollabAct::Leave => self.collab_end(None),
            },
            PalAction::None => {}
        }
    }

    pub(crate) fn draw_palette(&mut self, ui: &mut egui::Ui, full: Rect, events: &[Event]) {
        let Some(pal) = self.palette.as_ref() else { return };
        let spec = self.spec(&self.settings.animations.palette.clone());
        let (opened, closing) = (pal.opened, pal.closing);
        let k = match closing {
            Some(t) => 1.0 - self.tw.since(t, spec),
            None => self.tw.since(opened, spec),
        };
        if closing.is_some() && k <= 0.001 {
            self.palette = None;
            return;
        }
        let c = self.colors.clone();
        let radius = self.settings.theme.radius;

        // ввод
        if closing.is_none() && self.focus == Focus::Palette {
            let now = self.now;
            let out = self.palette.as_mut().map(|p| p.input.handle(events, now)).unwrap_or_default();
            if let Some(t) = out.copy {
                ui.ctx().copy_text(t);
            }
            if out.changed {
                if let Some(p) = self.palette.as_mut() {
                    p.previewed = None;
                }
                self.refresh_palette();
            }
            let n = self.palette.as_ref().map(|p| p.items.len()).unwrap_or(0);
            if let Some(p) = self.palette.as_mut()
                && n > 0
            {
                if out.up {
                    p.sel = (p.sel + n - 1) % n;
                }
                if out.down {
                    p.sel = (p.sel + 1) % n;
                }
                if out.page_up {
                    p.sel = p.sel.saturating_sub(10);
                }
                if out.page_down {
                    p.sel = (p.sel + 10).min(n - 1);
                }
            }
            if out.up || out.down || out.page_up || out.page_down {
                self.preview_palette();
            }
            if out.escape {
                self.close_palette(false);
            } else if out.enter {
                let a = self.palette.as_ref().and_then(|p| p.items.get(p.sel).map(|i| i.action.clone()));
                if let Some(a) = a {
                    self.run_pal_action(a);
                }
            }
        }

        let Some(pal) = self.palette.as_ref() else { return };
        let w = 620.0f32.min(full.width() - 40.0);
        let row_h = 36.0;
        let max_rows = 10usize;
        let n = pal.items.len();
        let rows = n.min(max_rows);
        let list_h_target = rows.max(1) as f32 * row_h + 12.0;
        let list_h = self.tw.value("pal-list-h", list_h_target, spec);
        let h = 48.0 + list_h + 30.0;
        let top = full.top() + self.settings.ui.titlebar_height * self.settings.ui.density.factor() + 10.0;
        let mut r = Rect::from_min_size(pos2(full.center().x - w / 2.0, top), vec2(w, h));
        match spec.style {
            Motion::Scale => r = Rect::from_center_size(r.center(), r.size() * (0.96 + 0.04 * k)),
            Motion::Slide => r = r.translate(vec2(0.0, (1.0 - k) * -14.0)),
            _ => {}
        }

        let p = ui.painter().clone();
        p.rect_filled(full, egui::CornerRadius::ZERO, with_alpha(c.scrim, k));
        // клик мимо — закрыть
        let outside = ui.interact(full, Id::unique("pal-scrim"), Sense::click());
        let close_outside = outside.clicked() && !ui.ctx().input(|i| i.pointer.interact_pos().is_some_and(|pp| r.contains(pp)));
        shadow(&p, r, radius, with_alpha(c.shadow, k), 36);
        p.rect_filled(r, cr(radius), with_alpha(c.popup, k));
        p.rect_stroke(r, cr(radius), Stroke::new(1.0, with_alpha(c.border_strong, k)), StrokeKind::Inside);
        let pp = p.with_clip_rect(r);

        // строка ввода
        let input_r = Rect::from_min_size(r.min, vec2(r.width(), 48.0));
        pp.line_segment([input_r.left_bottom(), input_r.right_bottom()], Stroke::new(1.0, with_alpha(c.border, k)));
        let (ic, chip, placeholder) = match &pal.mode {
            PalMode::Commands => (Icon::Search, Some("Команды"), "Введите команду…"),
            PalMode::Files => (Icon::Files, None, "Имя файла (> команды, : строка)"),
            PalMode::Themes { .. } => (Icon::Search, Some("Тема"), "Выберите тему — предпросмотр сразу"),
            PalMode::Accent { .. } => (Icon::Search, Some("Акцент"), "Цвет или #hex"),
            PalMode::GotoLine => (Icon::Search, Some("Строка"), "Номер строки"),
            PalMode::Pick(pk) => (Icon::Search, Some(pick_options(*pk).1), "Выберите вариант"),
            PalMode::Rename { .. } => (Icon::Search, Some("Переименовать"), "Новое имя"),
            PalMode::Locations { title } => (Icon::Search, Some(title.as_str()), "Фильтр по файлу или тексту"),
            PalMode::Join => (Icon::Users, Some("Войти"), "anvil://хост:порт/токен?fp=…"),
            PalMode::Session => (Icon::Users, Some("Сессия"), "Участники, приглашения, файлы"),
        };
        icon(&pp, Rect::from_min_size(pos2(input_r.left() + 16.0, input_r.center().y - 8.0), vec2(16.0, 16.0)), ic, with_alpha(c.text_faint, k), 1.4);
        let mut tx = input_r.left() + 42.0;
        if let Some(chip) = chip {
            let f = self.ui_font(11.0);
            let cw = text_width(ui.ctx(), chip, &f) + 14.0;
            let cr_ = Rect::from_min_size(pos2(tx, input_r.center().y - 11.0), vec2(cw, 22.0));
            pp.rect_filled(cr_, cr(5.0), with_alpha(c.accent, 0.14 * k));
            text(&pp, cr_.center(), Align2::CENTER_CENTER, chip, f, with_alpha(c.accent, k));
            tx = cr_.right() + 10.0;
        }
        let caret_a = if (self.now - pal.input.edited_at) < 0.5 { 1.0 } else { 0.5 + 0.5 * ((self.now - pal.input.edited_at) as f32 * 5.5).cos().max(-0.2) };
        if closing.is_none() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        }
        pal.input.paint(
            ui.ctx(),
            &pp,
            Rect::from_min_max(pos2(tx, input_r.top()), pos2(input_r.right() - 16.0, input_r.bottom())),
            &self.ui_font(14.0),
            &c,
            true,
            placeholder,
            caret_a * k,
        );

        // список
        let list = Rect::from_min_max(pos2(r.left(), input_r.bottom()), pos2(r.right(), r.bottom() - 30.0));
        let lp = pp.with_clip_rect(list);
        let mut first = pal.first;
        if pal.sel < first {
            first = pal.sel;
        }
        if pal.sel >= first + max_rows {
            first = pal.sel + 1 - max_rows;
        }
        let sel = pal.sel;
        let sel_y = self.tw.value("pal-sel-y", (sel as f32 - first as f32) * row_h, self.spec(&self.settings.animations.hover.clone()));
        let sel_rect = Rect::from_min_size(pos2(list.left() + 6.0, list.top() + 6.0 + sel_y), vec2(list.width() - 12.0, row_h));
        if n > 0 {
            lp.rect_filled(sel_rect, cr(radius * 0.7), with_alpha(c.accent, 0.13 * k));
            lp.rect_filled(Rect::from_min_size(pos2(sel_rect.left(), sel_rect.top() + 9.0), vec2(2.0, row_h - 18.0)), cr(1.0), with_alpha(c.accent, k));
        }
        let font = self.ui_font(13.0);
        let small = self.ui_font(11.0);
        let items: Vec<PalItem> = pal.items.iter().skip(first).take(max_rows).cloned().collect();
        let mut clicked = None;
        let mut hovered = None;
        for (i, it) in items.iter().enumerate() {
            let idx = first + i;
            let rr = Rect::from_min_size(pos2(list.left() + 6.0, list.top() + 6.0 + i as f32 * row_h), vec2(list.width() - 12.0, row_h));
            let resp = ui.interact(rr.intersect(list), Id::unique(("pal-item", idx)), Sense::click());
            if resp.hovered() && ui.ctx().input(|i| i.pointer.delta().length() > 0.0) {
                hovered = Some(idx);
            }
            if resp.clicked() {
                clicked = Some(idx);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            let cy = rr.center().y;
            let mut x = rr.left() + 12.0;
            if !it.swatch.is_empty() {
                let sw = Rect::from_min_size(pos2(x, cy - 8.0), vec2(14.0 * it.swatch.len() as f32, 16.0));
                for (j, col) in it.swatch.iter().enumerate() {
                    lp.rect_filled(
                        Rect::from_min_size(pos2(sw.left() + j as f32 * 14.0, sw.top()), vec2(14.0, 16.0)),
                        egui::CornerRadius::ZERO,
                        with_alpha(*col, k),
                    );
                }
                lp.rect_stroke(sw, cr(3.0), Stroke::new(1.0, with_alpha(c.border_strong, k)), StrokeKind::Outside);
                x = sw.right() + 12.0;
            }
            let is_sel = idx == sel;
            let col = if is_sel { c.text } else { c.text_muted };
            let wl = paint_highlighted(ui.ctx(), &lp, pos2(x, cy), &it.label, &it.hl, &font, with_alpha(col, k), with_alpha(c.accent, k));
            if !it.detail.is_empty() {
                let maxw = rr.right() - x - wl - 160.0;
                let mut d = it.detail.clone();
                while text_width(ui.ctx(), &d, &small) > maxw && d.chars().count() > 6 {
                    d = format!("…{}", d.chars().skip(4).collect::<String>().trim_start_matches('…'));
                }
                text(&lp, pos2(x + wl + 10.0, cy), Align2::LEFT_CENTER, &d, small.clone(), with_alpha(c.text_faint, k));
            }
            if it.current {
                text(&lp, pos2(rr.right() - 12.0, cy), Align2::RIGHT_CENTER, "текущая", small.clone(), with_alpha(c.text_faint, k));
            } else if !it.hint.is_empty() {
                self.kbd(&lp, pos2(rr.right() - 10.0, cy), &it.hint, true);
            }
        }
        if n == 0 {
            text(&lp, pos2(list.left() + 20.0, list.top() + 24.0), Align2::LEFT_CENTER, "Ничего не найдено", font.clone(), with_alpha(c.text_faint, k));
        }

        // подвал
        let foot = Rect::from_min_max(pos2(r.left(), r.bottom() - 30.0), r.max);
        pp.line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, with_alpha(c.border, k)));
        let hint = match pal.mode {
            PalMode::Themes { .. } | PalMode::Accent { .. } => "↑↓ предпросмотр  ·  Enter применить  ·  Esc отменить",
            _ => "↑↓ выбрать  ·  Enter выполнить  ·  Esc закрыть",
        };
        text(&pp, pos2(foot.left() + 16.0, foot.center().y), Align2::LEFT_CENTER, hint, small.clone(), with_alpha(c.text_faint, k));
        text(&pp, pos2(foot.right() - 16.0, foot.center().y), Align2::RIGHT_CENTER, &format!("{n}"), small, with_alpha(c.text_faint, k));

        if let Some(p) = self.palette.as_mut() {
            p.first = first;
            if let Some(h) = hovered {
                p.sel = h;
            }
        }
        if hovered.is_some() {
            self.preview_palette();
        }
        if close_outside {
            self.close_palette(false);
        }
        if let Some(i) = clicked {
            let a = self.palette.as_ref().and_then(|p| p.items.get(i).map(|x| x.action.clone()));
            if let Some(a) = a {
                self.run_pal_action(a);
            }
        }
    }

    // ------------------------------------------------------------------ поиск

    pub(crate) fn draw_find(&mut self, ui: &mut egui::Ui, area: Rect, events: &[Event]) {
        let Some(idx) = self.active else { return };
        if !self.find.open {
            if let Some(d) = self.docs.get_mut(idx)
                && !d.view.find_matches.is_empty()
            {
                d.view.find_matches.clear();
                d.view.find_key.clear();
            }
            return;
        }
        let c = self.colors.clone();
        let now = self.now;
        // ввод
        let mut jump = false;
        if matches!(self.focus, Focus::Find | Focus::Replace) {
            let replace_focus = self.focus == Focus::Replace;
            let out = if replace_focus { self.find.replacement.handle(events, now) } else { self.find.query.handle(events, now) };
            if let Some(t) = out.copy {
                ui.ctx().copy_text(t);
            }
            if out.changed && !replace_focus {
                jump = true;
            }
            if out.escape {
                self.find.open = false;
                self.focus = Focus::Editor;
                return;
            }
            if out.tab && self.find.replace {
                self.focus = if replace_focus { Focus::Find } else { Focus::Replace };
            }
            if out.enter {
                if replace_focus {
                    self.replace_one();
                } else {
                    self.find_step(true);
                }
            }
            if out.shift_enter {
                self.find_step(false);
            }
        }
        // пересчитать совпадения
        let key = {
            let d = &self.docs[idx];
            format!("{}:{}:{:?}:{}", d.id, d.ed.buffer.version, self.find.opts, self.find.query.text)
        };
        if self.docs[idx].view.find_key != key {
            let res = search::find_all(self.docs[idx].ed.rope(), &self.find.query.text, self.find.opts, 100_000);
            let d = &mut self.docs[idx];
            d.view.find_key = key;
            match res {
                Ok(m) => {
                    d.view.find_matches = m;
                    self.find.error = None;
                }
                Err(e) => {
                    d.view.find_matches.clear();
                    self.find.error = Some(e.to_string().lines().last().unwrap_or("ошибка regex").to_string());
                }
            }
            if jump {
                let head = d.ed.sel.primary().start();
                if let Some((i, &(a, b))) =
                    d.view.find_matches.iter().enumerate().find(|(_, r)| r.0 >= head).or_else(|| d.view.find_matches.iter().enumerate().next())
                {
                    d.ed.sel = anvil_core::Selections::from_one(anvil_core::Selection::new(a, b));
                    d.view.reveal = true;
                    d.view.center = true;
                    self.find.current = i;
                }
            }
        }
        let count = self.docs[idx].view.find_matches.len();
        let prim = self.docs[idx].ed.sel.primary();
        let current = self.docs[idx].view.find_matches.iter().position(|r| r.0 == prim.start() && r.1 == prim.end());

        // отрисовка
        let dens = self.settings.ui.density.factor();
        let top = area.top() + (self.settings.ui.tab_height * dens).round() + if self.settings.ui.breadcrumbs { (24.0 * dens).round() } else { 0.0 } + 6.0;
        let w = 440.0f32.min(area.width() - 30.0);
        let h = if self.find.replace { 76.0 } else { 40.0 };
        let spec = self.spec(&self.settings.animations.panels.clone());
        let k = self.tw.since(self.find.opened, spec);
        let r = Rect::from_min_size(
            pos2(area.right() - w - 18.0 - self.settings.editor.minimap.width * f32::from(self.settings.editor.minimap.enabled as u8), top + (1.0 - k) * -12.0),
            vec2(w, h),
        );
        let radius = self.settings.theme.radius;
        let p = ui.painter().clone();
        shadow(&p, r, radius, with_alpha(c.shadow, k * 0.9), 24);
        p.rect_filled(r, cr(radius), with_alpha(c.popup, k));
        p.rect_stroke(r, cr(radius), Stroke::new(1.0, with_alpha(c.border_strong, k)), StrokeKind::Inside);
        let _block = ui.interact(r, Id::unique("find-bg"), Sense::click());
        let font = self.ui_font(12.5);
        let row1 = Rect::from_min_size(r.min, vec2(w, 40.0));
        let field = Rect::from_min_max(pos2(row1.left() + 8.0, row1.top() + 6.0), pos2(row1.right() - 170.0, row1.bottom() - 6.0));
        let ffocus = self.focus == Focus::Find;
        p.rect_filled(field, cr(radius * 0.6), with_alpha(c.bg, k));
        p.rect_stroke(field, cr(radius * 0.6), Stroke::new(1.0, if ffocus { with_alpha(c.accent, 0.7) } else { c.border }), StrokeKind::Inside);
        if ui.interact(field, Id::unique("find-field"), Sense::click()).clicked() {
            self.focus = Focus::Find;
        }
        let caret = 0.6 + 0.4 * ((now - self.find.query.edited_at) as f32 * 5.0).cos();
        if ffocus {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
        }
        self.find.query.paint(ui.ctx(), &p, field.shrink2(vec2(8.0, 0.0)), &font, &c, ffocus, "Найти", caret);
        // переключатели
        let mut x = field.right() + 6.0;
        let toggles =
            [("Aa", "find-case", self.find.opts.case_sensitive), ("ab", "find-word", self.find.opts.whole_word), (".*", "find-re", self.find.opts.regex)];
        let mut flip = None;
        for (i, (label, key, on)) in toggles.iter().enumerate() {
            let b = Rect::from_min_size(pos2(x, row1.top() + 8.0), vec2(26.0, 24.0));
            let resp = ui.interact(b, Id::unique(*key), Sense::click());
            let ht = self.hover_t(*key, resp.hovered());
            if *on {
                p.rect_filled(b, cr(5.0), with_alpha(c.accent, 0.2));
                p.rect_stroke(b, cr(5.0), Stroke::new(1.0, with_alpha(c.accent, 0.6)), StrokeKind::Inside);
            } else if ht > 0.01 {
                p.rect_filled(b, cr(5.0), with_alpha(c.hover, ht * 1.6));
            }
            text(&p, b.center(), Align2::CENTER_CENTER, label, self.bold_font(11.0), if *on { c.accent } else { c.text_muted });
            if resp.clicked() {
                flip = Some(i);
            }
            x += 28.0;
        }
        if let Some(i) = flip {
            match i {
                0 => self.find.opts.case_sensitive = !self.find.opts.case_sensitive,
                1 => self.find.opts.whole_word = !self.find.opts.whole_word,
                _ => self.find.opts.regex = !self.find.opts.regex,
            }
        }
        let label = match (&self.find.error, count, current) {
            (Some(_), _, _) => "ошибка".to_string(),
            (None, 0, _) if !self.find.query.text.is_empty() => "нет".to_string(),
            (None, 0, _) => String::new(),
            (None, n, Some(i)) => format!("{}/{n}", i + 1),
            (None, n, None) => format!("{n}"),
        };
        text(&p, pos2(x + 4.0, row1.center().y), Align2::LEFT_CENTER, &label, self.ui_font(11.0), if self.find.error.is_some() { c.err } else { c.text_faint });
        let mut bx = row1.right() - 30.0;
        let mut act = None;
        for (key, ic, a) in [("find-close", Icon::Close, 0), ("find-next", Icon::Down, 1), ("find-prev", Icon::Up, 2)] {
            let b = Rect::from_min_size(pos2(bx, row1.top() + 8.0), vec2(24.0, 24.0));
            let ht = self.hover_t(key, ui.rect_contains_pointer(b));
            if super::icon_button(ui, b, Id::unique(key), ic, &c, ht, false).clicked() {
                act = Some(a);
            }
            bx -= 26.0;
        }
        match act {
            Some(0) => {
                self.find.open = false;
                self.focus = Focus::Editor;
            }
            Some(1) => self.find_step(true),
            Some(2) => self.find_step(false),
            _ => {}
        }
        if self.find.replace {
            let row2 = Rect::from_min_size(pos2(r.left(), row1.bottom() - 4.0), vec2(w, 36.0));
            let field2 = Rect::from_min_max(pos2(row2.left() + 8.0, row2.top() + 2.0), pos2(row2.right() - 170.0, row2.bottom() - 6.0));
            let rfocus = self.focus == Focus::Replace;
            p.rect_filled(field2, cr(radius * 0.6), with_alpha(c.bg, k));
            p.rect_stroke(field2, cr(radius * 0.6), Stroke::new(1.0, if rfocus { with_alpha(c.accent, 0.7) } else { c.border }), StrokeKind::Inside);
            if ui.interact(field2, Id::unique("replace-field"), Sense::click()).clicked() {
                self.focus = Focus::Replace;
            }
            let caret = 0.6 + 0.4 * ((now - self.find.replacement.edited_at) as f32 * 5.0).cos();
            self.find.replacement.paint(ui.ctx(), &p, field2.shrink2(vec2(8.0, 0.0)), &font, &c, rfocus, "Заменить на", caret);
            let mut x = field2.right() + 8.0;
            for (label, key, all) in [("Заменить", "rep-one", false), ("Все", "rep-all", true)] {
                let tw = text_width(ui.ctx(), label, &self.ui_font(11.5)) + 16.0;
                let b = Rect::from_min_size(pos2(x, row2.top() + 3.0), vec2(tw, 24.0));
                let resp = ui.interact(b, Id::unique(key), Sense::click());
                let ht = self.hover_t(key, resp.hovered());
                p.rect_filled(b, cr(5.0), with_alpha(c.hover, 1.0 + ht));
                text(&p, b.center(), Align2::CENTER_CENTER, label, self.ui_font(11.5), mix(c.text_muted, c.text, ht));
                if resp.clicked() {
                    if all {
                        self.replace_all();
                    } else {
                        self.replace_one();
                    }
                }
                x += tw + 6.0;
            }
        }
    }

    fn replace_one(&mut self) {
        let q = self.find.query.text.clone();
        let rep = self.find.replacement.text.clone();
        let opts = self.find.opts;
        let Some(d) = self.doc_mut() else { return };
        let prim = d.ed.sel.primary();
        if let Ok(list) = search::replacements(d.ed.rope(), &q, &rep, opts)
            && let Some(r) = list.into_iter().find(|r| r.0 == prim.start() && r.1 == prim.end())
        {
            d.ed.replace_ranges(vec![r]);
        }
        self.find_step(true);
    }

    fn replace_all(&mut self) {
        let q = self.find.query.text.clone();
        let rep = self.find.replacement.text.clone();
        let opts = self.find.opts;
        let Some(d) = self.doc_mut() else { return };
        match search::replacements(d.ed.rope(), &q, &rep, opts) {
            Ok(list) => {
                let n = list.len();
                d.ed.replace_ranges(list);
                self.status(format!("Заменено: {n}"));
            }
            Err(e) => self.status(format!("Ошибка: {e}")),
        }
    }
}
