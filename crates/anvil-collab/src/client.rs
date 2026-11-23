//! Подключение к relay в фоне. Интерфейс не ждёт сеть: события приходят в канал,
//! а `wake` будит окно, когда есть что показать.

use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::link::{self, Frame, LinkError};
use crate::tls::{self, Fingerprint};
use crate::wire::{self, Auth, FromRelay, PROTOCOL, Payload, PeerId, ToRelay};

/// Как часто напоминать relay, что мы на связи.
const PING: Duration = Duration::from_secs(25);
/// Сколько ждать хоть каких-то данных, прежде чем считать соединение потерянным.
const SILENCE: Duration = Duration::from_secs(80);

pub struct Hello {
    pub name: String,
    pub color: u32,
    pub auth: Auth,
}

#[derive(Debug)]
pub enum Event {
    Msg(FromRelay),
    /// Соединение закрыто (причина — для человека).
    Closed(String),
}

pub struct Client {
    tx: Sender<Frame>,
    rx: Receiver<Event>,
}

impl Client {
    /// Начать подключение. Возвращается сразу; приветствие уходит первым.
    pub fn start(host: String, port: u16, fp: Fingerprint, hello: Hello, wake: Arc<dyn Fn() + Send + Sync>) -> Client {
        let (tx, out_rx) = mpsc::channel::<Frame>();
        let (ev_tx, rx) = mpsc::channel::<Event>();
        let first = ToRelay::Hello { protocol: PROTOCOL, name: hello.name, color: hello.color, auth: hello.auth };
        match wire::encode(&first) {
            Ok(f) => {
                let _ = tx.send(Arc::new(f));
            }
            Err(e) => {
                let _ = ev_tx.send(Event::Closed(e.to_string()));
            }
        }
        let spawned = std::thread::Builder::new().name("anvil-collab".into()).spawn(move || {
            let reason = match run(&host, port, fp, out_rx, &ev_tx, &wake) {
                Ok(()) | Err(LinkError::Closed) => "соединение с relay закрыто".to_string(),
                Err(e) => e.to_string(),
            };
            let _ = ev_tx.send(Event::Closed(reason));
            wake();
        });
        let (tx, rx) = match spawned {
            Ok(_) => (tx, rx),
            Err(e) => {
                let (ev_tx, rx) = mpsc::channel();
                let _ = ev_tx.send(Event::Closed(format!("не удалось запустить поток: {e}")));
                (tx, rx)
            }
        };
        Client { tx, rx }
    }

    /// Отправить полезную нагрузку участнику или всем.
    pub fn send(&self, to: Option<PeerId>, payload: Payload) -> bool {
        match wire::encode(&ToRelay::Send { to, payload }) {
            Ok(f) => self.tx.send(Arc::new(f)).is_ok(),
            Err(_) => false,
        }
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }
}

fn run(host: &str, port: u16, fp: Fingerprint, out_rx: Receiver<Frame>, ev_tx: &Sender<Event>, wake: &Arc<dyn Fn() + Send + Sync>) -> Result<(), LinkError> {
    let addrs: Vec<_> = (host, port).to_socket_addrs()?.collect();
    let mut last = None;
    let mut tcp = None;
    for a in &addrs {
        match TcpStream::connect_timeout(a, Duration::from_secs(6)) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let tcp = match tcp {
        Some(t) => t,
        None => {
            return Err(LinkError::Io(last.unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, format!("адрес {host} не найден")))));
        }
    };
    tcp.set_read_timeout(Some(SILENCE))?;
    let name = tls::server_name().map_err(|e| LinkError::Io(std::io::Error::other(e.to_string())))?;
    let conn = rustls::ClientConnection::new(tls::client_config(fp)?, name)?;
    let ping = Arc::new(wire::encode(&ToRelay::Ping)?);
    let mut reader = link::start(tcp, conn, out_rx, Some((PING, ping)))?;
    let res = loop {
        let body = match reader.next_frame() {
            Ok(b) => b,
            Err(e) => break Err(e),
        };
        let msg: FromRelay = match wire::decode(&body) {
            Ok(m) => m,
            Err(e) => break Err(e.into()),
        };
        if msg == FromRelay::Pong {
            continue;
        }
        if ev_tx.send(Event::Msg(msg)).is_err() {
            // клиент больше не нужен
            break Ok(());
        }
        wake();
    };
    reader.shutdown();
    res
}
