//! TLS-канал поверх TCP на двух потоках.
//!
//! Читающий поток блокируется на сокете (в простое — ноль пробуждений), пишущий — на канале
//! исходящих кадров. Состояние TLS общее и под замком; шифрование и запись в сокет идут под
//! вторым замком `out`, поэтому TLS-записи уходят строго в порядке шифрования, а чтение никогда
//! не ждёт медленную запись (сокет пишется без замка соединения).

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::wire::{FrameReader, WireError};

/// Готовый кадр (с длиной) — разделяется между получателями без копирования.
pub type Frame = Arc<Vec<u8>>;

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("соединение закрыто")]
    Closed,
    #[error("нет ответа — соединение потеряно")]
    Timeout,
    #[error("сеть: {0}")]
    Io(#[from] io::Error),
    #[error("{}", tls_text(.0))]
    Tls(#[from] rustls::Error),
    #[error(transparent)]
    Wire(#[from] WireError),
}

fn tls_text(e: &rustls::Error) -> String {
    match e {
        rustls::Error::InvalidCertificate(_) => "сертификат relay не совпадает с отпечатком из ссылки — соединение прервано".into(),
        other => format!("TLS: {other}"),
    }
}

struct Shared {
    conn: Mutex<rustls::Connection>,
    out: Mutex<TcpStream>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Отправить в сокет всё, что TLS хочет записать (рукопожатие, оповещения).
fn flush(sh: &Shared) -> io::Result<()> {
    let mut out = lock(&sh.out);
    let mut bytes = Vec::new();
    {
        let mut c = lock(&sh.conn);
        while c.wants_write() {
            c.write_tls(&mut bytes)?;
        }
    }
    if !bytes.is_empty() {
        out.write_all(&bytes)?;
        out.flush()?;
    }
    Ok(())
}

/// Зашифровать и отправить кадры.
fn send(sh: &Shared, frames: &[Frame]) -> io::Result<()> {
    let mut out = lock(&sh.out);
    let mut bytes = Vec::with_capacity(frames.iter().map(|f| f.len() + 64).sum());
    {
        let mut c = lock(&sh.conn);
        for f in frames {
            c.writer().write_all(f)?;
        }
        while c.wants_write() {
            c.write_tls(&mut bytes)?;
        }
    }
    if !bytes.is_empty() {
        out.write_all(&bytes)?;
        out.flush()?;
    }
    Ok(())
}

fn writer(sh: Arc<Shared>, rx: Receiver<Frame>, ping: Option<(Duration, Frame)>) {
    let wait = ping.as_ref().map(|p| p.0).unwrap_or(Duration::from_secs(3600));
    loop {
        let first = match rx.recv_timeout(wait) {
            Ok(f) => f,
            Err(RecvTimeoutError::Timeout) => match &ping {
                Some((_, p)) => p.clone(),
                None => continue,
            },
            Err(RecvTimeoutError::Disconnected) => break,
        };
        // всё, что уже накопилось, — одной записью
        let mut batch = vec![first];
        while batch.len() < 256 {
            match rx.try_recv() {
                Ok(f) => batch.push(f),
                Err(_) => break,
            }
        }
        if send(&sh, &batch).is_err() {
            break;
        }
    }
    lock(&sh.conn).send_close_notify();
    let _ = flush(&sh);
    let _ = lock(&sh.out).shutdown(Shutdown::Both);
}

/// Читающая сторона канала.
pub struct Reader {
    shared: Arc<Shared>,
    tcp: TcpStream,
    frames: FrameReader,
    buf: Box<[u8]>,
    eof: bool,
}

/// Запустить канал: пишущий поток стартует сразу и берёт кадры из `outgoing`
/// (если долго нечего слать — отправляет `ping`). Чтение — через [`Reader::next_frame`].
/// Канал закрывается, когда все отправители `outgoing` исчезли или сокет оборвался.
pub fn start(tcp: TcpStream, conn: impl Into<rustls::Connection>, outgoing: Receiver<Frame>, ping: Option<(Duration, Frame)>) -> io::Result<Reader> {
    tcp.set_nodelay(true)?;
    let mut conn: rustls::Connection = conn.into();
    // без ограничения: большие кадры (начальное состояние файла) шифруются целиком
    conn.set_buffer_limit(None);
    let shared = Arc::new(Shared { conn: Mutex::new(conn), out: Mutex::new(tcp.try_clone()?) });
    // клиент начинает рукопожатие сам
    flush(&shared)?;
    let w = shared.clone();
    std::thread::Builder::new().name("anvil-link".into()).spawn(move || writer(w, outgoing, ping))?;
    Ok(Reader { shared, tcp, frames: FrameReader::default(), buf: vec![0u8; 64 * 1024].into_boxed_slice(), eof: false })
}

impl Reader {
    /// Следующий кадр (тело без длины). Блокируется до прихода данных или тайм-аута сокета.
    pub fn next_frame(&mut self) -> Result<Vec<u8>, LinkError> {
        loop {
            if let Some(f) = self.frames.next_frame()? {
                return Ok(f);
            }
            if self.eof {
                return Err(LinkError::Closed);
            }
            let n = match self.tcp.read(&mut self.buf) {
                Ok(0) => return Err(LinkError::Closed),
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => return Err(LinkError::Timeout),
                Err(e) => return Err(LinkError::Io(e)),
            };
            let mut plain = Vec::new();
            let res = self.absorb(n, &mut plain);
            // ответы рукопожатия и оповещения об ошибках — даже если разбор не удался
            let fl = flush(&self.shared);
            res?;
            fl?;
            self.frames.push(&plain);
        }
    }

    fn absorb(&mut self, n: usize, plain: &mut Vec<u8>) -> Result<(), LinkError> {
        let mut c = lock(&self.shared.conn);
        let mut src = &self.buf[..n];
        while !src.is_empty() {
            c.read_tls(&mut src)?;
            let st = c.process_new_packets()?;
            let k = st.plaintext_bytes_to_read();
            if k > 0 {
                let at = plain.len();
                plain.resize(at + k, 0);
                c.reader().read_exact(&mut plain[at..])?;
            }
            if st.peer_has_closed() {
                self.eof = true;
            }
        }
        Ok(())
    }

    /// Тайм-аут ожидания данных (`None` — ждать сколько угодно).
    pub fn set_timeout(&self, t: Option<Duration>) -> io::Result<()> {
        self.tcp.set_read_timeout(t)
    }

    /// Оборвать соединение (пишущий поток завершится на следующей записи).
    pub fn shutdown(&self) {
        let _ = self.tcp.shutdown(Shutdown::Both);
    }
}
