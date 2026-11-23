//! Ссылки-приглашения и адрес relay.
//!
//! * приглашение: `anvil://хост:порт/ТОКЕН?fp=ОТПЕЧАТОК`;
//! * адрес relay для настроек хозяина: `anvil://хост:порт?fp=ОТПЕЧАТОК&key=КЛЮЧ`.

use crate::tls::{Fingerprint, fp_hex, parse_fp};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub fp: Fingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayAddr {
    pub host: String,
    pub port: u16,
    pub fp: Fingerprint,
    /// Ключ для открытия сессий (пусто — relay открыт для всех).
    pub key: String,
}

/// Хост, порт, путь и параметры ссылки.
type Parts = (String, u16, String, Vec<(String, String)>);

/// Разбирает `[anvil://]хост:порт[/путь][?k=v&…]`.
fn split(s: &str) -> Result<Parts, String> {
    let s = s.trim().trim_matches(['"', '\'', '<', '>']);
    let s = s.strip_prefix("anvil://").or_else(|| s.strip_prefix("anvil:")).unwrap_or(s);
    let (main, query) = s.split_once('?').unwrap_or((s, ""));
    let (hostport, path) = main.split_once('/').unwrap_or((main, ""));
    let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
        // IPv6: [::1]:7878
        let (h, p) = rest.split_once("]:").ok_or("нет порта после адреса IPv6")?;
        (h.to_string(), p)
    } else {
        let (h, p) = hostport.rsplit_once(':').ok_or("нет порта: нужно хост:порт")?;
        (h.to_string(), p)
    };
    if host.is_empty() {
        return Err("пустой адрес".into());
    }
    let port: u16 = port.parse().map_err(|_| format!("неверный порт «{port}»"))?;
    let pairs = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_string(), v.to_string())
        })
        .collect();
    Ok((host, port, path.trim_end_matches('/').to_string(), pairs))
}

fn host_part(host: &str) -> String {
    if host.contains(':') { format!("[{host}]") } else { host.to_string() }
}

fn get_fp(pairs: &[(String, String)]) -> Result<Fingerprint, String> {
    let v = pairs.iter().find(|(k, _)| k == "fp").map(|(_, v)| v.as_str()).ok_or("в ссылке нет отпечатка сертификата (fp=…)")?;
    parse_fp(v).ok_or_else(|| "неверный отпечаток сертификата".to_string())
}

impl Invite {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (host, port, token, pairs) = split(s)?;
        if token.len() < 16 || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("в ссылке нет токена приглашения".into());
        }
        Ok(Self { host, port, token, fp: get_fp(&pairs)? })
    }

    pub fn link(&self) -> String {
        format!("anvil://{}:{}/{}?fp={}", host_part(&self.host), self.port, self.token, fp_hex(&self.fp))
    }
}

impl RelayAddr {
    pub fn parse(s: &str) -> Result<Self, String> {
        let (host, port, _, pairs) = split(s)?;
        let key = pairs.iter().find(|(k, _)| k == "key").map(|(_, v)| v.clone()).unwrap_or_default();
        Ok(Self { host, port, fp: get_fp(&pairs)?, key })
    }

    pub fn link(&self) -> String {
        let key = if self.key.is_empty() { String::new() } else { format!("&key={}", self.key) };
        format!("anvil://{}:{}?fp={}{key}", host_part(&self.host), self.port, fp_hex(&self.fp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_roundtrip() -> Result<(), String> {
        let inv = Invite { host: "192.168.1.5".into(), port: 7878, token: "0123456789abcdef0123456789abcdef".into(), fp: [7; 32] };
        let link = inv.link();
        assert!(link.starts_with("anvil://192.168.1.5:7878/0123"));
        assert_eq!(Invite::parse(&link)?, inv);
        assert_eq!(Invite::parse(&format!("  \"{link}\" "))?, inv);
        let v6 = Invite { host: "::1".into(), ..inv.clone() };
        assert_eq!(Invite::parse(&v6.link())?, v6);
        Ok(())
    }

    #[test]
    fn relay_roundtrip() -> Result<(), String> {
        let r = RelayAddr { host: "relay.example.com".into(), port: 443, fp: [1; 32], key: "secret".into() };
        assert_eq!(RelayAddr::parse(&r.link())?, r);
        let open = RelayAddr { key: String::new(), ..r };
        assert_eq!(RelayAddr::parse(&open.link())?, open);
        Ok(())
    }

    #[test]
    fn bad_links() {
        assert!(Invite::parse("anvil://host/abc").is_err());
        assert!(Invite::parse("anvil://host:77/short?fp=00").is_err());
        assert!(Invite::parse("anvil://host:77/0123456789abcdef0123").is_err());
        assert!(RelayAddr::parse("host:99999?fp=00").is_err());
    }
}
