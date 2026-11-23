//! Протокол совместной работы.
//!
//! Кадр — `[длина: u32 LE][сообщение postcard]`. Клиент шлёт relay [`ToRelay`], relay отвечает
//! [`FromRelay`]. Полезная нагрузка между участниками — [`Payload`]: relay её не хранит и не
//! понимает текст документов, но проверяет права: изменения принимает только от тех, у кого
//! роль позволяет редактировать, а список документов и их начальное состояние — только от хозяина.

use serde::{Deserialize, Serialize};

/// Версия протокола: при несовпадении relay вежливо отказывает.
pub const PROTOCOL: u16 = 1;
/// Наибольший кадр (начальное состояние большого файла).
pub const MAX_FRAME: usize = 64 * 1024 * 1024;

pub type PeerId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Открыл сессию: его файлы, его диск.
    Host,
    /// Может править.
    Edit,
    /// Только смотрит.
    Read,
}

impl Role {
    pub fn can_edit(self) -> bool {
        !matches!(self, Role::Read)
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Host => "хозяин сессии",
            Role::Edit => "редактирование",
            Role::Read => "только чтение",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Peer {
    pub id: PeerId,
    pub name: String,
    /// Цвет курсора `0xRRGGBB`.
    pub color: u32,
    pub role: Role,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Auth {
    /// Открыть сессию (ключ relay, если он задан).
    Host { key: String },
    /// Войти по приглашению.
    Join { token: String },
}

/// Токены приглашений — их видит только хозяин.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invites {
    pub edit: String,
    pub read: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToRelay {
    /// Первое сообщение соединения.
    Hello {
        protocol: u16,
        name: String,
        color: u32,
        auth: Auth,
    },
    /// Переслать участнику (`Some`) или всем остальным (`None`).
    Send {
        to: Option<PeerId>,
        payload: Payload,
    },
    Ping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FromRelay {
    Welcome {
        you: PeerId,
        role: Role,
        peers: Vec<Peer>,
        invites: Option<Invites>,
    },
    Refused {
        reason: String,
    },
    Joined(Peer),
    Left(PeerId),
    From {
        peer: PeerId,
        payload: Payload,
    },
    /// Сессия закончилась (хозяин вышел).
    Closed {
        reason: String,
    },
    Pong,
}

/// Документ, которым поделился хозяин.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocInfo {
    pub id: u64,
    /// Путь относительно корня проекта, через `/`.
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Payload {
    /// Хозяин → все: полный список общих документов.
    Docs(Vec<DocInfo>),
    /// Попросить у хозяина состояние документа (`sv` — что уже есть).
    SyncRequest { doc: u64, sv: Vec<u8> },
    /// Хозяин → спросившему: недостающая часть документа.
    SyncState { doc: u64, update: Vec<u8> },
    /// Изменение документа (обновление CRDT).
    Update { doc: u64, update: Vec<u8> },
    /// Где курсоры отправителя: документ и выделения (якорь, голова) в «липких» позициях.
    /// `doc: None` — участник смотрит не в общий документ.
    Cursor { doc: Option<u64>, sel: Vec<(Vec<u8>, Vec<u8>)> },
    /// Попросить хозяина сохранить файл на диск.
    Save { doc: u64 },
}

impl Payload {
    /// Может ли участник с ролью `role` это отправить.
    pub fn allowed(&self, role: Role) -> bool {
        match self {
            Payload::Docs(_) | Payload::SyncState { .. } => role == Role::Host,
            Payload::Update { .. } | Payload::Save { .. } => role.can_edit(),
            Payload::SyncRequest { .. } | Payload::Cursor { .. } => true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("кадр {0} байт больше допустимого")]
    TooLarge(usize),
    #[error("повреждённое сообщение: {0}")]
    Decode(#[from] postcard::Error),
}

/// Закодировать сообщение в кадр с длиной.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, WireError> {
    let body = postcard::to_allocvec(msg)?;
    if body.len() > MAX_FRAME {
        return Err(WireError::TooLarge(body.len()));
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Разобрать тело кадра (без длины).
pub fn decode<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T, WireError> {
    Ok(postcard::from_bytes(body)?)
}

/// Собирает кадры из потока байтов.
#[derive(Debug, Default)]
pub struct FrameReader {
    buf: Vec<u8>,
    pos: usize,
}

impl FrameReader {
    pub fn push(&mut self, data: &[u8]) {
        if self.pos > 0 && self.pos * 2 >= self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.extend_from_slice(data);
    }

    /// Следующий целый кадр, если он уже пришёл.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, WireError> {
        let rest = &self.buf[self.pos..];
        let Some(head) = rest.get(..4) else { return Ok(None) };
        let len = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
        if len > MAX_FRAME {
            return Err(WireError::TooLarge(len));
        }
        let Some(body) = rest.get(4..4 + len) else { return Ok(None) };
        let body = body.to_vec();
        self.pos += 4 + len;
        Ok(Some(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_through_stream() -> Result<(), WireError> {
        let msgs = vec![
            ToRelay::Hello { protocol: PROTOCOL, name: "Алиса".into(), color: 0xff5fa2, auth: Auth::Join { token: "t".into() } },
            ToRelay::Send { to: Some(3), payload: Payload::Update { doc: 7, update: vec![1, 2, 3] } },
            ToRelay::Send { to: None, payload: Payload::Cursor { doc: Some(7), sel: vec![(vec![9], vec![8, 7])] } },
            ToRelay::Ping,
        ];
        let mut stream = Vec::new();
        for m in &msgs {
            stream.extend(encode(m)?);
        }
        // кусками по 3 байта — как из сети
        let mut r = FrameReader::default();
        let mut got = Vec::new();
        for chunk in stream.chunks(3) {
            r.push(chunk);
            while let Some(f) = r.next_frame()? {
                got.push(decode::<ToRelay>(&f)?);
            }
        }
        assert_eq!(got, msgs);
        Ok(())
    }

    #[test]
    fn rejects_huge_frames() {
        let mut r = FrameReader::default();
        r.push(&(u32::MAX).to_le_bytes());
        assert!(matches!(r.next_frame(), Err(WireError::TooLarge(_))));
    }

    #[test]
    fn permissions() {
        let upd = Payload::Update { doc: 1, update: vec![] };
        assert!(upd.allowed(Role::Host) && upd.allowed(Role::Edit) && !upd.allowed(Role::Read));
        let docs = Payload::Docs(vec![]);
        assert!(docs.allowed(Role::Host) && !docs.allowed(Role::Edit));
        let state = Payload::SyncState { doc: 1, update: vec![] };
        assert!(!state.allowed(Role::Edit) && !state.allowed(Role::Read));
        let cur = Payload::Cursor { doc: None, sel: vec![] };
        assert!(cur.allowed(Role::Read));
        assert!(!Payload::Save { doc: 1 }.allowed(Role::Read));
    }
}
