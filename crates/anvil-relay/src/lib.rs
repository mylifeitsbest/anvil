//! Relay-сервер совместной работы.
//!
//! Хозяин открывает комнату и получает два токена-приглашения: на редактирование и только на
//! чтение. Гости входят по токену. Relay пересылает сообщения внутри комнаты и проверяет права:
//! изменения документов — только от ролей с правом редактирования, список документов и их
//! начальное состояние — только от хозяина. Текст документов relay не хранит: комната живёт,
//! пока подключён хозяин.
//!
//! Каждое соединение — поток чтения и поток записи (без tokio); в простое потоки спят на
//! сокетах и не тратят процессор.

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anvil_collab::link::{self, Frame};
use anvil_collab::tls::{self, Fingerprint};
use anvil_collab::wire::{self, Auth, FromRelay, Invites, PROTOCOL, Peer, PeerId, Role, ToRelay};

/// Очередь исходящих кадров на участника; переполнилась — участник отключается.
const QUEUE: usize = 2048;
/// Сколько ждать приветствия после подключения.
const HELLO_WAIT: Duration = Duration::from_secs(10);
/// Тишина дольше этого — соединение считается потерянным (клиенты пингуют раз в 25 с).
const SILENCE: Duration = Duration::from_secs(90);
/// Предел одновременных соединений (в том числе ещё не вошедших).
const MAX_CONNECTIONS: usize = 1024;

/// Сертификат и ключ relay (DER, ключ — PKCS#8).
#[derive(Clone)]
pub struct Identity {
    pub cert: Vec<u8>,
    pub key: Vec<u8>,
}

impl Identity {
    /// Новый самоподписанный сертификат.
    pub fn generate() -> Result<Self, String> {
        let ck = rcgen::generate_simple_self_signed(vec!["anvil-relay".to_string()]).map_err(|e| format!("не удалось создать сертификат: {e}"))?;
        Ok(Self { cert: ck.cert.der().to_vec(), key: ck.signing_key.serialize_der() })
    }

    /// Взять сертификат из папки или создать и сохранить (тогда отпечаток не меняется между запусками).
    pub fn load_or_create(dir: &Path) -> Result<Self, String> {
        let (cp, kp) = (dir.join("cert.der"), dir.join("key.der"));
        if let (Ok(cert), Ok(key)) = (std::fs::read(&cp), std::fs::read(&kp))
            && tls::server_config(&cert, &key).is_ok()
        {
            return Ok(Self { cert, key });
        }
        let id = Self::generate()?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(&cp, &id.cert).map_err(|e| format!("{}: {e}", cp.display()))?;
        write_private(&kp, &id.key)?;
        Ok(id)
    }

    pub fn fingerprint(&self) -> Fingerprint {
        tls::fingerprint(&self.cert)
    }
}

/// Записать секрет так, чтобы его читал только владелец (на Unix — права 600).
pub fn write_private(path: &Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub type Log = Arc<dyn Fn(&str) + Send + Sync>;

pub struct Config {
    pub listen: SocketAddr,
    pub identity: Identity,
    /// Ключ для открытия сессий; `None` — любой может открыть комнату.
    pub key: Option<String>,
    pub max_rooms: usize,
    pub max_peers: usize,
    pub log: Option<Log>,
}

impl Config {
    pub fn new(listen: SocketAddr, identity: Identity) -> Self {
        Self { listen, identity, key: None, max_rooms: 256, max_peers: 32, log: None }
    }
}

struct Slot {
    info: Peer,
    tx: SyncSender<Frame>,
    /// Чтобы оборвать соединение медленного или выгнанного участника.
    sock: TcpStream,
}

struct Room {
    host: PeerId,
    peers: BTreeMap<PeerId, Slot>,
    tokens: [String; 2],
}

struct State {
    rooms: HashMap<u64, Room>,
    tokens: HashMap<String, (u64, Role)>,
    next_peer: PeerId,
    next_room: u64,
}

struct Shared {
    state: Mutex<State>,
    key: Option<String>,
    max_rooms: usize,
    max_peers: usize,
    tls: Arc<rustls::ServerConfig>,
    log: Option<Log>,
    connections: AtomicUsize,
}

impl Shared {
    fn log(&self, s: &str) {
        if let Some(l) = &self.log {
            l(s);
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Запущенный relay. Останавливается при удалении.
pub struct Relay {
    addr: SocketAddr,
    fp: Fingerprint,
    stop: Arc<AtomicBool>,
}

impl Relay {
    pub fn start(cfg: Config) -> io::Result<Relay> {
        let tls = tls::server_config(&cfg.identity.cert, &cfg.identity.key).map_err(|e| io::Error::other(format!("сертификат: {e}")))?;
        let listener = TcpListener::bind(cfg.listen)?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            state: Mutex::new(State { rooms: HashMap::new(), tokens: HashMap::new(), next_peer: 1, next_room: 1 }),
            key: cfg.key.filter(|k| !k.is_empty()),
            max_rooms: cfg.max_rooms.max(1),
            max_peers: cfg.max_peers.max(2),
            tls,
            log: cfg.log,
            connections: AtomicUsize::new(0),
        });
        let st = stop.clone();
        std::thread::Builder::new().name("anvil-relay-accept".into()).spawn(move || {
            for conn in listener.incoming() {
                if st.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(tcp) = conn else { continue };
                if shared.connections.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    continue;
                }
                shared.connections.fetch_add(1, Ordering::Relaxed);
                let sh = shared.clone();
                let spawned = std::thread::Builder::new().name("anvil-relay-peer".into()).spawn(move || {
                    let who = tcp.peer_addr().map(|a| a.to_string()).unwrap_or_default();
                    if let Err(e) = handle(tcp, &sh) {
                        sh.log(&format!("{who}: {e}"));
                    }
                    sh.connections.fetch_sub(1, Ordering::Relaxed);
                });
                if spawned.is_err() {
                    shared.connections.fetch_sub(1, Ordering::Relaxed);
                }
            }
        })?;
        Ok(Relay { addr, fp: cfg.identity.fingerprint(), stop })
    }

    /// Адрес, на котором relay слушает (порт известен и при `:0`).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn fingerprint(&self) -> Fingerprint {
        self.fp
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // разбудить accept
        let ip = match self.addr.ip() {
            IpAddr::V4(v) if v.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(v) if v.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            ip => ip,
        };
        let _ = TcpStream::connect_timeout(&SocketAddr::new(ip, self.addr.port()), Duration::from_millis(300));
    }
}

fn encode(msg: &FromRelay) -> Option<Frame> {
    wire::encode(msg).ok().map(Arc::new)
}

/// Убрать участника из комнаты; если это хозяин — закрыть комнату целиком.
fn remove_peer(sh: &Shared, st: &mut State, room_id: u64, peer: PeerId, why: &str) {
    let Some(room) = st.rooms.get_mut(&room_id) else { return };
    let Some(slot) = room.peers.remove(&peer) else { return };
    let _ = slot.sock.shutdown(std::net::Shutdown::Both);
    if peer == room.host {
        let bye = encode(&FromRelay::Closed { reason: "хозяин завершил сессию".into() });
        for (_, s) in std::mem::take(&mut room.peers) {
            if let Some(b) = &bye {
                let _ = s.tx.try_send(b.clone());
            }
            // очередь допишется, затем соединение закроется само
        }
        let tokens = room.tokens.clone();
        st.rooms.remove(&room_id);
        for t in tokens {
            st.tokens.remove(&t);
        }
        sh.log(&format!("комната {room_id} закрыта ({why})"));
    } else {
        sh.log(&format!("комната {room_id}: {} выходит ({why})", slot.info.name));
        if let Some(left) = encode(&FromRelay::Left(peer)) {
            broadcast(sh, st, room_id, None, &left);
        }
    }
}

/// Разослать кадр всем в комнате, кроме `except`. Тех, кто не успевает читать, — отключить.
fn broadcast(sh: &Shared, st: &mut State, room_id: u64, except: Option<PeerId>, frame: &Frame) {
    let Some(room) = st.rooms.get(&room_id) else { return };
    let slow: Vec<PeerId> =
        room.peers.iter().filter(|(id, _)| Some(**id) != except).filter(|(_, s)| s.tx.try_send(frame.clone()).is_err()).map(|(id, _)| *id).collect();
    for id in slow {
        remove_peer(sh, st, room_id, id, "не успевает получать данные");
    }
}

fn send_to(sh: &Shared, st: &mut State, room_id: u64, to: PeerId, frame: &Frame) {
    let full = st.rooms.get(&room_id).and_then(|r| r.peers.get(&to)).is_some_and(|s| s.tx.try_send(frame.clone()).is_err());
    if full {
        remove_peer(sh, st, room_id, to, "не успевает получать данные");
    }
}

fn clean(s: &str, max: usize) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(max).collect();
    let t = t.trim().to_string();
    if t.is_empty() { "Гость".into() } else { t }
}

fn handle(tcp: TcpStream, sh: &Arc<Shared>) -> Result<(), String> {
    tcp.set_read_timeout(Some(HELLO_WAIT)).map_err(|e| e.to_string())?;
    let sock = tcp.try_clone().map_err(|e| e.to_string())?;
    let conn = rustls::ServerConnection::new(sh.tls.clone()).map_err(|e| e.to_string())?;
    let (tx, rx) = mpsc::sync_channel::<Frame>(QUEUE);
    let mut reader = link::start(tcp, conn, rx, None).map_err(|e| e.to_string())?;

    // приветствие
    let body = reader.next_frame().map_err(|e| e.to_string())?;
    let Ok(ToRelay::Hello { protocol, name, color, auth }) = wire::decode::<ToRelay>(&body) else {
        return Err("первым должно идти приветствие".into());
    };
    let refuse = |reason: String| {
        if let Some(f) = encode(&FromRelay::Refused { reason: reason.clone() }) {
            let _ = tx.try_send(f);
        }
        reason
    };
    if protocol != PROTOCOL {
        return Err(refuse(format!("разные версии протокола (relay {PROTOCOL}, клиент {protocol}) — обновите Anvil")));
    }
    let name = clean(&name, 40);
    let color = color & 0xff_ffff;

    // вход
    let (me, room_id, role) = {
        let mut st = sh.lock();
        let id = st.next_peer;
        st.next_peer = st.next_peer.wrapping_add(1).max(1);
        match auth {
            Auth::Host { key } => {
                if let Some(k) = &sh.key
                    && !anvil_collab::same_secret(k, &key)
                {
                    drop(st);
                    return Err(refuse("неверный ключ relay — проверьте адрес в настройках [collab] relay".into()));
                }
                if st.rooms.len() >= sh.max_rooms {
                    drop(st);
                    return Err(refuse("на relay слишком много сессий".into()));
                }
                let (Some(edit), Some(read)) = (anvil_collab::random_hex::<16>(), anvil_collab::random_hex::<16>()) else {
                    drop(st);
                    return Err(refuse("на сервере недоступен генератор случайных чисел".into()));
                };
                let room_id = st.next_room;
                st.next_room += 1;
                let info = Peer { id, name: name.clone(), color, role: Role::Host };
                st.tokens.insert(edit.clone(), (room_id, Role::Edit));
                st.tokens.insert(read.clone(), (room_id, Role::Read));
                let welcome = encode(&FromRelay::Welcome {
                    you: id,
                    role: Role::Host,
                    peers: vec![info.clone()],
                    invites: Some(Invites { edit: edit.clone(), read: read.clone() }),
                });
                let mut peers = BTreeMap::new();
                peers.insert(id, Slot { info, tx: tx.clone(), sock });
                st.rooms.insert(room_id, Room { host: id, peers, tokens: [edit, read] });
                if let Some(w) = welcome {
                    send_to(sh, &mut st, room_id, id, &w);
                }
                sh.log(&format!("комната {room_id} открыта: {name}"));
                (id, room_id, Role::Host)
            }
            Auth::Join { token } => {
                let Some(&(room_id, role)) = st.tokens.get(token.trim()) else {
                    drop(st);
                    return Err(refuse("приглашение недействительно или сессия уже закончилась".into()));
                };
                let Some(room) = st.rooms.get_mut(&room_id) else {
                    drop(st);
                    return Err(refuse("сессия уже закончилась".into()));
                };
                if room.peers.len() >= sh.max_peers {
                    drop(st);
                    return Err(refuse("в сессии нет свободных мест".into()));
                }
                let info = Peer { id, name: name.clone(), color, role };
                room.peers.insert(id, Slot { info: info.clone(), tx: tx.clone(), sock });
                let peers: Vec<Peer> = room.peers.values().map(|s| s.info.clone()).collect();
                if let Some(j) = encode(&FromRelay::Joined(info)) {
                    broadcast(sh, &mut st, room_id, Some(id), &j);
                }
                if let Some(w) = encode(&FromRelay::Welcome { you: id, role, peers, invites: None }) {
                    send_to(sh, &mut st, room_id, id, &w);
                }
                sh.log(&format!("комната {room_id}: {name} входит ({})", role.label()));
                (id, room_id, role)
            }
        }
    };
    // дальше отправка — только через очередь в комнате: если участника уберут, канал закроется
    drop(tx);
    let _ = reader.set_timeout(Some(SILENCE));

    let why = loop {
        let body = match reader.next_frame() {
            Ok(b) => b,
            Err(e) => break e.to_string(),
        };
        let msg = match wire::decode::<ToRelay>(&body) {
            Ok(m) => m,
            Err(e) => break e.to_string(),
        };
        let mut st = sh.lock();
        if !st.rooms.get(&room_id).is_some_and(|r| r.peers.contains_key(&me)) {
            break "отключён".into();
        }
        match msg {
            ToRelay::Ping => {
                if let Some(p) = encode(&FromRelay::Pong) {
                    send_to(sh, &mut st, room_id, me, &p);
                }
            }
            ToRelay::Hello { .. } => {}
            ToRelay::Send { to, payload } => {
                if !payload.allowed(role) {
                    // гость «только для чтения» не может менять документы — молча отбрасываем
                    continue;
                }
                let Some(frame) = encode(&FromRelay::From { peer: me, payload }) else { continue };
                match to {
                    Some(t) if t != me => send_to(sh, &mut st, room_id, t, &frame),
                    Some(_) => {}
                    None => broadcast(sh, &mut st, room_id, Some(me), &frame),
                }
            }
        }
    };
    let mut st = sh.lock();
    remove_peer(sh, &mut st, room_id, me, &why);
    drop(st);
    reader.shutdown();
    Ok(())
}
