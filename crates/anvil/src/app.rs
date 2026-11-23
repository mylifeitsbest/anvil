//! Состояние приложения, команды, настройки на лету, фоновые задачи.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use anvil_config::{Loaded, Paths, Settings, ThemeRegistry, vscode, vscode_settings};
use anvil_syntax::Highlighter;
use anvil_toolchains::{Found, Override, Plan, Registry, RunEvent, RunHandle};
use eframe::egui::{self, Color32, Event, Rect, emath::TSTransform, pos2, vec2};
use notify::Watcher;
use toml_edit::{DocumentMut, Item, Table, TableLike, Value};

use crate::Args;
use crate::anim::{Spec, Tweens};
use crate::commands::{Cmd, Keymap, Resolve, Stroke};
use crate::doc::{Document, edit_config};
use crate::fonts::{self, FontReport};
use crate::session::Session;
use crate::theme::{Colors, ThemeState};
use crate::ui::overlay::{FindState, PalMode, Palette};
use crate::workspace::Workspace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Editor,
    Palette,
    Find,
    Replace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarView {
    Explorer,
    Tools,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Ok,
    Warn,
    Error,
}

pub struct Toast {
    pub id: u64,
    pub text: String,
    pub kind: ToastKind,
    pub born: f64,
    pub ttl: f32,
}

pub enum ToolMsg {
    Found(String, Option<Found>),
    Version(String, String),
    /// Языковой сервер: путь и «действительно работает».
    Server(String, Option<(PathBuf, bool)>),
    Done,
}

#[derive(Default)]
pub struct ToolsState {
    pub found: HashMap<String, Option<Found>>,
    pub versions: HashMap<String, String>,
    pub scanning: bool,
    pub scanned: bool,
    pub rx: Option<Receiver<ToolMsg>>,
    pub servers: HashMap<String, Option<(PathBuf, bool)>>,
    /// Кнопка «Установить», нажатая один раз (ждём подтверждения).
    pub armed: Option<(String, f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Out,
    Err,
    Info,
    Ok,
    Fail,
}

pub struct PanelLine {
    pub text: String,
    pub kind: LineKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PanelTab {
    #[default]
    Output,
    Problems,
}

#[derive(Default)]
pub struct PanelState {
    pub open: bool,
    pub tab: PanelTab,
    pub lines: Vec<PanelLine>,
    pub title: String,
    pub run: Option<RunHandle>,
    pub exit: Option<(Option<i32>, u128)>,
    pub scroll: f32,
    pub started: f64,
    pub after_install: bool,
}

impl PanelState {
    pub fn push(&mut self, text: impl Into<String>, kind: LineKind) {
        self.lines.push(PanelLine { text: text.into(), kind });
        if self.lines.len() > 8000 {
            self.lines.drain(..1000);
        }
    }
}

pub struct Particle {
    pub pos: egui::Pos2,
    pub vel: egui::Vec2,
    pub born: f64,
    pub life: f32,
    pub color: Color32,
    pub size: f32,
    /// 0 — искра, 1 — кольцо, 2 — вспышка.
    pub kind: u8,
}

pub struct AnvilApp {
    pub(crate) paths: Paths,
    pub(crate) settings: Settings,
    pub(crate) theme_reg: ThemeRegistry,
    pub(crate) theme: ThemeState,
    pub(crate) colors: Colors,
    pub(crate) registry: Registry,
    pub(crate) tools: ToolsState,
    pub(crate) hl: Highlighter,
    pub(crate) ws: Workspace,
    pub(crate) docs: Vec<Document>,
    pub(crate) active: Option<usize>,
    pub(crate) closed: Vec<PathBuf>,
    pub(crate) focus: Focus,
    pub(crate) palette: Option<Palette>,
    pub(crate) find: FindState,
    pub(crate) panel: PanelState,
    pub(crate) toasts: Vec<Toast>,
    pub(crate) tw: Tweens,
    pub(crate) keymap: Keymap,
    pub(crate) chord: Option<(Stroke, f64)>,
    pub(crate) sidebar_view: SidebarView,
    pub(crate) sidebar_hidden: bool,
    pub(crate) zen: bool,
    pub(crate) zoom_steps: i32,
    _watcher: Option<notify::RecommendedWatcher>,
    watch_rx: Receiver<()>,
    fonts_key: String,
    pub(crate) font_report: FontReport,
    pub(crate) session: Session,
    pub(crate) args: Args,
    pub(crate) now: f64,
    pub(crate) dt: f32,
    pub(crate) frame_no: u64,
    pub(crate) launched: f64,
    next_id: u64,
    untitled_n: usize,
    pub(crate) particles: Vec<Particle>,
    pub(crate) shake: (f64, f32),
    pub(crate) tree_scroll: f32,
    pub(crate) tree_disp: f32,
    pub(crate) tools_scroll: f32,
    pub(crate) tools_disp: f32,
    pub(crate) dragging: bool,
    pub(crate) frame_times: Vec<f32>,
    pub(crate) msg: Option<(String, f64)>,
    pub(crate) wake: Arc<dyn Fn() + Send + Sync>,
    pub(crate) close_armed: Option<(u64, f64)>,
    warned: HashSet<String>,
    toast_seq: u64,
    pub(crate) window_focused: bool,
    pub(crate) ctx: egui::Context,
    pub(crate) lsp: crate::lsp::Lsp,
    pub(crate) geom: Option<crate::lsp::EditorGeom>,
    pub(crate) hover_rect: Option<Rect>,
    pub(crate) problems_scroll: f32,
}

fn opener(path: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(path).spawn();
}

/// Ключи настроек, которые хранятся целыми числами.
fn is_int_key(k: &str) -> bool {
    k.ends_with("_ms") || k.ends_with("_s") || matches!(k, "tab_size" | "max_width" | "large_file_mb")
}

pub fn set_toml(doc: &mut DocumentMut, path: &[&str], v: Option<Value>) -> bool {
    let Some((last, parents)) = path.split_last() else { return false };
    let mut cur: &mut dyn TableLike = doc.as_table_mut();
    for seg in parents {
        if cur.get(seg).is_none() {
            let mut t = Table::new();
            t.set_implicit(true);
            cur.insert(seg, Item::Table(t));
        }
        let Some(next) = cur.get_mut(seg).and_then(Item::as_table_like_mut) else { return false };
        cur = next;
    }
    match v {
        None => {
            cur.remove(last);
        }
        Some(mut v) => match cur.get_mut(last) {
            // меняем только значение: комментарии над ключом хранятся в самом ключе
            Some(item) if item.is_value() => {
                if let Some(old) = item.as_value() {
                    *v.decor_mut() = old.decor().clone();
                }
                *item = Item::Value(v);
            }
            _ => {
                cur.insert(last, Item::Value(v));
            }
        },
    }
    true
}

impl AnvilApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Paths, loaded: Loaded, args: Args) -> Self {
        let ctx = cc.egui_ctx.clone();
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        ctx.set_visuals(egui::Visuals::dark());
        let s = loaded.settings.clone();
        let font_report = fonts::install(&ctx, &s);
        ctx.set_zoom_factor(s.ui.scale.clamp(0.5, 3.0));
        let theme_reg = ThemeRegistry::load(Some(&paths.themes));
        let (resolved, theme_warn) = theme_reg.resolve(&s);
        let colors = Colors::new(&resolved, &s);
        let registry = Registry::load(Some(&paths.packs));
        let (keymap, key_warn) = Keymap::build(&s.keymap);
        let wake: Arc<dyn Fn() + Send + Sync> = {
            let c = ctx.clone();
            Arc::new(move || c.request_repaint())
        };

        let (tx, watch_rx) = mpsc::channel();
        let watcher = {
            let c = ctx.clone();
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res {
                    let relevant = ev.paths.iter().any(|p| {
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        name == "settings.toml" || (name.ends_with(".toml") && name != "state.toml")
                    });
                    if relevant {
                        let _ = tx.send(());
                        c.request_repaint();
                    }
                }
            })
            .ok()
            .and_then(|mut w| w.watch(&paths.dir, notify::RecursiveMode::Recursive).ok().map(|_| w))
        };

        let session = Session::load(&paths.state);
        let lsp_reg = anvil_lsp::ServerRegistry::load(Some(&paths.packs));
        let mut app = Self {
            settings: s.clone(),
            theme_reg,
            theme: ThemeState::new(resolved),
            colors,
            registry,
            tools: ToolsState::default(),
            hl: Highlighter::default(),
            ws: Workspace::empty(),
            docs: Vec::new(),
            active: None,
            closed: Vec::new(),
            focus: Focus::Editor,
            palette: None,
            find: FindState::default(),
            panel: PanelState { open: session.panel_open, ..PanelState::default() },
            toasts: Vec::new(),
            tw: Tweens::default(),
            keymap,
            chord: None,
            sidebar_view: SidebarView::Explorer,
            sidebar_hidden: false,
            zen: false,
            zoom_steps: 0,
            _watcher: watcher,
            watch_rx,
            fonts_key: fonts::fonts_key(&s),
            font_report,
            session,
            args,
            now: 0.0,
            dt: 0.016,
            frame_no: 0,
            launched: -1.0,
            next_id: 1,
            untitled_n: 0,
            particles: Vec::new(),
            shake: (-10.0, 0.0),
            tree_scroll: 0.0,
            tree_disp: 0.0,
            tools_scroll: 0.0,
            tools_disp: 0.0,
            dragging: false,
            frame_times: Vec::new(),
            msg: None,
            wake,
            close_armed: None,
            warned: HashSet::new(),
            toast_seq: 0,
            paths,
            window_focused: true,
            ctx: ctx.clone(),
            lsp: crate::lsp::Lsp::default(),
            geom: None,
            hover_rect: None,
            problems_scroll: 0.0,
        };
        app.lsp.reg = lsp_reg;
        for w in loaded.warnings.iter().chain(&theme_warn).chain(&key_warn).chain(&app.theme_reg.warnings.clone()).chain(&app.registry.warnings.clone()) {
            app.toast(w.clone(), ToastKind::Warn);
        }

        // что открыть: аргументы командной строки или прошлая сессия
        let args_paths = app.args.paths.clone();
        if !args_paths.is_empty() {
            for p in args_paths {
                if p.is_dir() {
                    app.open_folder(&p);
                } else {
                    if app.ws.root.is_none()
                        && let Some(parent) = p.parent().filter(|d| d.is_dir())
                    {
                        app.open_folder(parent);
                    }
                    app.open_path(&p);
                }
            }
        } else if app.settings.window.restore_session {
            if let Some(root) = app.session.root.clone().filter(|r| r.is_dir()) {
                app.open_folder(&root);
            }
            for f in app.session.open.clone() {
                if f.is_file() {
                    app.open_path(&f);
                }
            }
            if !app.docs.is_empty() {
                app.active = Some(app.session.active.min(app.docs.len() - 1));
            }
        }
        app
    }

    // ------------------------------------------------------------------ сообщения

    pub fn toast(&mut self, text: impl Into<String>, kind: ToastKind) {
        let text = text.into();
        if self.toasts.iter().any(|t| t.text == text) {
            return;
        }
        self.toast_seq += 1;
        let ttl = match kind {
            ToastKind::Error => 9.0,
            ToastKind::Warn => 7.0,
            _ => 3.5,
        };
        self.toasts.push(Toast { id: self.toast_seq, text, kind, born: self.now, ttl });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub fn status(&mut self, text: impl Into<String>) {
        self.msg = Some((text.into(), self.now));
    }

    pub fn spec(&self, a: &anvil_config::Anim) -> Spec {
        Spec::of(&self.settings.animations, a)
    }

    pub fn doc(&self) -> Option<&Document> {
        self.active.and_then(|i| self.docs.get(i))
    }

    pub fn doc_mut(&mut self) -> Option<&mut Document> {
        self.active.and_then(|i| self.docs.get_mut(i))
    }

    // ------------------------------------------------------------------ файлы и папки

    pub fn open_folder(&mut self, path: &Path) {
        if !path.is_dir() {
            self.toast(format!("Папка не найдена: {}", path.display()), ToastKind::Error);
            return;
        }
        self.ws = Workspace::open(path, self.settings.files.exclude.clone(), self.settings.files.show_hidden, self.now, self.wake.clone());
        if let Some(root) = self.ws.root.clone() {
            self.session.root = Some(root.clone());
            self.session.remember_root(&root);
            let project = root.join(".anvil");
            if project.is_dir()
                && let Some(w) = self._watcher.as_mut()
            {
                let _ = w.watch(&project, notify::RecursiveMode::NonRecursive);
            }
        }
        self.tree_scroll = 0.0;
        self.reload_settings();
        self.save_session();
    }

    pub fn open_path(&mut self, path: &Path) -> Option<usize> {
        let canon = path.canonicalize().map(crate::workspace::strip_unc).unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(canon.as_path())) {
            self.activate(i);
            return Some(i);
        }
        match Document::open(&canon, self.next_id, &self.settings, self.now) {
            Ok(d) => {
                self.next_id += 1;
                if d.ed.buffer.lossy {
                    self.toast(format!("{}: не UTF-8, часть символов заменена", d.name), ToastKind::Warn);
                }
                if d.large {
                    self.toast(format!("{}: большой файл — подсветка отключена", d.name), ToastKind::Info);
                }
                self.docs.push(d);
                let i = self.docs.len() - 1;
                self.activate(i);
                self.lsp_attach(i);
                self.ws.reveal(&canon, self.now);
                self.save_session();
                Some(i)
            }
            Err(e) => {
                self.toast(format!("Не удалось открыть {}: {e}", path.display()), ToastKind::Error);
                None
            }
        }
    }

    pub fn activate(&mut self, i: usize) {
        if i < self.docs.len() {
            self.active = Some(i);
            self.focus = Focus::Editor;
            self.docs[i].view.reveal = true;
        }
    }

    pub fn new_file(&mut self) {
        self.untitled_n += 1;
        let d = Document::untitled(self.untitled_n, self.next_id, &self.settings, self.now);
        self.next_id += 1;
        self.docs.push(d);
        self.active = Some(self.docs.len() - 1);
        self.focus = Focus::Editor;
    }

    pub fn save_doc(&mut self, i: usize, ask_path: bool) -> bool {
        let need_path = ask_path || self.docs.get(i).is_some_and(|d| d.path.is_none());
        if need_path {
            let start = self.ws.root.clone();
            let name = self.docs.get(i).map(|d| d.name.clone()).unwrap_or_default();
            let mut dlg = rfd::FileDialog::new().set_file_name(&name);
            if let Some(r) = start {
                dlg = dlg.set_directory(r);
            }
            let Some(p) = dlg.save_file() else { return false };
            let s = self.settings.clone();
            let mut id = None;
            if let Some(d) = self.docs.get_mut(i) {
                d.set_path(&p, &s);
                id = Some(d.id);
            }
            if let Some(id) = id {
                self.lsp_detach(id);
                self.lsp_attach(i);
            }
        }
        if self.settings.lsp.format_on_save && self.active == Some(i) && self.lsp_format(true) {
            // сохранение произойдёт после ответа сервера (или через 2,5 с)
            return true;
        }
        self.save_doc_now(i)
    }

    /// Записать файл на диск (без форматирования).
    pub fn save_doc_now(&mut self, i: usize) -> bool {
        let s = self.settings.clone();
        let Some(d) = self.docs.get_mut(i) else { return false };
        let doc_id = d.id;
        match d.save(&s) {
            Ok(()) => {
                let is_settings = d.path.as_deref() == Some(self.paths.settings.as_path())
                    || d.path.as_ref().is_some_and(|p| p.ends_with(".anvil/settings.toml") || p.ends_with(".anvil\\settings.toml"));
                let in_themes = d.path.as_ref().is_some_and(|p| p.starts_with(&self.paths.themes) || p.starts_with(&self.paths.packs));
                let name = d.name.clone();
                self.status(format!("Сохранено: {name}"));
                self.lsp_did_save(doc_id);
                if is_settings || in_themes {
                    self.reload_settings();
                }
                self.save_session();
                true
            }
            Err(e) => {
                self.toast(format!("Не удалось сохранить: {e}"), ToastKind::Error);
                false
            }
        }
    }

    pub fn close_tab(&mut self, i: usize, force: bool) {
        let Some(d) = self.docs.get(i) else { return };
        if d.dirty() && !force {
            let armed = self.close_armed.is_some_and(|(id, t)| id == d.id && self.now - t < 4.0);
            if !armed {
                self.close_armed = Some((d.id, self.now));
                let name = d.name.clone();
                self.toast(format!("«{name}» не сохранён. Закройте ещё раз, чтобы выбросить изменения, или Ctrl+S"), ToastKind::Warn);
                return;
            }
        }
        let d = self.docs.remove(i);
        self.lsp_detach(d.id);
        if let Some(p) = d.path {
            self.closed.push(p);
        }
        self.close_armed = None;
        self.active = match self.active {
            _ if self.docs.is_empty() => None,
            Some(a) if a > i => Some(a - 1),
            Some(a) if a == i => Some(i.min(self.docs.len() - 1)),
            other => other,
        };
        self.save_session();
    }

    pub fn save_session(&mut self) {
        self.session.open = self.docs.iter().filter_map(|d| d.path.clone()).collect();
        self.session.active = self.active.unwrap_or(0);
        self.session.panel_open = self.panel.open;
        self.session.root = self.ws.root.clone();
        self.session.save(&self.paths.state);
    }

    // ------------------------------------------------------------------ настройки

    pub fn reload_settings(&mut self) {
        let project = self.ws.root.as_ref().map(|r| r.join(".anvil").join("settings.toml"));
        let loaded = anvil_config::load_settings(&self.paths.settings, project.as_deref());
        let fresh: Vec<String> = loaded.warnings.iter().filter(|w| !self.warned.contains(*w)).cloned().collect();
        for w in fresh {
            self.warned.insert(w.clone());
            self.toast(w, if loaded.failed { ToastKind::Error } else { ToastKind::Warn });
        }
        if loaded.failed {
            self.toast("settings.toml не применён — исправьте ошибку и сохраните ещё раз", ToastKind::Error);
            return;
        }
        self.apply_settings(loaded.settings);
    }

    pub fn apply_settings(&mut self, s: Settings) {
        let ctx = self.ctx();
        if fonts::fonts_key(&s) != self.fonts_key {
            self.fonts_key = fonts::fonts_key(&s);
            self.font_report = fonts::install(&ctx, &s);
        }
        ctx.set_zoom_factor((s.ui.scale.clamp(0.5, 3.0) * 1.1f32.powi(self.zoom_steps)).clamp(0.4, 4.0));
        self.theme_reg = ThemeRegistry::load(Some(&self.paths.themes));
        let (rt, w) = self.theme_reg.resolve(&s);
        for x in w.into_iter().chain(self.theme_reg.warnings.clone()) {
            if self.warned.insert(x.clone()) {
                self.toast(x, ToastKind::Warn);
            }
        }
        self.theme.set(rt, self.now, Spec::of(&s.animations, &s.animations.theme));
        let (km, kw) = Keymap::build(&s.keymap);
        self.keymap = km;
        for x in kw {
            if self.warned.insert(x.clone()) {
                self.toast(x, ToastKind::Warn);
            }
        }
        self.registry = Registry::load(Some(&self.paths.packs));
        let lsp_was = self.settings.lsp.enabled;
        let lsp_servers_changed = s.lsp.servers != self.settings.lsp.servers;
        if (s.window.opacity < 0.999) != (self.settings.window.opacity < 0.999) || s.window.custom_titlebar != self.settings.window.custom_titlebar {
            self.toast("Прозрачность окна и рамка применятся после перезапуска", ToastKind::Info);
        }
        for d in &mut self.docs {
            d.ed.cfg = edit_config(d.lang, &s);
            if d.large != (d.ed.buffer.rope.len_bytes() as u64 > s.files.large_file_mb as u64 * 1024 * 1024) {
                d.hl_version = u64::MAX;
            }
        }
        if s.files.exclude != self.settings.files.exclude || s.files.show_hidden != self.settings.files.show_hidden {
            self.ws.exclude = s.files.exclude.clone();
            self.ws.show_hidden = s.files.show_hidden;
            self.ws.refresh();
            self.ws.reindex(self.wake.clone());
        }
        self.settings = s;
        if !self.settings.lsp.enabled && lsp_was {
            self.lsp_shutdown_all();
        } else if self.settings.lsp.enabled && (!lsp_was || lsp_servers_changed) {
            self.lsp_restart();
        }
    }

    fn ctx(&self) -> egui::Context {
        self.ctx.clone()
    }

    /// Правка settings.toml с сохранением комментариев.
    pub fn edit_settings(&mut self, f: impl FnOnce(&mut DocumentMut)) {
        let text = std::fs::read_to_string(&self.paths.settings).unwrap_or_else(|_| anvil_config::DEFAULT_SETTINGS_TOML.to_string());
        let mut doc = match text.parse::<DocumentMut>() {
            Ok(d) => d,
            Err(e) => {
                self.toast(format!("settings.toml не разобран: {e}"), ToastKind::Error);
                return;
            }
        };
        f(&mut doc);
        if let Err(e) = std::fs::write(&self.paths.settings, doc.to_string()) {
            self.toast(format!("Не удалось записать настройки: {e}"), ToastKind::Error);
            return;
        }
        // если settings.toml открыт во вкладке без правок — перечитать
        let s = self.settings.clone();
        for d in &mut self.docs {
            if d.path.as_deref() == Some(self.paths.settings.as_path()) {
                d.reload_if_changed(&s);
            }
        }
        self.reload_settings();
    }

    pub fn set_setting(&mut self, path: &[&str], v: Option<Value>) {
        self.edit_settings(|doc| {
            set_toml(doc, path, v);
        });
    }

    pub fn toggle_setting(&mut self, path: &[&str], current: bool) {
        self.set_setting(path, Some(Value::from(!current)));
    }

    fn import_vscode_settings(&mut self) {
        let Some(file) = vscode_settings::settings_candidates().into_iter().find(|p| p.is_file()) else {
            self.toast("settings.json VS Code не найден на этом компьютере", ToastKind::Warn);
            return;
        };
        let imported = match std::fs::read_to_string(&file).map_err(anvil_config::ConfigError::from).and_then(|t| vscode_settings::import_settings(&t)) {
            Ok(v) => v,
            Err(e) => {
                self.toast(format!("Не удалось прочитать {}: {e}", file.display()), ToastKind::Error);
                return;
            }
        };
        if imported.is_empty() {
            self.toast("В settings.json VS Code нет настроек, которые можно перенести", ToastKind::Info);
            return;
        }
        let n = imported.len();
        self.edit_settings(|doc| {
            for i in &imported {
                let last = i.path.last().copied().unwrap_or_default();
                let v = match &i.value {
                    vscode_settings::SettingValue::Str(s) => Value::from(s.as_str()),
                    vscode_settings::SettingValue::Bool(b) => Value::from(*b),
                    vscode_settings::SettingValue::Num(x) if is_int_key(last) => Value::from(x.round() as i64),
                    vscode_settings::SettingValue::Num(x) => Value::from(*x),
                };
                set_toml(doc, &i.path, Some(v));
            }
        });
        self.toast(format!("Перенесено настроек из VS Code: {n}"), ToastKind::Ok);
    }

    pub fn import_theme_file(&mut self, file: &Path) {
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                self.toast(format!("Не удалось прочитать тему: {e}"), ToastKind::Error);
                return;
            }
        };
        let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "imported".into());
        match vscode::import_vscode_theme(&text, &stem) {
            Ok(def) => {
                let slug: String =
                    def.name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect::<String>().trim_matches('-').to_string();
                let slug = if slug.is_empty() { stem } else { slug };
                let _ = std::fs::create_dir_all(&self.paths.themes);
                let out = self.paths.themes.join(format!("{slug}.toml"));
                let body = format!("# Импортировано из VS Code: {}\n{}", file.display(), def.to_toml());
                if let Err(e) = std::fs::write(&out, body) {
                    self.toast(format!("Не удалось сохранить тему: {e}"), ToastKind::Error);
                    return;
                }
                let name = def.name.clone();
                self.set_setting(&["theme", "name"], Some(Value::from(slug.as_str())));
                self.toast(format!("Тема «{name}» импортирована и включена"), ToastKind::Ok);
            }
            Err(e) => self.toast(format!("Тема не распознана: {e}"), ToastKind::Error),
        }
    }

    // ------------------------------------------------------------------ тулчейны и запуск

    pub fn rescan_tools(&mut self) {
        if self.tools.scanning {
            return;
        }
        let packs = self.registry.packs.clone();
        let servers = self.lsp.reg.servers.clone();
        let (tx, rx) = mpsc::channel();
        let wake = self.wake.clone();
        self.tools.scanning = true;
        self.tools.rx = Some(rx);
        std::thread::spawn(move || {
            let mut found = Vec::new();
            for p in &packs {
                let f = anvil_toolchains::detect(p);
                if let Some(ff) = &f {
                    found.push((p.clone(), ff.clone()));
                }
                let _ = tx.send(ToolMsg::Found(p.id.clone(), f));
            }
            wake();
            for s in &servers {
                let r = anvil_lsp::servers::locate(s).map(|exe| {
                    let ok = anvil_lsp::servers::probe(s, &exe);
                    (exe, ok)
                });
                let _ = tx.send(ToolMsg::Server(s.id.clone(), r));
            }
            wake();
            for (p, f) in found {
                if let Some(v) = anvil_toolchains::version(&p, &f) {
                    let _ = tx.send(ToolMsg::Version(p.id.clone(), v));
                    wake();
                }
            }
            let _ = tx.send(ToolMsg::Done);
            wake();
        });
    }

    pub fn start_plan(&mut self, plan: Plan, install: bool) {
        if let Some(r) = &self.panel.run {
            r.kill();
        }
        self.panel.open = true;
        self.panel.tab = PanelTab::Output;
        self.panel.lines.clear();
        self.panel.exit = None;
        self.panel.scroll = 0.0;
        self.panel.started = self.now;
        self.panel.after_install = install;
        self.panel.title = plan.title.clone();
        self.panel.push(format!("▸ {}  ·  {}", plan.title, plan.cwd.display()), LineKind::Info);
        self.panel.run = Some(anvil_toolchains::spawn(plan, self.wake.clone()));
    }

    pub fn run_current(&mut self) {
        let Some(i) = self.active else {
            self.toast("Откройте файл, чтобы запустить его", ToastKind::Info);
            return;
        };
        if self.docs[i].path.is_none() && !self.save_doc(i, false) {
            return;
        }
        if self.settings.toolchains.save_before_run && self.docs[i].dirty() {
            self.save_doc(i, false);
        }
        let d = &self.docs[i];
        let Some(path) = d.path.clone() else { return };
        let overrides: BTreeMap<String, Override> =
            self.settings.toolchains.overrides.iter().map(|(k, v)| (k.clone(), Override { build: v.build.clone(), run: v.run.clone() })).collect();
        match anvil_toolchains::plan_run(&self.registry, d.lang.id, &path, self.ws.root.as_deref(), &overrides) {
            Ok(plan) => self.start_plan(plan, false),
            Err(e) => {
                self.toast(e.to_string(), ToastKind::Warn);
                self.sidebar_view = SidebarView::Tools;
                self.sidebar_hidden = false;
                if !self.tools.scanned {
                    self.rescan_tools();
                }
            }
        }
    }

    pub fn install_pack(&mut self, id: &str) {
        let Some(p) = self.registry.get(id).cloned() else { return };
        match p.install.current() {
            Some(cmd) => {
                let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
                self.start_plan(Plan::shell(&format!("Установка: {}", p.name), cmd, &home), true);
            }
            None => opener(Path::new(&p.homepage)),
        }
    }

    pub fn install_server(&mut self, id: &str) {
        let Some(sv) = self.lsp.reg.get(id).cloned() else { return };
        match sv.install.current() {
            Some(cmd) => {
                let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
                self.start_plan(Plan::shell(&format!("Установка: {}", sv.name), cmd, &home), true);
            }
            None => opener(Path::new(&sv.homepage)),
        }
    }

    pub fn open_url(&self, url: &str) {
        opener(Path::new(url));
    }

    // ------------------------------------------------------------------ фон

    fn poll_background(&mut self) {
        let mut reload = false;
        while self.watch_rx.try_recv().is_ok() {
            reload = true;
        }
        if reload {
            self.reload_settings();
            let s = self.settings.clone();
            for d in &mut self.docs {
                if d.path.as_ref().is_some_and(|p| p.extension().is_some_and(|e| e == "toml")) {
                    d.reload_if_changed(&s);
                }
            }
        }
        if let Some(rx) = &self.tools.rx {
            let mut done = false;
            while let Ok(m) = rx.try_recv() {
                match m {
                    ToolMsg::Found(id, f) => {
                        self.tools.found.insert(id, f);
                    }
                    ToolMsg::Version(id, v) => {
                        self.tools.versions.insert(id, v);
                    }
                    ToolMsg::Server(id, r) => {
                        self.tools.servers.insert(id, r);
                    }
                    ToolMsg::Done => done = true,
                }
            }
            if done {
                self.tools.scanning = false;
                self.tools.scanned = true;
                self.tools.rx = None;
                // после установки сервера — подключить открытые файлы, которые остались без него
                let failed: Vec<String> =
                    self.lsp.servers.iter().filter(|(_, s)| matches!(s.status, crate::lsp::Status::Failed(_))).map(|(k, _)| k.clone()).collect();
                let orphan = self.docs.iter().enumerate().filter(|(_, d)| !self.lsp.docs.contains_key(&d.id)).count();
                if !failed.is_empty() || orphan > 0 {
                    for k in failed {
                        self.lsp.servers.remove(&k);
                        let ids: Vec<u64> = self.lsp.docs.iter().filter(|(_, d)| d.server == k).map(|(id, _)| *id).collect();
                        for id in ids {
                            self.lsp.docs.remove(&id);
                        }
                    }
                    for i in 0..self.docs.len() {
                        self.lsp_attach(i);
                    }
                }
            }
        }
        if let Some(run) = &self.panel.run {
            let mut evs = Vec::new();
            while let Ok(ev) = run.rx.try_recv() {
                evs.push(ev);
            }
            for ev in evs {
                match ev {
                    RunEvent::Step(s) => self.panel.push(format!("$ {s}"), LineKind::Info),
                    RunEvent::Out { text, err } => self.panel.push(text, if err { LineKind::Err } else { LineKind::Out }),
                    RunEvent::Exit { code, millis } => {
                        let ok = code == Some(0);
                        let msg = match code {
                            Some(c) => format!("{} · код {c} · {:.2} с", if ok { "Готово" } else { "Ошибка" }, millis as f64 / 1000.0),
                            None => format!("Остановлено · {:.2} с", millis as f64 / 1000.0),
                        };
                        self.panel.push(msg, if ok { LineKind::Ok } else { LineKind::Fail });
                        self.panel.exit = Some((code, millis));
                        self.panel.run = None;
                        if self.panel.after_install {
                            self.panel.after_install = false;
                            self.tools.scanned = false;
                            self.rescan_tools();
                        }
                        break;
                    }
                    RunEvent::Failed(e) => {
                        self.panel.push(e, LineKind::Fail);
                        self.panel.exit = Some((None, 0));
                        self.panel.run = None;
                        break;
                    }
                }
            }
        }
        // автосохранение по таймеру
        if self.settings.files.auto_save == anvil_config::AutoSave::Delay {
            let delay = self.settings.files.auto_save_delay_ms as f64 / 1000.0;
            let s = self.settings.clone();
            let mut saved = false;
            for d in &mut self.docs {
                if d.path.is_some() && d.dirty() && self.now - d.view.activity > delay {
                    saved |= d.save(&s).is_ok();
                }
            }
            if saved {
                self.save_session();
            }
        }
    }

    fn on_focus_change(&mut self, focused: bool) {
        self.window_focused = focused;
        let s = self.settings.clone();
        if focused {
            let mut reloaded = Vec::new();
            for d in &mut self.docs {
                if d.reload_if_changed(&s) {
                    reloaded.push(d.name.clone());
                }
            }
            self.ws.refresh();
            if !reloaded.is_empty() {
                self.status(format!("Обновлено с диска: {}", reloaded.join(", ")));
            }
        } else if s.files.auto_save == anvil_config::AutoSave::Focus {
            for d in &mut self.docs {
                if d.path.is_some() && d.dirty() {
                    let _ = d.save(&s);
                }
            }
        }
    }

    // ------------------------------------------------------------------ клавиатура и команды

    fn handle_keys(&mut self, events: Vec<Event>) -> Vec<Event> {
        let mut rest = Vec::with_capacity(events.len());
        for ev in events {
            if let Event::WindowFocused(f) = ev {
                self.on_focus_change(f);
            }
            if let Event::Key { key, pressed: true, modifiers, .. } = &ev {
                let st = Stroke::from_event(*key, *modifiers);
                let pending = self.chord.filter(|(_, t)| self.now - t < 2.5).map(|(s, _)| s);
                // в полях ввода Ctrl+A / Ctrl+Z работают как обычно
                let text_combo = self.focus != Focus::Editor
                    && st.ctrl
                    && !st.alt
                    && matches!(
                        key,
                        egui::Key::A | egui::Key::Z | egui::Key::Y | egui::Key::Backspace | egui::Key::Delete | egui::Key::ArrowLeft | egui::Key::ArrowRight
                    );
                if !text_combo {
                    match self.keymap.resolve(pending, st) {
                        Resolve::Cmd(c) => {
                            self.chord = None;
                            self.exec(c);
                            continue;
                        }
                        Resolve::Chord(s) => {
                            self.chord = Some((s, self.now));
                            self.status(format!("{} — ждём вторую клавишу…", s.label()));
                            continue;
                        }
                        Resolve::Cancel => {
                            self.chord = None;
                            self.status("Нет такого сочетания");
                            continue;
                        }
                        Resolve::None => {}
                    }
                }
            }
            rest.push(ev);
        }
        rest
    }

    pub fn open_palette(&mut self, mode: PalMode) {
        if let Some(p) = &self.palette
            && p.mode_kind() == mode.kind()
            && p.closing.is_none()
        {
            self.close_palette(false);
            return;
        }
        let mut pal = Palette::new(mode, self.now);
        if let PalMode::Themes { .. } = pal.mode
            && let Some(i) = self.theme_reg.themes.iter().position(|t| t.id == self.settings.theme.name)
        {
            pal.sel = i;
        }
        self.palette = Some(pal);
        self.focus = Focus::Palette;
        self.refresh_palette();
    }

    pub fn exec(&mut self, c: Cmd) {
        let ed_cmd = |app: &mut Self, f: &dyn Fn(&mut anvil_core::Editor)| {
            if let Some(d) = app.doc_mut() {
                f(&mut d.ed);
                d.view.reveal = true;
            }
        };
        match c {
            Cmd::PaletteCommands => self.open_palette(PalMode::Commands),
            Cmd::PaletteFiles => self.open_palette(PalMode::Files),
            Cmd::PaletteThemes => {
                let original = self.settings.theme.name.clone();
                self.open_palette(PalMode::Themes { original });
            }
            Cmd::PaletteAccent => {
                let original = self.settings.theme.accent;
                self.open_palette(PalMode::Accent { original });
            }
            Cmd::PaletteGotoLine => self.open_palette(PalMode::GotoLine),
            Cmd::PalettePick(p) => self.open_palette(PalMode::Pick(p)),
            Cmd::FileNew => self.new_file(),
            Cmd::FileOpen => {
                let mut dlg = rfd::FileDialog::new();
                if let Some(r) = &self.ws.root {
                    dlg = dlg.set_directory(r);
                }
                if let Some(files) = dlg.pick_files() {
                    for f in files {
                        self.open_path(&f);
                    }
                }
            }
            Cmd::FileOpenFolder => {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder(&dir);
                }
            }
            Cmd::FileSave => {
                if let Some(i) = self.active {
                    self.save_doc(i, false);
                }
            }
            Cmd::FileSaveAs => {
                if let Some(i) = self.active {
                    self.save_doc(i, true);
                }
            }
            Cmd::FileSaveAll => {
                for i in 0..self.docs.len() {
                    if self.docs[i].dirty() && self.docs[i].path.is_some() {
                        self.save_doc(i, false);
                    }
                }
            }
            Cmd::FileClose => {
                if let Some(i) = self.active {
                    self.close_tab(i, false);
                }
            }
            Cmd::FileCloseAll => {
                let mut i = self.docs.len();
                while i > 0 {
                    i -= 1;
                    if !self.docs[i].dirty() {
                        self.close_tab(i, true);
                    }
                }
                if !self.docs.is_empty() {
                    self.toast("Несохранённые вкладки оставлены открытыми", ToastKind::Info);
                }
            }
            Cmd::FileReopen => {
                if let Some(p) = self.closed.pop() {
                    self.open_path(&p);
                }
            }
            Cmd::FileCloseFolder => {
                self.ws = Workspace::empty();
                self.reload_settings();
                self.save_session();
            }
            Cmd::EditUndo => ed_cmd(self, &|e| {
                e.undo();
            }),
            Cmd::EditRedo => ed_cmd(self, &|e| {
                e.redo();
            }),
            Cmd::EditSelectAll => ed_cmd(self, &|e| e.select_all()),
            Cmd::EditSelectLine => ed_cmd(self, &|e| e.select_line()),
            Cmd::EditSelectNext => ed_cmd(self, &|e| {
                e.select_next_occurrence();
            }),
            Cmd::EditSelectAllOcc => ed_cmd(self, &|e| e.select_all_occurrences()),
            Cmd::EditCursorAbove => ed_cmd(self, &|e| e.add_cursor_vertical(true)),
            Cmd::EditCursorBelow => ed_cmd(self, &|e| e.add_cursor_vertical(false)),
            Cmd::EditMoveLineUp => ed_cmd(self, &|e| e.move_lines(true)),
            Cmd::EditMoveLineDown => ed_cmd(self, &|e| e.move_lines(false)),
            Cmd::EditCopyLineUp => ed_cmd(self, &|e| e.duplicate_lines(false)),
            Cmd::EditCopyLineDown => ed_cmd(self, &|e| e.duplicate_lines(true)),
            Cmd::EditDeleteLine => ed_cmd(self, &|e| e.delete_lines()),
            Cmd::EditToggleComment => ed_cmd(self, &|e| e.toggle_comment()),
            Cmd::EditIndent => ed_cmd(self, &|e| e.indent_lines(false)),
            Cmd::EditOutdent => ed_cmd(self, &|e| e.indent_lines(true)),
            Cmd::EditInsertLineBelow => ed_cmd(self, &|e| {
                e.move_cursors(anvil_core::Move::End, false);
                e.newline();
            }),
            Cmd::FindOpen | Cmd::FindReplace => {
                let sel = self.doc().map(|d| {
                    let s = d.ed.sel.primary();
                    if !s.is_empty() && s.len() < 200 { d.ed.rope().slice(s.start()..s.end()).to_string() } else { String::new() }
                });
                self.find.open = true;
                self.find.replace = c == Cmd::FindReplace;
                self.find.opened = self.now;
                if let Some(s) = sel.filter(|s| !s.contains('\n') && !s.is_empty()) {
                    self.find.query.set(&s);
                } else {
                    let t = self.find.query.text.clone();
                    self.find.query.set(&t);
                    self.find.query.anchor = Some(0);
                }
                self.focus = Focus::Find;
            }
            Cmd::FindNext => self.find_step(true),
            Cmd::FindPrev => self.find_step(false),
            Cmd::ViewSidebar => self.sidebar_hidden = !self.sidebar_hidden,
            Cmd::ViewPanel => {
                self.panel.open = !self.panel.open;
                self.save_session();
            }
            Cmd::ViewZen => self.zen = !self.zen,
            Cmd::ViewZoomIn | Cmd::ViewZoomOut | Cmd::ViewZoomReset => {
                self.zoom_steps = match c {
                    Cmd::ViewZoomIn => (self.zoom_steps + 1).min(8),
                    Cmd::ViewZoomOut => (self.zoom_steps - 1).max(-5),
                    _ => 0,
                };
                let z = (self.settings.ui.scale.clamp(0.5, 3.0) * 1.1f32.powi(self.zoom_steps)).clamp(0.4, 4.0);
                self.ctx().set_zoom_factor(z);
                self.status(format!("Масштаб {:.0}%", z * 100.0));
            }
            Cmd::ViewMinimap => {
                let v = self.settings.editor.minimap.enabled;
                self.toggle_setting(&["editor", "minimap", "enabled"], v);
            }
            Cmd::ViewNextTab | Cmd::ViewPrevTab => {
                if let Some(i) = self.active {
                    let n = self.docs.len();
                    if n > 0 {
                        let j = if c == Cmd::ViewNextTab { (i + 1) % n } else { (i + n - 1) % n };
                        self.activate(j);
                    }
                }
            }
            Cmd::ViewExplorer => {
                self.sidebar_view = SidebarView::Explorer;
                self.sidebar_hidden = false;
            }
            Cmd::ViewToolchains => {
                self.sidebar_view = SidebarView::Tools;
                self.sidebar_hidden = false;
                if !self.tools.scanned {
                    self.rescan_tools();
                }
            }
            Cmd::ViewSidebarSide => {
                let right = self.settings.ui.sidebar == anvil_config::Side::Right;
                self.set_setting(&["ui", "sidebar"], Some(Value::from(if right { "left" } else { "right" })));
            }
            Cmd::ViewFps => {
                let v = self.settings.ui.show_fps;
                self.toggle_setting(&["ui", "show_fps"], v);
            }
            Cmd::RunFile => self.run_current(),
            Cmd::RunStop => {
                if let Some(r) = &self.panel.run {
                    r.kill();
                }
            }
            Cmd::SettingsOpen => {
                let p = self.paths.settings.clone();
                if !p.exists() {
                    let _ = self.paths.ensure();
                }
                self.open_path(&p);
            }
            Cmd::SettingsDefaults => {
                self.new_file();
                let s = self.settings.clone();
                if let Some(d) = self.doc_mut() {
                    d.ed = anvil_core::Editor::new(anvil_core::Buffer::from_text(anvil_config::DEFAULT_SETTINGS_TOML), edit_config(d.lang, &s));
                    d.name = "settings.default.toml".into();
                    d.lang = anvil_syntax::by_id("toml").unwrap_or(d.lang);
                    d.hl_version = u64::MAX;
                }
            }
            Cmd::SettingsReload => {
                self.reload_settings();
                self.status("Настройки перечитаны");
            }
            Cmd::SettingsFolder => opener(&self.paths.dir.clone()),
            Cmd::SettingsProject => match self.ws.root.clone() {
                Some(r) => {
                    let dir = r.join(".anvil");
                    let file = dir.join("settings.toml");
                    if !file.exists() {
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::write(
                            &file,
                            "# Настройки этого проекта — перекрывают пользовательские.\n# Например, подкрасить окно проекта (как Peacock):\n[theme]\n# tint = \"#2fc6f6\"\n",
                        );
                    }
                    self.open_path(&file);
                }
                None => self.toast("Сначала откройте папку проекта", ToastKind::Info),
            },
            Cmd::ImportVscodeSettings => self.import_vscode_settings(),
            Cmd::ThemeImportFile => {
                let mut dlg = rfd::FileDialog::new().add_filter("Тема VS Code", &["json", "jsonc"]);
                if let Some(d) = vscode_settings::extension_dirs().into_iter().find(|d| d.is_dir()) {
                    dlg = dlg.set_directory(d);
                }
                if let Some(f) = dlg.pick_file() {
                    self.import_theme_file(&f);
                }
            }
            Cmd::ThemesFolder => {
                let _ = std::fs::create_dir_all(&self.paths.themes);
                opener(&self.paths.themes.clone());
            }
            Cmd::PacksFolder => {
                let _ = std::fs::create_dir_all(&self.paths.packs);
                opener(&self.paths.packs.clone());
            }
            Cmd::ToolsRescan => {
                self.tools.scanned = false;
                self.rescan_tools();
            }
            Cmd::ToggleAnimations => {
                let v = self.settings.animations.enabled;
                self.toggle_setting(&["animations", "enabled"], v);
            }
            Cmd::ToggleCursorTrail => {
                let v = self.settings.cursor.trail;
                self.toggle_setting(&["cursor", "trail"], v);
            }
            Cmd::ToggleRainbowIndent => {
                let v = self.settings.editor.rainbow_indent;
                self.toggle_setting(&["editor", "rainbow_indent"], v);
            }
            Cmd::ToggleBrackets => {
                let v = self.settings.editor.bracket_colorization;
                self.toggle_setting(&["editor", "bracket_colorization"], v);
            }
            Cmd::RefreshTree => {
                self.ws.refresh();
                self.ws.reindex(self.wake.clone());
            }
            Cmd::About => {
                let msg = format!(
                    "Anvil {} · {} языков в паках · шрифт {} · настройки: {}",
                    env!("CARGO_PKG_VERSION"),
                    self.registry.language_count(),
                    self.font_report.editor,
                    self.paths.settings.display()
                );
                self.toast(msg, ToastKind::Info);
            }
            Cmd::LspComplete => self.lsp_complete(),
            Cmd::LspHover => self.lsp_hover_at_cursor(),
            Cmd::GotoDefinition => self.lsp_goto_definition(),
            Cmd::FindReferences => self.lsp_references(),
            Cmd::RenameSymbol => self.lsp_start_rename(),
            Cmd::FormatDocument => {
                self.lsp_format(false);
            }
            Cmd::NextProblem => self.lsp_next_problem(true),
            Cmd::PrevProblem => self.lsp_next_problem(false),
            Cmd::LspRestart => self.lsp_restart(),
            Cmd::ViewProblems => {
                let showing = self.panel.open && self.panel.tab == PanelTab::Problems;
                self.panel.open = !showing;
                self.panel.tab = PanelTab::Problems;
            }
            Cmd::AppQuit => self.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    pub fn find_step(&mut self, forward: bool) {
        let Some(d) = self.doc_mut() else { return };
        let m = &d.view.find_matches;
        if m.is_empty() {
            return;
        }
        let head = d.ed.sel.primary();
        let i = if forward {
            m.iter().position(|r| r.0 >= head.end().max(head.start() + usize::from(!head.is_empty()))).unwrap_or(0)
        } else {
            m.iter().rposition(|r| r.1 <= head.start()).unwrap_or(m.len() - 1)
        };
        let (a, b) = m[i];
        d.ed.sel = anvil_core::Selections::from_one(anvil_core::Selection::new(a, b));
        d.view.reveal = true;
        d.view.center = true;
        self.find.current = i;
    }

    // ------------------------------------------------------------------ кадр

    fn frame(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.now = ctx.input(|i| i.time);
        self.dt = ctx.input(|i| i.stable_dt).clamp(0.001, 0.1);
        self.frame_no += 1;
        if self.launched < 0.0 {
            self.launched = self.now;
        }
        self.tw.begin_frame(self.now);
        if self.frame_no == 3 && !self.tools.scanned {
            self.rescan_tools();
        }
        self.poll_background();
        self.lsp_tick();
        let theme_moving = self.theme.tick(self.now);
        self.colors = Colors::new(&self.theme.current, &self.settings);

        let events = ctx.input(|i| i.events.clone());
        let rest = self.handle_keys(events);

        // запуск: лёгкое проявление и подъём
        let launch = self.tw.since(self.launched, self.spec(&self.settings.animations.launch.clone()));
        let layer = ui.layer_id();
        let full = ui.max_rect();
        if launch < 1.0 {
            let s = 0.985 + 0.015 * launch;
            let c = full.center().to_vec2();
            ctx.set_transform_layer(layer, TSTransform::new(c * (1.0 - s) + vec2(0.0, (1.0 - launch) * 8.0), s));
            ui.set_opacity(launch.max(0.02));
        } else if self.frame_no < 200 {
            ctx.set_transform_layer(layer, TSTransform::IDENTITY);
        }

        let p = ui.painter().clone();
        p.rect_filled(full, egui::CornerRadius::ZERO, self.colors.bg);

        let s = &self.settings.ui;
        let dens = s.density.factor();
        let zen = self.zen || s.zen;
        let title_h = if self.settings.window.custom_titlebar && !zen { (s.titlebar_height * dens).round() } else { 0.0 };
        let status_h = if s.statusbar && !zen { (s.statusbar_height * dens).round() } else { 0.0 };
        let title_rect = Rect::from_min_max(full.min, pos2(full.max.x, full.min.y + title_h));
        let status_rect = Rect::from_min_max(pos2(full.min.x, full.max.y - status_h), full.max);
        let body = Rect::from_min_max(pos2(full.min.x, title_rect.max.y), pos2(full.max.x, status_rect.min.y));

        let sb_visible = s.sidebar_visible && !self.sidebar_hidden && !zen;
        let sb_spec = self.spec(&self.settings.animations.panels.clone());
        let sb_k = self.tw.value("sidebar-open", if sb_visible { 1.0 } else { 0.0 }, sb_spec);
        let sb_w = s.sidebar_width.clamp(160.0, 600.0) * sb_k;
        let right = s.sidebar == anvil_config::Side::Right;
        let side_k = self.tw.value("sidebar-side", if right { 1.0 } else { 0.0 }, sb_spec);
        let sb_x = egui::lerp(body.min.x..=body.max.x - sb_w, side_k);
        let sb_rect = Rect::from_min_size(pos2(sb_x, body.min.y), vec2(sb_w, body.height()));
        let main_x0 = egui::lerp(body.min.x + sb_w..=body.min.x, side_k);
        let main = Rect::from_min_size(pos2(main_x0, body.min.y), vec2(body.width() - sb_w, body.height()));

        let panel_k = self.tw.value("panel-open", if self.panel.open && !zen { 1.0 } else { 0.0 }, sb_spec);
        let panel_h = (s.panel_height.clamp(80.0, 900.0) * panel_k).min(main.height() * 0.7);
        let panel_rect = Rect::from_min_max(pos2(main.min.x, main.max.y - panel_h), main.max);
        let editor_area = Rect::from_min_max(main.min, pos2(main.max.x, panel_rect.min.y));

        // порядок: основное содержимое → панели → оверлеи (последние — сверху)
        self.draw_editor_area(ui, editor_area, &rest, zen);
        if panel_h > 0.5 {
            self.draw_panel(ui, panel_rect, panel_k);
        }
        if sb_w > 0.5 {
            self.draw_sidebar(ui, sb_rect, sb_k, right);
        }
        if title_h > 0.0 {
            self.draw_titlebar(ui, title_rect);
        }
        if status_h > 0.0 {
            self.draw_statusbar(ui, status_rect);
        }
        self.draw_lsp_popups(ui);
        self.draw_find(ui, editor_area, &rest);
        self.draw_palette(ui, full, &rest);
        self.draw_toasts(ui, Rect::from_min_max(full.min, pos2(full.max.x, status_rect.min.y)));
        if self.settings.window.custom_titlebar {
            self.window_edges(ui, full);
        }

        // кадры для счётчика
        self.frame_times.push(self.dt);
        if self.frame_times.len() > 60 {
            self.frame_times.remove(0);
        }

        // перерисовка — только пока что-то движется
        let busy = self.tw.active
            || theme_moving
            || !self.particles.is_empty()
            || self.panel.run.is_some()
            || self.tools.scanning
            || self.palette.as_ref().is_some_and(|p| p.closing.is_some());
        if busy {
            ctx.request_repaint();
        } else if let Some(next) = self.next_blink_repaint() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(next));
        }
        if !self.toasts.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        self.toasts.retain(|t| (self.now - t.born) < t.ttl as f64 + 0.5);

        // демонстрационные команды и снимок экрана
        if self.frame_no == 3
            && let (Some((l, col)), Some(d)) = (self.args.goto, self.doc_mut())
        {
            let rope = d.ed.rope();
            let line = l.saturating_sub(1).min(rope.len_lines().saturating_sub(1));
            let pos = (rope.line_to_char(line) + col.saturating_sub(1)).min(anvil_core::text::line_end(rope, line));
            d.ed.set_cursor(pos);
            d.view.reveal = true;
            d.view.center = true;
        }
        if self.frame_no == self.args.run_at && !self.args.run.is_empty() {
            for id in self.args.run.clone() {
                match crate::commands::by_id(&id) {
                    Some(c) => self.exec(c),
                    None => self.toast(format!("--run: неизвестная команда {id}"), ToastKind::Warn),
                }
            }
        }
        let script: Vec<String> = self.args.script.iter().filter(|(n, _)| *n == self.frame_no).map(|(_, a)| a.clone()).collect();
        for action in script {
            self.script_action(&action);
        }
        if self.args.script.iter().any(|(n, _)| *n >= self.frame_no) {
            ctx.request_repaint();
        }
        if let Some(path) = self.args.screenshot.clone() {
            if self.frame_no == self.args.frames {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    let _ = write_bmp(&path, &img);
                    std::process::exit(0);
                })));
            }
            if self.frame_no <= self.args.frames + 5 {
                ctx.request_repaint();
            }
        }
    }

    /// Действие сценария `--script`.
    fn script_action(&mut self, action: &str) {
        let (verb, rest) = action.split_once(' ').unwrap_or((action, ""));
        match verb {
            "goto" => {
                let mut it = rest.split_whitespace().filter_map(|x| x.parse::<usize>().ok());
                let (l, col) = (it.next().unwrap_or(1), it.next().unwrap_or(1));
                if let Some(d) = self.doc_mut() {
                    let rope = d.ed.rope();
                    let line = l.saturating_sub(1).min(rope.len_lines().saturating_sub(1));
                    let pos = (rope.line_to_char(line) + col.saturating_sub(1)).min(anvil_core::text::line_end(rope, line));
                    d.ed.set_cursor(pos);
                    d.view.reveal = true;
                }
            }
            "type" => {
                // в сценарии «\n» записывается двумя символами
                let text = rest.replace("\\n", "\n");
                let doc_id = self.doc().map(|d| d.id);
                if let (Some(d), Some(id)) = (self.doc_mut(), doc_id) {
                    d.ed.insert_text(&text);
                    d.view.reveal = true;
                    let last = text.chars().last();
                    self.after_typing(id, last, false, false);
                }
            }
            "run" => match crate::commands::by_id(rest.trim()) {
                Some(c) => self.exec(c),
                None => self.toast(format!("--script: неизвестная команда {rest}"), ToastKind::Warn),
            },
            "shot" => {
                let path = PathBuf::from(rest.trim());
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                    let _ = write_bmp(&path, &img);
                })));
            }
            "input" => {
                if let Some(p) = self.palette.as_mut() {
                    p.input.set(rest);
                    p.previewed = None;
                }
                self.refresh_palette();
            }
            "enter" => {
                let a = self.palette.as_ref().and_then(|p| p.items.get(p.sel).map(|i| i.action.clone()));
                if let Some(a) = a {
                    self.run_pal_action(a);
                }
            }
            "esc" => {
                self.lsp.hover = None;
                self.lsp.completion = None;
                self.close_palette(false);
            }
            "quit" => {
                self.lsp_shutdown_all();
                std::process::exit(0);
            }
            other => self.toast(format!("--script: неизвестное действие {other}"), ToastKind::Warn),
        }
    }

    /// Когда нужен следующий кадр для мигания курсора (None — курсор стоит).
    fn next_blink_repaint(&self) -> Option<f32> {
        let d = self.doc()?;
        if self.focus != Focus::Editor || !self.window_focused {
            return None;
        }
        let c = &self.settings.cursor;
        let idle = (self.now - d.view.activity) as f32;
        if c.blink == anvil_config::CursorBlink::Solid || (c.blink_timeout_s > 0 && idle > c.blink_timeout_s as f32) {
            return None;
        }
        let period = (c.blink_interval_ms.max(200) as f32) / 1000.0;
        match c.blink {
            anvil_config::CursorBlink::Blink => {
                let half = period / 2.0;
                Some(half - (idle % half) + 0.005)
            }
            _ => Some(1.0 / 30.0),
        }
    }
}

/// Простейшая запись BMP (без зависимостей) — для `--screenshot`.
fn write_bmp(path: &Path, img: &egui::ColorImage) -> std::io::Result<()> {
    let [w, h] = img.size;
    let row = w * 3;
    let pad = (4 - row % 4) % 4;
    let data_size = (row + pad) * h;
    let mut out = Vec::with_capacity(54 + data_size);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + data_size) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(data_size as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    for y in (0..h).rev() {
        for x in 0..w {
            let c = img.pixels[y * w + x];
            out.extend_from_slice(&[c.b(), c.g(), c.r()]);
        }
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    std::fs::write(path, out)
}

impl eframe::App for AnvilApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame(ui);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        if self.settings.window.opacity < 0.999 { [0.0, 0.0, 0.0, 0.0] } else { self.colors.bg.to_normalized_gamma_f32() }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_session();
        self.lsp_shutdown_all();
        if let Some(r) = &self.panel.run {
            r.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_toml_preserves_comments_and_creates_tables() {
        let mut doc: DocumentMut = "# шапка\n[theme]\n# комментарий\nname = \"anvil-dark\" # хвост\n".parse().unwrap_or_default();
        set_toml(&mut doc, &["theme", "name"], Some(Value::from("midnight")));
        set_toml(&mut doc, &["cursor", "trail"], Some(Value::from(false)));
        let s = doc.to_string();
        assert!(s.contains("# шапка"));
        assert!(s.contains("# комментарий"));
        assert!(s.contains("name = \"midnight\" # хвост"), "{s}");
        assert!(s.contains("trail = false"), "{s}");
    }

    #[test]
    fn set_toml_inside_inline_table() {
        let mut doc: DocumentMut = "[animations]\ntyping = { effect = \"none\", intensity = 1.0 }\n".parse().unwrap_or_default();
        set_toml(&mut doc, &["animations", "typing", "effect"], Some(Value::from("sparks")));
        assert!(doc.to_string().contains("effect = \"sparks\""), "{doc}");
    }
}
