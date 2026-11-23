//! Языковые серверы в приложении: запуск по требованию, синхронизация документов,
//! запросы (дополнение, подсказки, переход, ссылки, переименование, форматирование)
//! и диагностика.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anvil_lsp::lsp_types::{Position, Range, TextEdit};
use anvil_lsp::markdown::{self, Block};
use anvil_lsp::{Client, Enc, Incoming, ServerDef, ServerRegistry, pos, servers, snippet, uri};
use eframe::egui;
use serde_json::{Value, json};

use crate::app::{AnvilApp, ToastKind};
use crate::ui::fuzzy;
use crate::ui::overlay::{PalAction, PalItem, PalMode, Palette};

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Starting,
    Ready,
    Failed(String),
}

pub struct Server {
    pub def: ServerDef,
    pub client: Option<Client>,
    pub status: Status,
    pub init_id: i64,
    pub enc: Enc,
    pub triggers: Vec<String>,
    pub caps: Value,
    /// Прогресс индексации: токен → (текст, проценты).
    pub progress: BTreeMap<String, (String, Option<u64>)>,
}

#[derive(Debug, Clone)]
pub struct Diag {
    pub range: Range,
    pub severity: u8,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub enum Pending {
    Completion { doc: u64, anchor: usize },
    Hover { doc: u64, at: usize },
    Definition,
    References,
    Rename,
    Format { doc: u64, save_after: bool },
}

#[derive(Debug, Clone)]
pub struct CItem {
    pub label: String,
    pub detail: String,
    pub doc: String,
    pub kind: u32,
    pub insert: String,
    pub snippet: bool,
    pub edit_start: Option<Position>,
    pub filter: String,
    pub sort: String,
    pub extra: Vec<TextEdit>,
    pub deprecated: bool,
}

pub struct Completion {
    pub doc: u64,
    pub anchor: usize,
    pub items: Vec<CItem>,
    pub shown: Vec<(usize, Vec<usize>)>,
    pub sel: usize,
    pub opened: f64,
    pub incomplete: bool,
}

pub struct Hover {
    pub doc: u64,
    pub at: usize,
    pub blocks: Vec<Block>,
    pub diags: Vec<(u8, String)>,
    pub anchor: egui::Pos2,
    pub shown_at: f64,
    /// Открыто наведением мыши (закрывается, когда мышь уходит) или с клавиатуры.
    pub from_mouse: bool,
}

pub struct DocLsp {
    pub server: String,
    pub uri: String,
    pub version: i32,
    pub synced: u64,
    pub opened: bool,
    pub lang: String,
}

#[derive(Default)]
pub struct Lsp {
    pub reg: ServerRegistry,
    pub servers: HashMap<String, Server>,
    pub docs: HashMap<u64, DocLsp>,
    pub pending: HashMap<(String, i64), (Pending, f64)>,
    pub diags: HashMap<String, Vec<Diag>>,
    /// Ключ диагностики → настоящий путь и кодировка позиций сервера.
    pub diag_src: HashMap<String, (PathBuf, Enc)>,
    /// Растёт при каждом изменении диагностики — для кэшей интерфейса.
    pub diag_serial: u64,
    pub completion: Option<Completion>,
    pub hover: Option<Hover>,
    /// Указатель замер: (позиция, с какого момента, символ под ним, запрос отправлен).
    pub hover_probe: Option<(egui::Pos2, f64, usize, bool)>,
    pub want_completion: Option<(u64, f64)>,
    pub warned: HashSet<String>,
    /// Последний запрос переименования (для сообщения после ответа).
    pub rename_to: String,
}

/// Идентификатор языка LSP по нашему id и расширению файла.
pub fn language_id(lang: &str, path: &Path) -> String {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match (lang, ext.as_str()) {
        ("tsx", _) => "typescriptreact".into(),
        ("javascript", "jsx") => "javascriptreact".into(),
        ("css", "scss") => "scss".into(),
        ("css", "less") => "less".into(),
        ("json", "jsonc") => "jsonc".into(),
        ("shell", _) => "shellscript".into(),
        ("batch", _) => "bat".into(),
        ("make", _) => "makefile".into(),
        _ => lang.to_string(),
    }
}

fn severity_of(v: &Value) -> u8 {
    v.get("severity").and_then(Value::as_u64).unwrap_or(1).clamp(1, 4) as u8
}

fn markup_to_md(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(markup_to_md).collect::<Vec<_>>().join("\n\n---\n\n"),
        Value::Object(o) => {
            if let Some(lang) = o.get("language").and_then(Value::as_str) {
                format!("```{lang}\n{}\n```", o.get("value").and_then(Value::as_str).unwrap_or_default())
            } else {
                o.get("value").and_then(Value::as_str).unwrap_or_default().to_string()
            }
        }
        _ => String::new(),
    }
}

fn parse_range(v: &Value) -> Option<Range> {
    serde_json::from_value(v.clone()).ok()
}

fn parse_edits(v: &Value) -> Vec<TextEdit> {
    v.as_array().map(|a| a.iter().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect()).unwrap_or_default()
}

/// Location | Location[] | LocationLink[] → (uri, range).
fn parse_locations(v: &Value) -> Vec<(String, Range)> {
    let one = |x: &Value| -> Option<(String, Range)> {
        if let Some(u) = x.get("targetUri").and_then(Value::as_str) {
            let r = x.get("targetSelectionRange").or_else(|| x.get("targetRange")).and_then(parse_range)?;
            Some((u.to_string(), r))
        } else {
            Some((x.get("uri")?.as_str()?.to_string(), parse_range(x.get("range")?)?))
        }
    };
    match v {
        Value::Array(a) => a.iter().filter_map(one).collect(),
        Value::Object(_) => one(v).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn parse_completion(v: &Value) -> (Vec<CItem>, bool) {
    let (items, incomplete) = match v {
        Value::Array(a) => (a.clone(), false),
        Value::Object(o) => {
            (o.get("items").and_then(Value::as_array).cloned().unwrap_or_default(), o.get("isIncomplete").and_then(Value::as_bool).unwrap_or(false))
        }
        _ => (Vec::new(), false),
    };
    let out = items
        .iter()
        .filter_map(|it| {
            let label = it.get("label")?.as_str()?.to_string();
            let s = |k: &str| it.get(k).and_then(Value::as_str).map(str::to_string);
            let label_detail = it.pointer("/labelDetails/detail").and_then(Value::as_str).unwrap_or_default();
            let detail = s("detail").unwrap_or_else(|| it.pointer("/labelDetails/description").and_then(Value::as_str).unwrap_or_default().to_string());
            let (insert, edit_start) = match it.get("textEdit") {
                Some(te) => (
                    te.get("newText").and_then(Value::as_str).unwrap_or(&label).to_string(),
                    te.get("range").or_else(|| te.get("replace")).and_then(parse_range).map(|r| r.start),
                ),
                None => (s("insertText").unwrap_or_else(|| label.clone()), None),
            };
            Some(CItem {
                label: format!("{label}{label_detail}"),
                detail,
                doc: it.get("documentation").map(markup_to_md).unwrap_or_default(),
                kind: it.get("kind").and_then(Value::as_u64).unwrap_or(1) as u32,
                insert,
                snippet: it.get("insertTextFormat").and_then(Value::as_u64) == Some(2),
                edit_start,
                filter: s("filterText").unwrap_or(label.clone()),
                sort: s("sortText").unwrap_or(label),
                extra: it.get("additionalTextEdits").map(parse_edits).unwrap_or_default(),
                deprecated: it.get("deprecated").and_then(Value::as_bool).unwrap_or(false)
                    || it.get("tags").and_then(Value::as_array).is_some_and(|t| t.iter().any(|x| x.as_u64() == Some(1))),
            })
        })
        .collect();
    (out, incomplete)
}

/// Буква и класс подсветки для вида элемента дополнения.
pub fn kind_badge(kind: u32) -> (&'static str, &'static str) {
    match kind {
        2..=4 => ("ƒ", "function"),
        5 | 10 => ("p", "property"),
        6 => ("v", "variable"),
        7 | 22 => ("S", "type"),
        8 => ("I", "type"),
        9 => ("M", "module"),
        11 => ("u", "number"),
        12 => ("=", "constant"),
        13 => ("E", "type"),
        14 => ("k", "keyword"),
        15 => ("s", "macro"),
        20 => ("e", "constant"),
        21 => ("C", "constant"),
        23 => ("!", "macro"),
        24 => ("+", "operator"),
        25 => ("T", "type"),
        17 | 19 => ("f", "string"),
        _ => ("·", "text"),
    }
}

impl AnvilApp {
    fn lsp_server_for(&self, lang: &str) -> Option<(ServerDef, PathBuf)> {
        let ov = self.settings.lsp.servers.get(lang);
        if ov.and_then(|o| o.enabled) == Some(false) {
            return None;
        }
        for def in self.lsp.reg.for_language(lang) {
            let mut def = def.clone();
            if let Some(o) = ov {
                if let Some(c) = &o.command {
                    def.command = vec![c.clone()];
                }
                if let Some(a) = &o.args {
                    def.args = a.clone();
                }
            }
            // известно, что сервер «найден, но не работает» (прокси rustup без компонента) — пропускаем
            if self.tools.servers.get(&def.id).is_some_and(|s| s.as_ref().is_some_and(|(_, works)| !works)) && ov.and_then(|o| o.command.as_ref()).is_none() {
                continue;
            }
            if let Some(exe) = servers::locate(&def) {
                return Some((def, exe));
            }
        }
        None
    }

    /// Подключить документ к языковому серверу (запустив его при необходимости).
    pub fn lsp_attach(&mut self, i: usize) {
        if !self.settings.lsp.enabled {
            return;
        }
        let Some(d) = self.docs.get(i) else { return };
        let Some(path) = d.path.clone() else { return };
        let (doc_id, lang) = (d.id, d.lang.id.to_string());
        if self.lsp.docs.contains_key(&doc_id) {
            return;
        }
        let Some((def, exe)) = self.lsp_server_for(&lang) else {
            if self.lsp.reg.for_language(&lang).next().is_some() && self.lsp.warned.insert(format!("missing:{lang}")) {
                let hint = self.lsp.reg.for_language(&lang).next().and_then(|s| s.install.current().map(str::to_string)).unwrap_or_default();
                self.status(format!("Языковой сервер для {lang} не найден{}", if hint.is_empty() { String::new() } else { format!(" — {hint}") }));
            }
            return;
        };
        let root = servers::project_root(&def, &path, self.ws.root.as_deref());
        let key = format!("{}@{}", def.id, uri::key_path(&root));
        if !self.lsp.servers.contains_key(&key) {
            match Client::spawn(&def.id, &exe, &def.args, &root, self.wake.clone()) {
                Ok(client) => {
                    let init_id = client.initialize();
                    self.lsp.servers.insert(
                        key.clone(),
                        Server {
                            def: def.clone(),
                            client: Some(client),
                            status: Status::Starting,
                            init_id,
                            enc: Enc::Utf16,
                            triggers: Vec::new(),
                            caps: Value::Null,
                            progress: BTreeMap::new(),
                        },
                    );
                }
                Err(e) => {
                    self.toast(format!("Не удалось запустить {}: {e}", def.name), ToastKind::Error);
                    return;
                }
            }
        }
        self.lsp
            .docs
            .insert(doc_id, DocLsp { server: key, uri: uri::from_path(&path), version: 0, synced: u64::MAX, opened: false, lang: language_id(&lang, &path) });
    }

    pub fn lsp_detach(&mut self, doc_id: u64) {
        if let Some(dl) = self.lsp.docs.remove(&doc_id)
            && dl.opened
            && let Some(c) = self.lsp.servers.get(&dl.server).and_then(|s| s.client.as_ref())
        {
            c.notify("textDocument/didClose", json!({ "textDocument": { "uri": dl.uri } }));
        }
        if self.lsp.completion.as_ref().is_some_and(|c| c.doc == doc_id) {
            self.lsp.completion = None;
        }
    }

    pub fn lsp_shutdown_all(&mut self) {
        for s in self.lsp.servers.values() {
            if let Some(c) = &s.client {
                c.shutdown();
            }
        }
        self.lsp.servers.clear();
        self.lsp.docs.clear();
        self.lsp.pending.clear();
        self.lsp.diags.clear();
        self.lsp.diag_src.clear();
        self.lsp.diag_serial += 1;
        self.lsp.completion = None;
        self.lsp.hover = None;
    }

    pub fn lsp_restart(&mut self) {
        self.lsp_shutdown_all();
        self.lsp.reg = ServerRegistry::load(Some(&self.paths.packs));
        self.lsp.warned.clear();
        for i in 0..self.docs.len() {
            self.lsp_attach(i);
        }
        self.status("Языковые серверы перезапущены");
    }

    /// Отправить серверу текущий текст документа, если он изменился.
    fn lsp_sync(&mut self, doc_id: u64, force: bool) -> bool {
        let Some(i) = self.docs.iter().position(|d| d.id == doc_id) else { return false };
        let Some(dl) = self.lsp.docs.get_mut(&doc_id) else { return false };
        let Some(srv) = self.lsp.servers.get(&dl.server) else { return false };
        if srv.status != Status::Ready {
            return false;
        }
        let Some(c) = srv.client.as_ref() else { return false };
        let d = &self.docs[i];
        let version = d.ed.buffer.version;
        if !dl.opened {
            dl.version = 1;
            c.notify(
                "textDocument/didOpen",
                json!({ "textDocument": { "uri": dl.uri, "languageId": dl.lang, "version": dl.version, "text": d.ed.rope().to_string() } }),
            );
            dl.opened = true;
            dl.synced = version;
            return true;
        }
        if dl.synced != version && (force || self.now - d.view.activity > 0.06) {
            dl.version += 1;
            c.notify(
                "textDocument/didChange",
                json!({ "textDocument": { "uri": dl.uri, "version": dl.version }, "contentChanges": [{ "text": d.ed.rope().to_string() }] }),
            );
            dl.synced = version;
        }
        true
    }

    pub fn lsp_did_save(&mut self, doc_id: u64) {
        if let Some(dl) = self.lsp.docs.get(&doc_id)
            && let Some(c) = self.lsp.servers.get(&dl.server).and_then(|s| s.client.as_ref())
        {
            c.notify("textDocument/didSave", json!({ "textDocument": { "uri": dl.uri } }));
        }
    }

    /// Отправить запрос по документу; позиция — символьная.
    fn lsp_request(&mut self, doc_id: u64, method: &str, at: Option<usize>, extra: Value, pending: Pending) -> bool {
        if !self.lsp_sync(doc_id, true) {
            return false;
        }
        let Some(i) = self.docs.iter().position(|d| d.id == doc_id) else { return false };
        let Some(dl) = self.lsp.docs.get(&doc_id) else { return false };
        let Some(srv) = self.lsp.servers.get(&dl.server) else { return false };
        let Some(c) = srv.client.as_ref() else { return false };
        let mut params = json!({ "textDocument": { "uri": dl.uri } });
        if let Some(at) = at {
            let p = pos::to_lsp(self.docs[i].ed.rope(), at, srv.enc);
            params["position"] = json!({ "line": p.line, "character": p.character });
        }
        if let (Value::Object(o), Value::Object(e)) = (&mut params, extra) {
            o.extend(e);
        }
        let id = c.request(method, params);
        self.lsp.pending.insert((dl.server.clone(), id), (pending, self.now));
        true
    }

    fn doc_enc(&self, doc_id: u64) -> Enc {
        self.lsp.docs.get(&doc_id).and_then(|dl| self.lsp.servers.get(&dl.server)).map(|s| s.enc).unwrap_or_default()
    }

    // ------------------------------------------------------------------ команды

    pub fn lsp_complete(&mut self) {
        let Some(d) = self.doc() else { return };
        let (id, head) = (d.id, d.ed.sel.primary().head);
        let rope = d.ed.rope();
        let mut a = head;
        while a > 0 && anvil_core::text::is_word(rope.char(a - 1)) {
            a -= 1;
        }
        if !self.lsp_request(id, "textDocument/completion", Some(head), json!({ "context": { "triggerKind": 1 } }), Pending::Completion { doc: id, anchor: a })
        {
            self.status("Языковой сервер ещё не готов");
        }
    }

    pub fn lsp_hover_at_cursor(&mut self) {
        let Some(d) = self.doc() else { return };
        let (id, head) = (d.id, d.ed.sel.primary().head);
        self.lsp_request(id, "textDocument/hover", Some(head), Value::Null, Pending::Hover { doc: id, at: head });
    }

    /// Описание символа в произвольной позиции (для наведения мышью).
    pub fn lsp_hover_at(&mut self, doc_id: u64, at: usize) -> bool {
        self.lsp_request(doc_id, "textDocument/hover", Some(at), Value::Null, Pending::Hover { doc: doc_id, at })
    }

    pub fn lsp_goto_definition(&mut self) {
        let Some(d) = self.doc() else { return };
        let (id, head) = (d.id, d.ed.sel.primary().head);
        if !self.lsp_request(id, "textDocument/definition", Some(head), Value::Null, Pending::Definition) {
            self.status("Переход к определению: нет языкового сервера");
        }
    }

    pub fn lsp_references(&mut self) {
        let Some(d) = self.doc() else { return };
        let (id, head) = (d.id, d.ed.sel.primary().head);
        if !self.lsp_request(id, "textDocument/references", Some(head), json!({ "context": { "includeDeclaration": true } }), Pending::References) {
            self.status("Поиск ссылок: нет языкового сервера");
        }
    }

    pub fn lsp_start_rename(&mut self) {
        let Some(d) = self.doc() else { return };
        if !self.lsp.docs.contains_key(&d.id) {
            self.status("Переименование: нет языкового сервера");
            return;
        }
        let rope = d.ed.rope();
        let (a, b) = anvil_core::text::word_at(rope, d.ed.sel.primary().head);
        let word = rope.slice(a..b).to_string();
        let (doc, at) = (d.id, d.ed.sel.primary().head);
        let mut pal = Palette::new(PalMode::Rename { doc, at, old: word.clone() }, self.now);
        pal.input.set(&word);
        pal.input.anchor = Some(0);
        self.palette = Some(pal);
        self.focus = crate::app::Focus::Palette;
        self.refresh_palette();
    }

    pub fn lsp_rename(&mut self, doc: u64, at: usize, new_name: &str) {
        self.lsp.rename_to = new_name.to_string();
        if !self.lsp_request(doc, "textDocument/rename", Some(at), json!({ "newName": new_name }), Pending::Rename) {
            self.status("Переименование: сервер не готов");
        }
    }

    pub fn lsp_format(&mut self, save_after: bool) -> bool {
        let Some(d) = self.doc() else { return false };
        let id = d.id;
        let opts = json!({ "options": { "tabSize": d.ed.cfg.tab_size, "insertSpaces": d.ed.cfg.insert_spaces } });
        let supported = self
            .lsp
            .docs
            .get(&id)
            .and_then(|dl| self.lsp.servers.get(&dl.server))
            .is_some_and(|s| s.caps.get("documentFormattingProvider").is_some_and(|v| !v.is_null() && v != &json!(false)));
        if !supported {
            if !save_after {
                self.status("Форматирование не поддерживается сервером");
            }
            return false;
        }
        self.lsp_request(id, "textDocument/formatting", None, opts, Pending::Format { doc: id, save_after })
    }

    /// Следующая/предыдущая проблема в текущем файле.
    pub fn lsp_next_problem(&mut self, forward: bool) {
        let Some(d) = self.doc() else { return };
        let Some(path) = d.path.clone() else { return };
        let enc = self.doc_enc(d.id);
        let Some(list) = self.lsp.diags.get(&uri::key_path(&path)) else { return };
        let rope = d.ed.rope();
        let mut starts: Vec<usize> = list.iter().map(|g| pos::from_lsp(rope, g.range.start, enc)).collect();
        starts.sort_unstable();
        starts.dedup();
        let head = d.ed.sel.primary().head;
        let target = if forward { starts.iter().find(|p| **p > head).or(starts.first()) } else { starts.iter().rev().find(|p| **p < head).or(starts.last()) };
        if let (Some(&t), Some(d)) = (target, self.doc_mut()) {
            d.ed.set_cursor(t);
            d.view.reveal = true;
            d.view.center = true;
        }
    }

    // ------------------------------------------------------------------ дополнение

    pub fn completion_refilter(&mut self) {
        let Some(c) = self.lsp.completion.as_mut() else { return };
        let Some(d) = self.docs.iter().find(|d| d.id == c.doc) else {
            self.lsp.completion = None;
            return;
        };
        let head = d.ed.sel.primary().head;
        let rope = d.ed.rope();
        if head < c.anchor || rope.char_to_line(head) != rope.char_to_line(c.anchor.min(rope.len_chars())) {
            self.lsp.completion = None;
            return;
        }
        let prefix = rope.slice(c.anchor..head).to_string();
        let mut shown: Vec<(usize, i32, Vec<usize>)> = c
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| fuzzy(&prefix, &it.filter).map(|(s, _)| (i, s, fuzzy(&prefix, &it.label).map(|x| x.1).unwrap_or_default())))
            .collect();
        if !prefix.is_empty() {
            shown.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| c.items[a.0].sort.cmp(&c.items[b.0].sort)));
        } else {
            shown.sort_by(|a, b| c.items[a.0].sort.cmp(&c.items[b.0].sort));
        }
        c.shown = shown.into_iter().take(200).map(|(i, _, hl)| (i, hl)).collect();
        c.sel = c.sel.min(c.shown.len().saturating_sub(1));
        if c.shown.is_empty() || (c.shown.len() == 1 && c.items[c.shown[0].0].filter == prefix) {
            self.lsp.completion = None;
        }
    }

    pub fn completion_accept(&mut self) {
        let Some(c) = self.lsp.completion.take() else { return };
        let Some(&(idx, _)) = c.shown.get(c.sel) else { return };
        let item = c.items[idx].clone();
        let enc = self.doc_enc(c.doc);
        let Some(d) = self.docs.iter_mut().find(|d| d.id == c.doc) else { return };
        let rope = d.ed.rope().clone();
        let head = d.ed.sel.primary().head;
        let start = item.edit_start.map(|p| pos::from_lsp(&rope, p, enc)).unwrap_or(c.anchor).min(head);
        let (text, cursor) = if item.snippet { snippet::expand(&item.insert) } else { (item.insert.clone(), None) };
        let len = text.chars().count();
        let mut ranges = vec![(start, head, text)];
        for e in &item.extra {
            let (a, b) = pos::range_to_chars(&rope, e.range, enc);
            if b <= start || a >= head {
                ranges.push((a, b, e.new_text.clone()));
            }
        }
        ranges.sort_by_key(|r| r.0);
        d.ed.replace_ranges(ranges);
        if let Some(off) = cursor {
            let end = d.ed.sel.primary().head;
            d.ed.set_cursor(end.saturating_sub(len - off.min(len)));
        }
        d.view.reveal = true;
        d.view.activity = self.now;
    }

    // ------------------------------------------------------------------ кадр

    pub fn lsp_tick(&mut self) {
        // входящие сообщения
        let mut inbox: Vec<(String, Incoming)> = Vec::new();
        for (key, s) in &self.lsp.servers {
            if let Some(c) = &s.client {
                while let Ok(m) = c.rx.try_recv() {
                    inbox.push((key.clone(), m));
                }
            }
        }
        for (key, m) in inbox {
            self.lsp_handle(&key, m);
        }
        // синхронизация текста
        let ids: Vec<u64> = self.lsp.docs.keys().copied().collect();
        for id in ids {
            self.lsp_sync(id, false);
        }
        // отложенное дополнение
        if let Some((doc, t)) = self.lsp.want_completion
            && self.now - t > 0.03
        {
            self.lsp.want_completion = None;
            if self.active.and_then(|i| self.docs.get(i)).is_some_and(|d| d.id == doc) {
                self.lsp_complete();
            }
        }
        // форматирование перед сохранением не должно блокировать сохранение навсегда
        let stale: Vec<(String, i64)> = self
            .lsp
            .pending
            .iter()
            .filter(|(_, (p, t))| matches!(p, Pending::Format { save_after: true, .. }) && self.now - t > 2.5)
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            if let Some((Pending::Format { doc, .. }, _)) = self.lsp.pending.remove(&k)
                && let Some(i) = self.docs.iter().position(|d| d.id == doc)
            {
                self.save_doc_now(i);
            }
        }
        if !self.lsp.pending.is_empty()
            || self.lsp.want_completion.is_some()
            || self.lsp.servers.values().any(|s| s.status == Status::Starting || !s.progress.is_empty())
        {
            self.ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn lsp_handle(&mut self, key: &str, m: Incoming) {
        match m {
            Incoming::Response { id, result } => {
                let is_init = self.lsp.servers.get(key).is_some_and(|s| s.init_id == id && s.status == Status::Starting);
                if is_init {
                    self.lsp_initialized(key, result);
                    return;
                }
                let Some((pending, _)) = self.lsp.pending.remove(&(key.to_string(), id)) else { return };
                match result {
                    Ok(v) => self.lsp_response(key, pending, v),
                    Err(e) => {
                        if !matches!(pending, Pending::Completion { .. } | Pending::Hover { .. }) {
                            self.toast(format!("Языковой сервер: {e}"), ToastKind::Warn);
                        }
                        if let Pending::Format { doc, save_after: true } = pending
                            && let Some(i) = self.docs.iter().position(|d| d.id == doc)
                        {
                            self.save_doc_now(i);
                        }
                    }
                }
            }
            Incoming::Notification { method, params } => match method.as_str() {
                "textDocument/publishDiagnostics" => {
                    let Some(u) = params.get("uri").and_then(Value::as_str) else { return };
                    let list: Vec<Diag> = params
                        .get("diagnostics")
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|d| {
                                    Some(Diag {
                                        range: parse_range(d.get("range")?)?,
                                        severity: severity_of(d),
                                        message: d.get("message")?.as_str()?.to_string(),
                                        source: d.get("source").and_then(Value::as_str).unwrap_or_default().to_string(),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let k = uri::key(u);
                    if list.is_empty() {
                        self.lsp.diags.remove(&k);
                        self.lsp.diag_src.remove(&k);
                    } else {
                        let enc = self.lsp.servers.get(key).map(|s| s.enc).unwrap_or_default();
                        if let Some(p) = uri::to_path(u) {
                            self.lsp.diag_src.insert(k.clone(), (p, enc));
                        }
                        self.lsp.diags.insert(k, list);
                    }
                    self.lsp.diag_serial += 1;
                }
                "$/progress" => {
                    let token = params.get("token").map(|t| t.to_string()).unwrap_or_default();
                    let v = params.get("value").cloned().unwrap_or(Value::Null);
                    let Some(s) = self.lsp.servers.get_mut(key) else { return };
                    match v.get("kind").and_then(Value::as_str) {
                        Some("end") => {
                            s.progress.remove(&token);
                        }
                        Some(kind) => {
                            let prev = s.progress.get(&token).map(|p| p.0.clone()).unwrap_or_default();
                            let title = v
                                .get("title")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                                .unwrap_or_else(|| prev.split(" · ").next().unwrap_or_default().to_string());
                            let msg = v.get("message").and_then(Value::as_str).unwrap_or_default();
                            let text = if msg.is_empty() { title } else { format!("{title} · {msg}") };
                            let pct = v.get("percentage").and_then(Value::as_u64);
                            if kind == "begin" || kind == "report" {
                                s.progress.insert(token, (text, pct));
                            }
                        }
                        None => {}
                    }
                }
                "window/showMessage" => {
                    let t = params.get("type").and_then(Value::as_u64).unwrap_or(3);
                    let msg = params.get("message").and_then(Value::as_str).unwrap_or_default().to_string();
                    let name = self.lsp.servers.get(key).map(|s| s.def.name.clone()).unwrap_or_default();
                    if t <= 2 {
                        self.toast(format!("{name}: {msg}"), if t == 1 { ToastKind::Error } else { ToastKind::Warn });
                    }
                }
                _ => {}
            },
            Incoming::Exited { stderr } => {
                let Some(s) = self.lsp.servers.get_mut(key) else { return };
                let name = s.def.name.clone();
                let hint = s.def.install.current().map(str::to_string);
                let first = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("процесс завершился").to_string();
                s.status = Status::Failed(first.clone());
                s.client = None;
                s.progress.clear();
                for dl in self.lsp.docs.values_mut().filter(|d| d.server == key) {
                    dl.opened = false;
                }
                let msg = match hint {
                    Some(h) if first.contains("Unknown binary") || first.contains("not installed") || first.contains("не найден") => {
                        format!("{name} не установлен. Установить: {h}")
                    }
                    _ => format!("{name} остановился: {first}"),
                };
                if self.lsp.warned.insert(format!("exit:{key}")) {
                    self.toast(msg, ToastKind::Warn);
                }
            }
        }
    }

    fn lsp_initialized(&mut self, key: &str, result: Result<Value, String>) {
        let Some(s) = self.lsp.servers.get_mut(key) else { return };
        match result {
            Ok(v) => {
                let caps = v.get("capabilities").cloned().unwrap_or(Value::Null);
                s.enc = caps.get("positionEncoding").and_then(Value::as_str).map(Enc::from_name).unwrap_or_default();
                s.triggers = caps
                    .pointer("/completionProvider/triggerCharacters")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                s.caps = caps;
                s.status = Status::Ready;
                if let Some(c) = &s.client {
                    c.notify("initialized", json!({}));
                }
                let name = s.def.name.clone();
                self.status(format!("{name} подключён"));
            }
            Err(e) => {
                s.status = Status::Failed(e.clone());
                let name = s.def.name.clone();
                self.toast(format!("{name}: ошибка инициализации — {e}"), ToastKind::Error);
            }
        }
    }

    fn lsp_response(&mut self, key: &str, pending: Pending, v: Value) {
        let enc = self.lsp.servers.get(key).map(|s| s.enc).unwrap_or_default();
        match pending {
            Pending::Completion { doc, anchor } => {
                if !self.settings.lsp.completion {
                    return;
                }
                let (items, incomplete) = parse_completion(&v);
                if items.is_empty() || self.active.and_then(|i| self.docs.get(i)).is_none_or(|d| d.id != doc) {
                    return;
                }
                let sel = self.lsp.completion.as_ref().filter(|c| c.doc == doc && c.anchor == anchor).map(|c| c.sel).unwrap_or(0);
                let opened = self.lsp.completion.as_ref().filter(|c| c.doc == doc).map(|c| c.opened).unwrap_or(self.now);
                self.lsp.completion = Some(Completion { doc, anchor, items, shown: Vec::new(), sel, opened, incomplete });
                self.completion_refilter();
            }
            Pending::Hover { doc, at } => {
                let md = v.get("contents").map(markup_to_md).unwrap_or_default();
                let blocks = markdown::parse(&md);
                if let Some(h) = self.lsp.hover.as_mut().filter(|h| h.doc == doc && h.at == at) {
                    h.blocks = blocks;
                } else if !blocks.is_empty() {
                    let anchor = self.cursor_screen_pos().unwrap_or_default();
                    self.lsp.hover = Some(Hover { doc, at, blocks, diags: Vec::new(), anchor, shown_at: self.now, from_mouse: false });
                }
            }
            Pending::Definition => {
                let locs = parse_locations(&v);
                match locs.len() {
                    0 => self.status("Определение не найдено"),
                    1 => self.open_location(&locs[0].0, locs[0].1.start, enc),
                    _ => self.show_locations("Определения", locs, enc),
                }
            }
            Pending::References => {
                let locs = parse_locations(&v);
                if locs.is_empty() {
                    self.status("Ссылок не найдено");
                } else {
                    self.show_locations("Ссылки", locs, enc);
                }
            }
            Pending::Rename => {
                let (files, edits) = self.apply_workspace_edit(&v, enc);
                let to = self.lsp.rename_to.clone();
                if edits == 0 {
                    self.status("Нечего переименовывать");
                } else {
                    let pl = |n: usize, one: &str, few: &str, many: &str| {
                        let w = if n % 10 == 1 && n % 100 != 11 {
                            one
                        } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
                            few
                        } else {
                            many
                        };
                        format!("{n} {w}")
                    };
                    let msg = format!(
                        "Переименовано в «{to}»: {} в {}. Проверьте и сохраните (Ctrl+K S)",
                        pl(edits, "правка", "правки", "правок"),
                        pl(files, "файле", "файлах", "файлах")
                    );
                    self.toast(msg, ToastKind::Ok);
                }
            }
            Pending::Format { doc, save_after } => {
                let edits = parse_edits(&v);
                if let Some(i) = self.docs.iter().position(|d| d.id == doc) {
                    let n = edits.len();
                    self.apply_text_edits(i, &edits, enc);
                    if save_after {
                        self.save_doc_now(i);
                    } else if n > 0 {
                        self.status("Отформатировано");
                    }
                }
            }
        }
    }

    pub fn apply_text_edits(&mut self, i: usize, edits: &[TextEdit], enc: Enc) {
        let Some(d) = self.docs.get_mut(i) else { return };
        if edits.is_empty() {
            return;
        }
        let rope = d.ed.rope().clone();
        let mut ranges: Vec<(usize, usize, String)> = edits
            .iter()
            .map(|e| {
                let (a, b) = pos::range_to_chars(&rope, e.range, enc);
                (a, b, e.new_text.replace("\r\n", "\n"))
            })
            .collect();
        ranges.sort_by_key(|r| (r.0, r.1));
        d.ed.replace_ranges(ranges);
        d.view.activity = self.now;
    }

    /// Применить WorkspaceEdit: файлы открываются во вкладках, правки — одним шагом undo на файл.
    fn apply_workspace_edit(&mut self, v: &Value, enc: Enc) -> (usize, usize) {
        let mut per_file: Vec<(String, Vec<TextEdit>)> = Vec::new();
        if let Some(Value::Object(ch)) = v.get("changes") {
            for (u, e) in ch {
                per_file.push((u.clone(), parse_edits(e)));
            }
        }
        if let Some(Value::Array(dc)) = v.get("documentChanges") {
            for c in dc {
                if let (Some(u), Some(e)) = (c.pointer("/textDocument/uri").and_then(Value::as_str), c.get("edits")) {
                    per_file.push((u.to_string(), parse_edits(e)));
                }
            }
        }
        let active = self.active;
        let mut files = 0;
        let mut total = 0;
        for (u, edits) in per_file {
            if edits.is_empty() {
                continue;
            }
            let Some(path) = uri::to_path(&u) else { continue };
            let Some(i) = self.open_path(&path) else { continue };
            total += edits.len();
            files += 1;
            self.apply_text_edits(i, &edits, enc);
        }
        if let Some(a) = active {
            self.active = Some(a.min(self.docs.len().saturating_sub(1)));
        }
        (files, total)
    }

    fn open_location(&mut self, u: &str, p: Position, enc: Enc) {
        let Some(path) = uri::to_path(u) else { return };
        let Some(i) = self.open_path(&path) else { return };
        let d = &mut self.docs[i];
        let at = pos::from_lsp(d.ed.rope(), p, enc);
        d.ed.set_cursor(at);
        d.view.reveal = true;
        d.view.center = true;
    }

    fn show_locations(&mut self, title: &str, mut locs: Vec<(String, Range)>, enc: Enc) {
        locs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.start.line.cmp(&b.1.start.line)).then(a.1.start.character.cmp(&b.1.start.character)));
        locs.dedup();
        let mut items = Vec::new();
        for (u, r) in locs.into_iter().take(500) {
            let Some(path) = uri::to_path(&u) else { continue };
            let line = r.start.line as usize;
            let preview = std::fs::read_to_string(&path).ok().and_then(|t| t.lines().nth(line).map(|l| l.trim().to_string())).unwrap_or_default();
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            items.push(PalItem {
                label: format!("{name}:{}", line + 1),
                detail: preview,
                hint: String::new(),
                hl: Vec::new(),
                action: PalAction::Location(path, r.start, enc),
                swatch: Vec::new(),
                current: false,
            });
        }
        let n = items.len();
        let mut pal = Palette::new(PalMode::Locations { title: format!("{title}: {n}") }, self.now);
        pal.fixed = items;
        self.palette = Some(pal);
        self.focus = crate::app::Focus::Palette;
        self.refresh_palette();
    }

    pub fn goto_location(&mut self, path: &Path, p: Position, enc: Enc) {
        self.open_location(&uri::from_path(path), p, enc);
    }

    /// Экранная позиция основного курсора (для всплывающих окон по клавиатуре).
    pub fn cursor_screen_pos(&self) -> Option<egui::Pos2> {
        let g = self.geom.as_ref()?;
        let d = self.docs.iter().find(|d| d.id == g.doc)?;
        let rope = d.ed.rope();
        let h = d.ed.sel.primary().head.min(rope.len_chars());
        let l = rope.char_to_line(h);
        let col = anvil_core::text::display_col(rope, h, g.tab);
        Some(egui::pos2(g.ox + col as f32 * g.char_w, g.oy + l as f32 * g.line_h))
    }

    /// Диагностика документа в символьных диапазонах.
    pub fn doc_diags(&self, doc_id: u64) -> Vec<(usize, usize, u8, String)> {
        let Some(d) = self.docs.iter().find(|d| d.id == doc_id) else { return Vec::new() };
        let Some(path) = &d.path else { return Vec::new() };
        let Some(list) = self.lsp.diags.get(&uri::key_path(path)) else { return Vec::new() };
        let enc = self.doc_enc(doc_id);
        let rope = d.ed.rope();
        list.iter()
            .take(3000)
            .map(|g| {
                let (a, b) = pos::range_to_chars(rope, g.range, enc);
                let msg = if g.source.is_empty() { g.message.clone() } else { format!("{} ({})", g.message, g.source) };
                (a, b, g.severity, msg)
            })
            .collect()
    }

    /// Счётчики ошибок и предупреждений по всем файлам.
    pub fn diag_counts(&self) -> (usize, usize) {
        let mut e = 0;
        let mut w = 0;
        for list in self.lsp.diags.values() {
            for g in list {
                match g.severity {
                    1 => e += 1,
                    2 => w += 1,
                    _ => {}
                }
            }
        }
        (e, w)
    }

    /// Состояние сервера текущего файла для строки состояния.
    pub fn lsp_status_label(&self) -> Option<(String, Option<u64>, bool, bool)> {
        let d = self.doc()?;
        let dl = self.lsp.docs.get(&d.id)?;
        let s = self.lsp.servers.get(&dl.server)?;
        Some(match &s.status {
            Status::Starting => (format!("{} · запуск…", s.def.name), None, true, false),
            Status::Failed(_) => (format!("{} · остановлен", s.def.name), None, false, true),
            Status::Ready => match s.progress.values().next() {
                Some((t, p)) => (format!("{} · {t}", s.def.name), *p, true, false),
                None => (s.def.name.clone(), None, false, false),
            },
        })
    }
}

/// Геометрия редактора прошлого кадра — для всплывающих окон.
#[derive(Clone, Copy, Debug)]
pub struct EditorGeom {
    pub doc: u64,
    pub ox: f32,
    pub oy: f32,
    pub line_h: f32,
    pub char_w: f32,
    pub tab: usize,
    pub font_size: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_completion_list_and_snippets() {
        let v = json!({
            "isIncomplete": true,
            "items": [
                { "label": "len", "kind": 2, "detail": "fn(&self) -> usize", "insertTextFormat": 2, "textEdit": { "range": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 6 } }, "newText": "len()$0" } },
                { "label": "push", "kind": 2, "labelDetails": { "detail": "(…)" }, "deprecated": true }
            ]
        });
        let (items, inc) = parse_completion(&v);
        assert!(inc);
        assert_eq!(items.len(), 2);
        assert!(items[0].snippet);
        assert_eq!(items[0].edit_start, Some(Position::new(0, 4)));
        assert_eq!(items[1].label, "push(…)");
        assert!(items[1].deprecated);
    }

    #[test]
    fn parses_all_location_shapes() {
        let loc = json!({ "uri": "file:///a.rs", "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 5 } } });
        let link = json!([{ "targetUri": "file:///b.rs", "targetRange": loc["range"], "targetSelectionRange": loc["range"] }]);
        assert_eq!(parse_locations(&loc).len(), 1);
        assert_eq!(parse_locations(&link)[0].0, "file:///b.rs");
        assert_eq!(parse_locations(&json!([loc.clone(), loc])).len(), 2);
    }

    #[test]
    fn hover_markup_shapes() {
        assert_eq!(markup_to_md(&json!({ "kind": "markdown", "value": "**x**" })), "**x**");
        assert_eq!(markup_to_md(&json!({ "language": "rust", "value": "fn f()" })), "```rust\nfn f()\n```");
        assert!(markup_to_md(&json!(["a", { "language": "c", "value": "int" }])).contains("```c"));
    }
}
