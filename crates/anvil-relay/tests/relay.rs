//! Сквозной тест: настоящий relay на 127.0.0.1, настоящие TLS-клиенты.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anvil_collab::{Auth, Client, Event, FromRelay, Hello, Payload, Role, SharedText};
use anvil_relay::{Config, Identity, Relay};

type R<T> = Result<T, String>;

fn relay(key: Option<&str>) -> R<Relay> {
    let mut cfg = Config::new(SocketAddr::from(([127, 0, 0, 1], 0)), Identity::generate()?);
    cfg.key = key.map(str::to_string);
    Relay::start(cfg).map_err(|e| e.to_string())
}

fn client(r: &Relay, name: &str, auth: Auth) -> Client {
    client_fp(r, name, auth, r.fingerprint())
}

fn client_fp(r: &Relay, name: &str, auth: Auth, fp: [u8; 32]) -> Client {
    Client::start("127.0.0.1".into(), r.addr().port(), fp, Hello { name: name.into(), color: 0x3cc8ff, auth }, Arc::new(|| {}))
}

/// Ждать событие, подходящее под условие (остальные пропускаются).
fn wait<T>(c: &Client, what: &str, mut f: impl FnMut(&Event) -> Option<T>) -> R<T> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match c.try_recv() {
            Some(ev) => {
                if let Some(v) = f(&ev) {
                    return Ok(v);
                }
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    Err(format!("не дождались: {what}"))
}

fn welcome(c: &Client) -> R<(u32, Role, Option<anvil_collab::Invites>)> {
    wait(c, "Welcome", |e| match e {
        Event::Msg(FromRelay::Welcome { you, role, invites, .. }) => Some(Ok((*you, *role, invites.clone()))),
        Event::Msg(FromRelay::Refused { reason }) => Some(Err(reason.clone())),
        Event::Closed(r) => Some(Err(r.clone())),
        _ => None,
    })?
}

fn update_of(e: &Event) -> Option<Vec<u8>> {
    match e {
        Event::Msg(FromRelay::From { payload: Payload::Update { update, .. } | Payload::SyncState { update, .. }, .. }) => Some(update.clone()),
        _ => None,
    }
}

#[test]
fn session_with_editor_and_viewer() -> R<()> {
    let r = relay(Some("k3y-0123456789abcdef"))?;
    let host = client(&r, "Хозяин", Auth::Host { key: "k3y-0123456789abcdef".into() });
    let (host_id, role, invites) = welcome(&host)?;
    assert_eq!(role, Role::Host);
    let inv = invites.ok_or("хозяин не получил приглашения")?;

    let editor = client(&r, "Алиса", Auth::Join { token: inv.edit.clone() });
    let (alice, role, none) = welcome(&editor)?;
    assert_eq!(role, Role::Edit);
    assert!(none.is_none(), "гость не должен видеть токены");
    let joined = wait(&host, "Joined", |e| match e {
        Event::Msg(FromRelay::Joined(p)) => Some(p.clone()),
        _ => None,
    })?;
    assert_eq!((joined.id, joined.name.as_str(), joined.role), (alice, "Алиса", Role::Edit));

    let viewer = client(&r, "Борис", Auth::Join { token: inv.read.clone() });
    let (boris, role, _) = welcome(&viewer)?;
    assert_eq!(role, Role::Read);

    // хозяин делится документом; гость запрашивает состояние и получает текст
    let mut doc_h = SharedText::new(1, "fn main() {}\n");
    let mut doc_a = SharedText::new(2, "");
    host.send(None, Payload::Docs(vec![anvil_collab::DocInfo { id: 7, path: "src/main.rs".into() }]));
    wait(&editor, "Docs", |e| matches!(e, Event::Msg(FromRelay::From { payload: Payload::Docs(_), .. })).then_some(()))?;
    editor.send(Some(host_id), Payload::SyncRequest { doc: 7, sv: doc_a.state_vector() });
    let (from, sv) = wait(&host, "SyncRequest", |e| match e {
        Event::Msg(FromRelay::From { peer, payload: Payload::SyncRequest { sv, .. } }) => Some((*peer, sv.clone())),
        _ => None,
    })?;
    assert_eq!(from, alice);
    host.send(Some(from), Payload::SyncState { doc: 7, update: doc_h.diff(&sv).map_err(|e| e.to_string())? });
    let u = wait(&editor, "SyncState", update_of)?;
    doc_a.apply_remote(&u).map_err(|e| e.to_string())?;
    assert_eq!(doc_a.text(), "fn main() {}\n");

    // Алиса правит — хозяин видит
    let u = doc_a.apply_local(&[anvil_collab::Edit { start: 11, remove: 0, insert: " 42 ".into() }]).ok_or("нет обновления")?;
    editor.send(None, Payload::Update { doc: 7, update: u });
    let u = wait(&host, "Update от Алисы", update_of)?;
    doc_h.apply_remote(&u).map_err(|e| e.to_string())?;
    assert_eq!(doc_h.text(), "fn main() { 42 }\n");

    // Борис (только чтение) пытается править — relay не пропускает, а курсор пропускает
    viewer.send(None, Payload::Update { doc: 7, update: vec![0, 0] });
    viewer.send(None, Payload::Cursor { doc: Some(7), sel: vec![] });
    let got = wait(&host, "Cursor от Бориса", |e| match e {
        Event::Msg(FromRelay::From { peer, payload }) => Some((*peer, payload.clone())),
        _ => None,
    })?;
    assert_eq!(got.0, boris);
    assert!(matches!(got.1, Payload::Cursor { .. }), "первым должен прийти курсор, а не правка: {:?}", got.1);

    // Борис уходит — остальные узнают
    drop(viewer);
    let left = wait(&host, "Left", |e| match e {
        Event::Msg(FromRelay::Left(id)) => Some(*id),
        _ => None,
    })?;
    assert_eq!(left, boris);

    // хозяин уходит — сессия закрывается у гостей, приглашение больше не работает
    drop(host);
    wait(&editor, "Closed", |e| matches!(e, Event::Msg(FromRelay::Closed { .. })).then_some(()))?;
    let late = client(&r, "Поздний", Auth::Join { token: inv.edit });
    assert!(welcome(&late).is_err());
    Ok(())
}

#[test]
fn wrong_key_and_token_are_refused() -> R<()> {
    let r = relay(Some("right-key-0123456789"))?;
    let h = client(&r, "x", Auth::Host { key: "wrong".into() });
    let e = welcome(&h).err().ok_or("пустили с неверным ключом")?;
    assert!(e.contains("ключ"), "{e}");
    let g = client(&r, "y", Auth::Join { token: "00000000000000000000000000000000".into() });
    let e = welcome(&g).err().ok_or("пустили с выдуманным токеном")?;
    assert!(e.contains("недействительно"), "{e}");
    Ok(())
}

#[test]
fn wrong_fingerprint_is_rejected() -> R<()> {
    let r = relay(None)?;
    let c = client_fp(&r, "z", Auth::Host { key: String::new() }, [9; 32]);
    let e = welcome(&c).err().ok_or("подключились к relay с чужим сертификатом")?;
    assert!(e.contains("отпечат"), "{e}");
    Ok(())
}

#[test]
fn open_relay_accepts_any_host() -> R<()> {
    let r = relay(None)?;
    let h = client(&r, "x", Auth::Host { key: "что угодно".into() });
    assert_eq!(welcome(&h)?.1, Role::Host);
    Ok(())
}
