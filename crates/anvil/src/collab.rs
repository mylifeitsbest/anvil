//! Совместная работа: сессия, общие документы, чужие курсоры.
//!
//! Хозяин делится открытыми файлами проекта, гости видят их вкладками. Каждый кадр:
//! 1. свои правки из журналов буферов → CRDT → участникам;
//! 2. события сети: чужие правки → в редактор, курсоры, входы и выходы;
//! 3. свой курсор → участникам (не чаще 20 раз в секунду).
//!
//! Когда сессии нет, всё это стоит одну проверку `Option` за кадр.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use anvil_collab::tls::Fingerprint;
use anvil_collab::{Auth, Client, DocInfo, Edit, Event, FromRelay, Hello, Invite, Payload, Peer, PeerId, RelayAddr, Role, SharedText};
use anvil_config::{CollabSettings, CursorLabels};
use anvil_core::{Change, Selections};
use eframe::egui::Color32;

use crate::app::{AnvilApp, ToastKind};
use crate::doc::Document;

/// Цвета участников, если свой не задан.
pub const PALETTE: [u32; 8] = [0xff5fa2, 0x3cc8ff, 0xffb547, 0x7ee787, 0xc792ea, 0xff7b72, 0x56d4bc, 0xf0c674];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Connecting,
    Live,
}

pub struct PeerView {
    pub info: Peer,
    /// В каком общем документе курсор.
    pub doc: Option<u64>,
    /// Выделения в «липких» позициях (якорь, голова).
    pub sel: Vec<(Vec<u8>, Vec<u8>)>,
    /// Когда курсор двигался (для подписи с именем).
    pub moved: f64,
}

pub struct SharedDoc {
    /// Вкладка (`Document::id`); у гостя `None`, пока не пришёл текст или если вкладку закрыли.
    pub local: Option<u64>,
    pub path: String,
    pub text: SharedText,
    /// У гостя: начальное состояние получено.
    pub synced: bool,
}

pub struct Session {
    pub phase: Phase,
    client: Client,
    /// Встроенный relay (у хозяина без внешнего сервера).
    _relay: Option<anvil_relay::Relay>,
    pub me: PeerId,
    pub role: Role,
    pub host: bool,
    pub host_peer: PeerId,
    pub host_name: String,
    pub peers: BTreeMap<PeerId, PeerView>,
    /// Общие документы по идентификатору сессии (у хозяина он равен `Document::id`).
    pub docs: BTreeMap<u64, SharedDoc>,
    /// Ссылки-приглашения (редактирование, чтение) — только у хозяина.
    pub invites: Option<(String, String)>,
    link: (String, u16, Fingerprint),
    pub relay_label: String,
    sent: Option<(Option<u64>, Selections)>,
    sent_at: f64,
    resend: bool,
    client_id: u64,
}

impl Session {
    fn new(client: Client, relay: Option<anvil_relay::Relay>, host: bool, link: (String, u16, Fingerprint), relay_label: String) -> Self {
        // номер участника в CRDT: случайный, как в Yjs (32 бита)
        let client_id = anvil_collab::random_bytes::<4>().map(u32::from_le_bytes).unwrap_or_else(std::process::id) as u64;
        Self {
            phase: Phase::Connecting,
            client,
            _relay: relay,
            me: 0,
            role: if host { Role::Host } else { Role::Read },
            host,
            host_peer: 0,
            host_name: String::new(),
            peers: BTreeMap::new(),
            docs: BTreeMap::new(),
            invites: None,
            link,
            relay_label,
            sent: None,
            sent_at: -1.0,
            resend: false,
            client_id,
        }
    }

    fn shared_of(&self, local: u64) -> Option<u64> {
        self.docs.iter().find(|(_, d)| d.local == Some(local)).map(|(k, _)| *k)
    }
}

#[derive(Default)]
pub struct Collab {
    pub session: Option<Session>,
}

impl Collab {
    /// Общий ли документ (и его номер в сессии).
    pub fn shared_id(&self, local: u64) -> Option<u64> {
        self.session.as_ref()?.shared_of(local)
    }

    /// Цвета участников, у которых курсор в этом документе.
    pub fn viewers(&self, local: u64) -> Vec<Color32> {
        let Some(s) = &self.session else { return Vec::new() };
        let Some(id) = s.shared_of(local) else { return Vec::new() };
        s.peers.values().filter(|p| p.doc == Some(id)).map(|p| color32(p.info.color)).collect()
    }
}

pub fn color32(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub fn my_name(cs: &CollabSettings) -> String {
    let n = cs.name.trim();
    if !n.is_empty() {
        return n.chars().take(40).collect();
    }
    std::env::var("USERNAME").or_else(|_| std::env::var("USER")).ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Гость".into())
}

pub fn my_color(cs: &CollabSettings, name: &str) -> u32 {
    match cs.color {
        Some(c) => ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32,
        None => {
            let h = name.chars().fold(7u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
            PALETTE[h as usize % PALETTE.len()]
        }
    }
}

/// Первая буква имени для аватара.
pub fn initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".into())
}

/// Позиция после своих ещё не отправленных правок («липкая» позиция держится за символ справа).
fn through_journal(mut p: usize, journal: &[Change]) -> usize {
    for c in journal {
        let rem = c.removed.chars().count();
        let ins = c.inserted.chars().count();
        p = if p < c.start {
            p
        } else if p >= c.start + rem {
            p - rem + ins
        } else {
            c.start
        };
    }
    p
}

fn to_edits(j: Vec<Change>) -> Vec<Edit> {
    j.into_iter().map(|c| Edit { start: c.start, remove: c.removed.chars().count(), insert: c.inserted }).collect()
}

/// Правка для `Editor::apply_remote`: начало, сколько удалить, что вставить.
type RemoteEdit = (usize, usize, String);

fn to_tuples(e: Vec<Edit>) -> Vec<RemoteEdit> {
    e.into_iter().map(|e| (e.start, e.remove, e.insert)).collect()
}

/// Чужой курсор, готовый к отрисовке.
pub struct RemoteCaret {
    pub color: Color32,
    pub name: String,
    /// (якорь, голова) в символах; первым — основной курсор.
    pub sels: Vec<(usize, usize)>,
    /// Прозрачность подписи с именем.
    pub label: f32,
}

impl AnvilApp {
    // ------------------------------------------------------------------ начать, войти, выйти

    /// «Поделиться»: открыть сессию (или показать её, если уже открыта).
    pub fn collab_share(&mut self) {
        if self.collab.session.is_some() {
            self.open_palette(crate::ui::overlay::PalMode::Session);
            return;
        }
        let cs = self.settings.collab.clone();
        let name = my_name(&cs);
        let color = my_color(&cs, &name);
        let (connect, link, relay, label, key) = if cs.relay.trim().is_empty() {
            let id = match anvil_relay::Identity::load_or_create(&self.paths.dir.join("collab")) {
                Ok(i) => i,
                Err(e) => {
                    self.toast(format!("Совместная работа: {e}"), ToastKind::Error);
                    return;
                }
            };
            let Some(key) = anvil_collab::random_hex::<16>() else {
                self.toast("Совместная работа: недоступен генератор случайных чисел", ToastKind::Error);
                return;
            };
            let ip: IpAddr = cs.listen.trim().parse().unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
            let mut started = None;
            let mut err = String::new();
            for port in cs.port..=cs.port.saturating_add(9) {
                let mut cfg = anvil_relay::Config::new(SocketAddr::new(ip, port), id.clone());
                cfg.key = Some(key.clone());
                match anvil_relay::Relay::start(cfg) {
                    Ok(r) => {
                        started = Some(r);
                        break;
                    }
                    Err(e) => err = e.to_string(),
                }
            }
            let Some(relay) = started else {
                self.toast(format!("Не удалось запустить relay на порту {}: {err}", cs.port), ToastKind::Error);
                return;
            };
            let port = relay.addr().port();
            let fp = relay.fingerprint();
            let connect_ip = match ip {
                IpAddr::V4(v) if v.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(v) if v.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
                other => other,
            };
            let public = cs.public_host.trim();
            let link_host = if !public.is_empty() {
                public.to_string()
            } else if ip.is_loopback() {
                ip.to_string()
            } else {
                anvil_collab::local_ip().map(|i| i.to_string()).unwrap_or_else(|| "127.0.0.1".into())
            };
            let label = if ip.is_loopback() {
                "встроенный relay · этот компьютер"
            } else {
                "встроенный relay · локальная сеть"
            };
            ((connect_ip.to_string(), port, fp), (link_host, port, fp), Some(relay), label.to_string(), key)
        } else {
            match RelayAddr::parse(&cs.relay) {
                Ok(a) => ((a.host.clone(), a.port, a.fp), (a.host.clone(), a.port, a.fp), None, format!("relay {}", a.host), a.key),
                Err(e) => {
                    self.toast(format!("[collab] relay: {e}"), ToastKind::Error);
                    return;
                }
            }
        };
        let client = Client::start(connect.0, connect.1, connect.2, Hello { name, color, auth: Auth::Host { key } }, self.wake.clone());
        self.collab.session = Some(Session::new(client, relay, true, link, label));
        self.status("Открываю сессию…");
    }

    /// Войти в чужую сессию по ссылке-приглашению.
    pub fn collab_join(&mut self, link: &str) {
        if self.collab.session.is_some() {
            self.toast("Сначала завершите текущую сессию", ToastKind::Info);
            return;
        }
        let inv = match Invite::parse(link) {
            Ok(i) => i,
            Err(e) => {
                self.toast(format!("Ссылка не подходит: {e}"), ToastKind::Error);
                return;
            }
        };
        let cs = self.settings.collab.clone();
        let name = my_name(&cs);
        let color = my_color(&cs, &name);
        let client = Client::start(inv.host.clone(), inv.port, inv.fp, Hello { name, color, auth: Auth::Join { token: inv.token.clone() } }, self.wake.clone());
        let label = format!("relay {}", inv.host);
        self.status(format!("Подключаюсь к {}…", inv.host));
        self.collab.session = Some(Session::new(client, None, false, (inv.host, inv.port, inv.fp), label));
    }

    /// Завершить сессию (хозяин) или выйти из неё (гость). Вкладки гостя остаются копиями.
    pub fn collab_end(&mut self, why: Option<String>) {
        self.collab_flush();
        let Some(s) = self.collab.session.take() else { return };
        for sd in s.docs.values() {
            let Some(local) = sd.local else { continue };
            if let Some(d) = self.docs.iter_mut().find(|d| d.id == local) {
                d.ed.buffer.journal = None;
                if d.remote.is_some() {
                    d.remote = None;
                    d.ed.readonly = false;
                    d.name = format!("{} — копия", d.name);
                }
            }
        }
        let host = s.host;
        drop(s);
        match why {
            Some(w) => self.toast(w, ToastKind::Warn),
            None => self.status(if host { "Сессия завершена" } else { "Вы вышли из сессии" }),
        }
    }

    // ------------------------------------------------------------------ каждый кадр

    pub fn collab_tick(&mut self) {
        if self.collab.session.is_none() {
            return;
        }
        self.collab_flush();
        let mut events = Vec::new();
        if let Some(s) = &self.collab.session {
            while let Some(e) = s.client.try_recv() {
                events.push(e);
            }
        }
        for e in events {
            self.collab_event(e);
            if self.collab.session.is_none() {
                return;
            }
        }
        if self.collab.session.as_ref().is_some_and(|s| s.host && s.phase == Phase::Live) {
            self.collab_host_docs();
        }
        self.collab_send_cursor();
    }

    /// Свои правки → участникам. Вызывается до применения чужих — так буфер и CRDT совпадают.
    pub fn collab_flush(&mut self) {
        let Some(s) = self.collab.session.as_mut() else { return };
        for (sid, sd) in s.docs.iter_mut() {
            let Some(local) = sd.local else { continue };
            let Some(d) = self.docs.iter_mut().find(|d| d.id == local) else { continue };
            let j = d.ed.buffer.take_journal();
            if j.is_empty() {
                continue;
            }
            if let Some(update) = sd.text.apply_local(&to_edits(j)) {
                s.client.send(None, Payload::Update { doc: *sid, update });
            }
        }
    }

    fn collab_event(&mut self, ev: Event) {
        let now = self.now;
        match ev {
            Event::Closed(reason) => {
                let connecting = self.collab.session.as_ref().is_some_and(|s| s.phase == Phase::Connecting);
                self.collab_end(Some(if connecting {
                    format!("Не удалось подключиться: {reason}")
                } else {
                    format!("Связь с сессией потеряна: {reason}")
                }));
            }
            Event::Msg(FromRelay::Welcome { you, role, peers, invites }) => {
                let copy = self.settings.collab.copy_invite;
                // «Подключаюсь…» больше не актуально
                self.msg = None;
                let Some(s) = self.collab.session.as_mut() else { return };
                s.phase = Phase::Live;
                s.me = you;
                s.role = role;
                for p in peers {
                    if p.role == Role::Host {
                        s.host_peer = p.id;
                        s.host_name = p.name.clone();
                    }
                    if p.id != you {
                        s.peers.insert(p.id, PeerView { info: p, doc: None, sel: Vec::new(), moved: -100.0 });
                    }
                }
                if let Some(inv) = invites {
                    let (host, port, fp) = s.link.clone();
                    let edit = Invite { host: host.clone(), port, token: inv.edit, fp }.link();
                    let read = Invite { host, port, token: inv.read, fp }.link();
                    s.invites = Some((edit.clone(), read));
                    if copy {
                        self.ctx.copy_text(edit);
                    }
                    self.toast(
                        if copy {
                            "Сессия открыта · ссылка для редактирования скопирована"
                        } else {
                            "Сессия открыта — пригласите участников"
                        },
                        ToastKind::Ok,
                    );
                    self.collab_host_docs();
                } else {
                    let who = s.host_name.clone();
                    self.toast(format!("Вы в сессии {who} · {}", role.label()), ToastKind::Ok);
                }
            }
            Event::Msg(FromRelay::Refused { reason }) => self.collab_end(Some(format!("Relay отказал: {reason}"))),
            Event::Msg(FromRelay::Joined(p)) => {
                let Some(s) = self.collab.session.as_mut() else { return };
                let (name, label, id) = (p.name.clone(), p.role.label(), p.id);
                s.peers.insert(p.id, PeerView { info: p, doc: None, sel: Vec::new(), moved: now });
                s.resend = true;
                if s.host {
                    let list: Vec<DocInfo> = s.docs.iter().map(|(id, d)| DocInfo { id: *id, path: d.path.clone() }).collect();
                    s.client.send(Some(id), Payload::Docs(list));
                }
                self.toast(format!("{name} входит в сессию · {label}"), ToastKind::Info);
            }
            Event::Msg(FromRelay::Left(id)) => {
                let Some(s) = self.collab.session.as_mut() else { return };
                if let Some(p) = s.peers.remove(&id) {
                    self.toast(format!("{} выходит из сессии", p.info.name), ToastKind::Info);
                }
            }
            Event::Msg(FromRelay::Closed { reason }) => self.collab_end(Some(format!("Сессия завершена: {reason}"))),
            Event::Msg(FromRelay::From { peer, payload }) => self.collab_payload(peer, payload),
            Event::Msg(FromRelay::Pong) => {}
        }
    }

    fn collab_payload(&mut self, from: PeerId, p: Payload) {
        let now = self.now;
        match p {
            Payload::Docs(list) => self.collab_guest_docs(from, list),
            Payload::SyncRequest { doc, sv } => {
                let Some(s) = self.collab.session.as_mut() else { return };
                if !s.host {
                    return;
                }
                if let Some(update) = s.docs.get(&doc).and_then(|d| d.text.diff(&sv).ok()) {
                    s.client.send(Some(from), Payload::SyncState { doc, update });
                }
            }
            Payload::SyncState { doc, update } => self.collab_apply(doc, &update, true),
            Payload::Update { doc, update } => self.collab_apply(doc, &update, false),
            Payload::Cursor { doc, sel } => {
                let Some(s) = self.collab.session.as_mut() else { return };
                if let Some(pv) = s.peers.get_mut(&from) {
                    pv.doc = doc;
                    pv.sel = sel;
                    pv.moved = now;
                }
            }
            Payload::Save { doc } => {
                let Some(s) = self.collab.session.as_ref() else { return };
                if !s.host {
                    return;
                }
                let who = s.peers.get(&from).map(|p| p.info.name.clone()).unwrap_or_default();
                let local = s.docs.get(&doc).and_then(|d| d.local);
                if let Some(i) = local.and_then(|l| self.docs.iter().position(|d| d.id == l))
                    && self.save_doc_now(i)
                {
                    let name = self.docs[i].name.clone();
                    self.status(format!("{name} сохранён по просьбе: {who}"));
                }
            }
        }
    }

    /// Гость: список общих документов от хозяина.
    fn collab_guest_docs(&mut self, from: PeerId, list: Vec<DocInfo>) {
        let mut close = Vec::new();
        {
            let Some(s) = self.collab.session.as_mut() else { return };
            if s.host || from != s.host_peer {
                return;
            }
            for di in &list {
                match s.docs.get_mut(&di.id) {
                    Some(sd) => {
                        if sd.path != di.path {
                            sd.path = di.path.clone();
                            if let Some(l) = sd.local
                                && let Some(d) = self.docs.iter_mut().find(|d| d.id == l)
                            {
                                d.name = di.path.rsplit('/').next().unwrap_or(&di.path).to_string();
                                d.remote = Some(di.path.clone());
                                d.lang = anvil_syntax::detect(Path::new(&di.path));
                                d.hl_version = u64::MAX;
                            }
                        }
                    }
                    None => {
                        let text = SharedText::new(s.client_id, "");
                        let sv = text.state_vector();
                        s.docs.insert(di.id, SharedDoc { local: None, path: di.path.clone(), text, synced: false });
                        s.client.send(Some(s.host_peer), Payload::SyncRequest { doc: di.id, sv });
                    }
                }
            }
            let gone: Vec<u64> = s.docs.keys().filter(|k| !list.iter().any(|d| d.id == **k)).copied().collect();
            for g in gone {
                if let Some(sd) = s.docs.remove(&g)
                    && let Some(l) = sd.local
                {
                    close.push(l);
                }
            }
        }
        // хозяин закрыл файл — у гостя вкладка тоже закрывается (всё уже у хозяина)
        for l in close {
            if let Some(i) = self.docs.iter().position(|d| d.id == l) {
                self.close_tab(i, true);
            }
        }
    }

    /// Чужое обновление документа (`initial` — начальное состояние от хозяина).
    fn collab_apply(&mut self, doc: u64, update: &[u8], initial: bool) {
        let mut create: Option<(String, String, bool)> = None;
        let mut apply: Option<(u64, Vec<RemoteEdit>)> = None;
        {
            let Some(s) = self.collab.session.as_mut() else { return };
            let readonly = !s.role.can_edit();
            let Some(sd) = s.docs.get_mut(&doc) else { return };
            let edits = match sd.text.apply_remote(update) {
                Ok(e) => e,
                Err(e) => {
                    self.toast(format!("Совместная работа: {e}"), ToastKind::Warn);
                    return;
                }
            };
            if !sd.synced {
                if initial {
                    sd.synced = true;
                    create = Some((sd.path.clone(), sd.text.text(), readonly));
                }
            } else if let Some(l) = sd.local {
                apply = Some((l, to_tuples(edits)));
            }
        }
        if let Some((l, edits)) = apply
            && let Some(d) = self.docs.iter_mut().find(|d| d.id == l)
        {
            d.ed.apply_remote(&edits);
        }
        if let Some((path, text, readonly)) = create {
            let first = self.collab.session.as_ref().is_some_and(|s| s.docs.values().filter(|d| d.local.is_some()).count() == 0);
            let id = self.alloc_doc_id();
            let d = Document::remote(&path, &text, readonly, id, &self.settings, self.now);
            self.docs.push(d);
            if let Some(sd) = self.collab.session.as_mut().and_then(|s| s.docs.get_mut(&doc)) {
                sd.local = Some(id);
            }
            if first || self.active.is_none() {
                let i = self.docs.len() - 1;
                self.activate(i);
            }
        }
    }

    /// Хозяин: общие документы = открытые файлы проекта (кроме настроек Anvil и очень больших).
    fn collab_host_docs(&mut self) {
        let root = self.ws.root.clone();
        let cfg_dir = self.paths.dir.clone();
        let eligible: Vec<(u64, String)> = self
            .docs
            .iter()
            .filter(|d| !d.large && d.remote.is_none())
            .filter_map(|d| {
                let p = d.path.as_ref()?;
                if p.starts_with(&cfg_dir) {
                    return None;
                }
                let rel = match &root {
                    Some(r) => p.strip_prefix(r).ok()?,
                    None => Path::new(p.file_name()?),
                };
                Some((d.id, rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/")))
            })
            .collect();
        let Some(s) = self.collab.session.as_mut() else { return };
        let same = s.docs.len() == eligible.len() && eligible.iter().all(|(id, path)| s.docs.get(id).is_some_and(|d| &d.path == path));
        if same {
            return;
        }
        let removed: Vec<u64> = s.docs.keys().filter(|k| !eligible.iter().any(|(e, _)| e == *k)).copied().collect();
        for id in removed {
            s.docs.remove(&id);
            if let Some(d) = self.docs.iter_mut().find(|d| d.id == id) {
                d.ed.buffer.journal = None;
            }
        }
        for (id, path) in &eligible {
            if let Some(sd) = s.docs.get_mut(id) {
                sd.path = path.clone();
                continue;
            }
            let Some(d) = self.docs.iter_mut().find(|d| d.id == *id) else { continue };
            d.ed.buffer.journal = Some(Vec::new());
            let text = SharedText::new(s.client_id, &d.ed.rope().to_string());
            s.docs.insert(*id, SharedDoc { local: Some(*id), path: path.clone(), text, synced: true });
        }
        let list: Vec<DocInfo> = s.docs.iter().map(|(id, d)| DocInfo { id: *id, path: d.path.clone() }).collect();
        s.client.send(None, Payload::Docs(list));
        s.resend = true;
    }

    /// Свой курсор → участникам.
    fn collab_send_cursor(&mut self) {
        let now = self.now;
        let Some(s) = self.collab.session.as_mut() else { return };
        if s.phase != Phase::Live {
            return;
        }
        let cur = self.active.and_then(|i| self.docs.get(i)).and_then(|d| {
            let sid = s.docs.iter().find(|(_, sd)| sd.local == Some(d.id) && sd.synced).map(|(k, _)| *k)?;
            Some((sid, d.ed.sel.clone()))
        });
        let key = match &cur {
            Some((id, sel)) => (Some(*id), sel.clone()),
            None => (None, Selections::default()),
        };
        if s.sent.as_ref() == Some(&key) && !s.resend {
            return;
        }
        if now - s.sent_at < 0.05 && !s.resend {
            self.ctx.request_repaint_after(Duration::from_millis(50));
            return;
        }
        let payload = match &cur {
            Some((sid, sel)) => {
                let Some(sd) = s.docs.get(sid) else { return };
                let n = sel.list.len();
                let order = std::iter::once(sel.primary).chain((0..n).filter(|i| *i != sel.primary)).take(16);
                let list = order.filter_map(|i| sel.list.get(i)).map(|x| (sd.text.sticky(x.anchor), sd.text.sticky(x.head))).collect();
                Payload::Cursor { doc: Some(*sid), sel: list }
            }
            None => Payload::Cursor { doc: None, sel: Vec::new() },
        };
        s.client.send(None, payload);
        s.sent = Some(key);
        s.sent_at = now;
        s.resend = false;
    }

    // ------------------------------------------------------------------ действия

    /// Отмена в общем документе — только своих правок. `true`, если команда обработана здесь.
    pub fn collab_undo(&mut self, redo: bool) -> bool {
        let Some(i) = self.active else { return false };
        let Some(id) = self.docs.get(i).map(|d| d.id) else { return false };
        let Some(sid) = self.collab.shared_id(id) else { return false };
        if self.docs[i].ed.readonly {
            return true;
        }
        self.collab_flush();
        let Some(s) = self.collab.session.as_mut() else { return false };
        let Some(sd) = s.docs.get_mut(&sid) else { return false };
        let (edits, update) = if redo { sd.text.redo() } else { sd.text.undo() };
        if let Some(update) = update {
            s.client.send(None, Payload::Update { doc: sid, update });
        }
        if edits.is_empty() {
            self.status(if redo {
                "Нечего повторять"
            } else {
                "Нечего отменять — свои правки уже отменены"
            });
            return true;
        }
        let caret = edits.last().map(|e| e.start + e.insert.chars().count());
        let d = &mut self.docs[i];
        d.ed.apply_remote(&to_tuples(edits));
        if let Some(c) = caret {
            d.ed.set_cursor(c.min(d.ed.rope().len_chars()));
        }
        d.view.reveal = true;
        d.view.activity = self.now;
        true
    }

    /// Гость нажал «Сохранить»: файл у хозяина — просим его.
    pub fn collab_request_save(&mut self, i: usize) -> bool {
        let Some(d) = self.docs.get(i) else { return false };
        if d.remote.is_none() {
            return false;
        }
        let id = d.id;
        let Some(sid) = self.collab.shared_id(id) else { return false };
        if !self.collab.session.as_ref().is_some_and(|s| s.role.can_edit()) {
            self.status("Только чтение — сохраняет хозяин сессии");
            return true;
        }
        self.collab_flush();
        if let Some(s) = &self.collab.session {
            s.client.send(Some(s.host_peer), Payload::Save { doc: sid });
        }
        self.status("Файл сохранится у хозяина сессии");
        true
    }

    /// Гость: снова открыть общий документ, если вкладку закрыли.
    pub fn collab_open_doc(&mut self, sid: u64) {
        let Some(s) = self.collab.session.as_ref() else { return };
        let Some(sd) = s.docs.get(&sid) else { return };
        if let Some(i) = sd.local.and_then(|l| self.docs.iter().position(|d| d.id == l)) {
            self.activate(i);
            return;
        }
        if !sd.synced {
            return;
        }
        let (path, text, readonly) = (sd.path.clone(), sd.text.text(), !s.role.can_edit());
        let id = self.alloc_doc_id();
        self.docs.push(Document::remote(&path, &text, readonly, id, &self.settings, self.now));
        if let Some(sd) = self.collab.session.as_mut().and_then(|s| s.docs.get_mut(&sid)) {
            sd.local = Some(id);
        }
        let i = self.docs.len() - 1;
        self.activate(i);
    }

    /// Перейти к курсору участника.
    pub fn collab_goto_peer(&mut self, peer: PeerId) {
        let Some(s) = self.collab.session.as_ref() else { return };
        let Some(pv) = s.peers.get(&peer) else { return };
        let Some(sid) = pv.doc else {
            let name = pv.info.name.clone();
            self.status(format!("{name} сейчас не в общем файле"));
            return;
        };
        let head = s.docs.get(&sid).and_then(|sd| pv.sel.first().and_then(|(_, h)| sd.text.resolve(h)));
        self.collab_open_doc(sid);
        if let (Some(h), Some(d)) = (head, self.doc_mut()) {
            d.ed.set_cursor(h.min(d.ed.rope().len_chars()));
            d.view.reveal = true;
            d.view.center = true;
        }
    }

    pub fn collab_copy_invite(&mut self, read: bool) {
        let link = self.collab.session.as_ref().and_then(|s| s.invites.as_ref()).map(|(e, r)| if read { r.clone() } else { e.clone() });
        match link {
            Some(l) => {
                self.ctx.copy_text(l);
                self.status(if read {
                    "Ссылка «только чтение» скопирована"
                } else {
                    "Ссылка для редактирования скопирована"
                });
            }
            None => self.toast("Ссылки есть только у хозяина открытой сессии — сначала «Поделиться»", ToastKind::Info),
        }
    }

    /// Чужие курсоры в документе `idx` — для отрисовки.
    pub fn remote_carets(&mut self, idx: usize) -> Vec<RemoteCaret> {
        let Some(s) = self.collab.session.as_ref() else { return Vec::new() };
        let Some(d) = self.docs.get(idx) else { return Vec::new() };
        let Some((sid, sd)) = s.docs.iter().find(|(_, sd)| sd.local == Some(d.id)) else { return Vec::new() };
        let journal = d.ed.buffer.journal.as_deref().unwrap_or(&[]);
        let len = d.ed.rope().len_chars();
        let cs = &self.settings.collab;
        let mut out = Vec::new();
        let mut wake: Option<f32> = None;
        let mut moving = false;
        for pv in s.peers.values().filter(|p| p.doc == Some(*sid)) {
            let sels: Vec<(usize, usize)> = pv
                .sel
                .iter()
                .filter_map(|(a, h)| {
                    let hp = sd.text.resolve(h)?;
                    let ap = sd.text.resolve(a).unwrap_or(hp);
                    Some((through_journal(ap, journal).min(len), through_journal(hp, journal).min(len)))
                })
                .collect();
            if sels.is_empty() {
                continue;
            }
            let age = (self.now - pv.moved) as f32;
            let label = match cs.labels {
                CursorLabels::Always => 1.0,
                CursorLabels::Never => 0.0,
                CursorLabels::Activity => {
                    let hold = cs.label_seconds.max(0.0);
                    if age < hold {
                        wake = Some(wake.map_or(hold - age, |w: f32| w.min(hold - age)));
                        1.0
                    } else {
                        let fade = if self.settings.animations.enabled { 0.35 } else { 0.0 };
                        let k = if fade > 0.0 { 1.0 - (age - hold) / fade } else { 0.0 };
                        if k > 0.0 {
                            moving = true;
                        }
                        k.max(0.0)
                    }
                }
            };
            out.push(RemoteCaret { color: color32(pv.info.color), name: pv.info.name.clone(), sels, label });
        }
        if moving {
            self.tw.active = true;
        }
        if let Some(w) = wake {
            self.ctx.request_repaint_after(Duration::from_secs_f32(w + 0.01));
        }
        out
    }
}
