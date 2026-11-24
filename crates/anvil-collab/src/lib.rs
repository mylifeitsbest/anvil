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

/// Адрес этой машины по маршруту «наружу» (пакеты не отправляются). На сервере это его
/// публичный адрес; на домашнем компьютере с VPN — адрес VPN, поэтому для ссылок в локальной
/// сети есть [`lan_ip`].
pub fn local_ip() -> Option<std::net::IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:53").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// Адрес этой машины в локальной сети — для ссылок встроенного relay.
/// Адреса VPN-адаптеров (Clash, sing-box и др. выдают себе 198.18.0.0/15) и виртуальных сетей
/// (WSL, VirtualBox, Docker) пропускаются: по ним соседи по сети не подключатся.
pub fn lan_ip() -> Option<std::net::IpAddr> {
    let route = local_ip();
    let list: Vec<(String, std::net::IpAddr)> = if_addrs::get_if_addrs().map(|v| v.into_iter().map(|i| (i.name.clone(), i.ip())).collect()).unwrap_or_default();
    pick_lan_ip(&list, route).or_else(|| route.filter(|ip| !is_vpn_fake(ip)))
}

/// 198.18.0.0/15 — диапазон для тестов сетей; его занимают TUN-адаптеры VPN-клиентов.
fn is_vpn_fake(ip: &std::net::IpAddr) -> bool {
    matches!(ip, std::net::IpAddr::V4(v) if v.octets()[0] == 198 && (v.octets()[1] & 0xfe) == 18)
}

fn virtual_adapter(name: &str) -> bool {
    let n = name.to_lowercase();
    [
        "vethernet",
        "virtualbox",
        "vmware",
        "hyper-v",
        "wsl",
        "docker",
        "br-",
        "veth",
        "tun",
        "tap",
        "wg",
        "utun",
        "meta",
        "clash",
        "mihomo",
        "sing",
        "tailscale",
        "zerotier",
        "radmin",
        "hamachi",
        "loopback",
        "vpn",
    ]
    .iter()
    .any(|k| n.contains(k))
}

/// Лучший адрес из списка интерфейсов (имя, адрес); `route` — адрес маршрута «наружу».
fn pick_lan_ip(list: &[(String, std::net::IpAddr)], route: Option<std::net::IpAddr>) -> Option<std::net::IpAddr> {
    list.iter()
        .filter_map(|(name, ip)| {
            let std::net::IpAddr::V4(v) = ip else { return None };
            if v.is_loopback() || v.is_link_local() || v.is_unspecified() || is_vpn_fake(ip) {
                return None;
            }
            let mut score = 0;
            if v.is_private() {
                score += 100;
            }
            if Some(*ip) == route {
                score += 30;
            }
            if virtual_adapter(name) {
                score -= 80;
            }
            Some((score, *ip))
        })
        .fold(None::<(i32, std::net::IpAddr)>, |best, cur| match best {
            Some(b) if b.0 >= cur.0 => Some(b),
            _ => Some(cur),
        })
        .map(|(_, ip)| ip)
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
    fn lan_ip_skips_vpn_and_virtual() {
        let ip = |s: &str| s.parse::<std::net::IpAddr>().ok();
        let l = |v: &[(&str, &str)]| -> Vec<(String, std::net::IpAddr)> { v.iter().filter_map(|(n, a)| Some((n.to_string(), ip(a)?))).collect() };
        // VPN (Clash Meta) забрал маршрут — берём Wi-Fi
        let list = l(&[("Meta", "198.18.0.1"), ("Wi-Fi", "192.168.0.109"), ("Loopback Pseudo-Interface 1", "127.0.0.1")]);
        assert_eq!(pick_lan_ip(&list, ip("198.18.0.1")), ip("192.168.0.109"));
        // WSL идёт первым, но это виртуальная сеть
        let list = l(&[("vEthernet (WSL)", "172.29.16.1"), ("Ethernet", "10.0.0.5")]);
        assert_eq!(pick_lan_ip(&list, ip("10.0.0.5")), ip("10.0.0.5"));
        // только VPN — адреса нет
        assert_eq!(pick_lan_ip(&l(&[("Meta", "198.19.0.1")]), None), None);
        assert!(is_vpn_fake(&std::net::IpAddr::from([198, 18, 0, 1])) && !is_vpn_fake(&std::net::IpAddr::from([198, 20, 0, 1])));
    }

    #[test]
    fn secrets() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
    }
}
