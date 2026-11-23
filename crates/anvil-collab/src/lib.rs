//! Совместная работа в Anvil.
//!
//! * [`wire`] — протокол: кадры `[длина u32][postcard]`, сообщения клиент ↔ relay ↔ участники;
//! * [`link`] — TLS-канал поверх TCP на двух потоках (без tokio и без опроса в простое);
//! * [`tls`] — TLS 1.3 с привязкой к отпечатку сертификата relay (самоподписанный сертификат
//!   не требует удостоверяющего центра: отпечаток приходит вместе со ссылкой-приглашением);
//! * [`invite`] — ссылки `anvil://хост:порт/токен?fp=…` и адрес relay для настроек;
//! * [`doc`] — общий текст на CRDT (`yrs`): локальные правки → обновления, чужие обновления →
//!   правки в символах, своя отмена (не трогает чужие правки), «липкие» позиции курсоров;
//! * [`client`] — подключение к relay в фоне, события приходят в канал.

pub mod client;
pub mod doc;
pub mod invite;
pub mod link;
pub mod tls;
pub mod wire;

pub use client::{Client, Event, Hello};
pub use doc::{Edit, SharedText};
pub use invite::{Invite, RelayAddr};
pub use tls::{Fingerprint, fingerprint};
pub use wire::{Auth, DocInfo, FromRelay, Invites, Payload, Peer, PeerId, Role, ToRelay};

/// Случайные байты из системного криптографического генератора
/// (`None` — генератор недоступен; секреты тогда не создаются вовсе).
pub fn random_bytes<const N: usize>() -> Option<[u8; N]> {
    use ring::rand::SecureRandom;
    let mut b = [0u8; N];
    ring::rand::SystemRandom::new().fill(&mut b).ok()?;
    Some(b)
}

/// Случайная строка из `N` байт в шестнадцатеричном виде (токены, ключи).
pub fn random_hex<const N: usize>() -> Option<String> {
    random_bytes::<N>().map(|b| hex(&b))
}

/// Адрес этой машины в локальной сети (по маршруту «наружу»; пакеты не отправляются).
pub fn local_ip() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:53").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| *c != ':').collect();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Сравнение секретов за постоянное время.
pub fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let b = random_bytes::<16>().unwrap_or_default();
        assert_eq!(unhex(&hex(&b)).as_deref(), Some(&b[..]));
        assert_eq!(unhex("ab:CD"), Some(vec![0xab, 0xcd]));
        assert_eq!(unhex("abc"), None);
        assert_ne!(random_hex::<16>(), random_hex::<16>());
    }

    #[test]
    fn secrets() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
    }
}
