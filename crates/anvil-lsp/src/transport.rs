//! Кадры JSON-RPC: заголовок `Content-Length` и тело JSON.

use std::io::{self, BufRead, Write};

use serde_json::Value;

pub fn write_message(w: &mut impl Write, v: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(v).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write!(w, "Content-Length: {}\r\n\r\n", body.len())?;
    w.write_all(&body)?;
    w.flush()
}

/// Читает одно сообщение; `Ok(None)` — поток закрыт.
pub fn read_message(r: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut len: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let l = line.trim_end();
        if l.is_empty() {
            if len.is_some() {
                break;
            }
            continue;
        }
        if let Some((k, v)) = l.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            len = v.trim().parse().ok();
        }
    }
    let len = len.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "нет Content-Length"))?;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    serde_json::from_slice(&buf).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn roundtrip_several_messages() -> io::Result<()> {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({"jsonrpc": "2.0", "id": 1, "method": "привет"}))?;
        write_message(&mut buf, &json!({"jsonrpc": "2.0", "method": "x", "params": [1, 2]}))?;
        let mut r = io::BufReader::new(&buf[..]);
        let a = read_message(&mut r)?;
        let b = read_message(&mut r)?;
        assert_eq!(a.and_then(|v| v["method"].as_str().map(String::from)).as_deref(), Some("привет"));
        assert_eq!(b.map(|v| v["params"][1].clone()), Some(json!(2)));
        assert!(read_message(&mut r)?.is_none());
        Ok(())
    }

    #[test]
    fn tolerates_extra_headers() -> io::Result<()> {
        let raw = b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}";
        let mut r = io::BufReader::new(&raw[..]);
        assert_eq!(read_message(&mut r)?, Some(json!({})));
        Ok(())
    }
}
