//! `file://` URI ↔ путь (Windows-диски, пробелы, кириллица).

use std::path::{Path, PathBuf};

fn keep(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':')
}

pub fn from_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s).to_string();
    let s = if s.starts_with('/') { s } else { format!("/{s}") };
    let mut out = String::from("file://");
    for b in s.bytes() {
        if keep(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |x: u8| (x as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub fn to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = decode(rest);
    // file:///C:/x → C:/x
    let bytes = path.as_bytes();
    let path = if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() { path[1..].to_string() } else { path };
    Some(PathBuf::from(if cfg!(windows) { path.replace('/', "\\") } else { path }))
}

/// Ключ для сравнения путей из разных источников (регистр диска, разделители, %3A).
pub fn key(uri: &str) -> String {
    match to_path(uri) {
        Some(p) => key_path(&p),
        None => uri.to_string(),
    }
}

pub fn key_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_keys() {
        let p = if cfg!(windows) { PathBuf::from(r"C:\Мои проекты\a#b.rs") } else { PathBuf::from("/home/u/Мои проекты/a#b.rs") };
        let u = from_path(&p);
        assert!(u.starts_with("file:///"));
        assert!(!u.contains(' ') && !u.contains('#'));
        assert_eq!(to_path(&u), Some(p.clone()));
        if cfg!(windows) {
            // rust-analyzer присылает диск в нижнем регистре и с %3A
            assert_eq!(key("file:///c%3A/%D0%9C%D0%BE%D0%B8%20%D0%BF%D1%80%D0%BE%D0%B5%D0%BA%D1%82%D1%8B/a%23b.rs"), key_path(&p));
        }
    }
}
