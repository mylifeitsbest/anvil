//! `anvil-relay` — сервер для совместной работы в Anvil через интернет.
//!
//! Запустите на машине с публичным адресом (VDS) и вставьте напечатанный адрес в Anvil
//! хозяина сессии: команда «Совместная работа: свой relay-сервер…» (или `[collab] relay = "…"`).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anvil_collab::RelayAddr;
use anvil_relay::{Config, Identity, Relay};

const HELP: &str = "anvil-relay — сервер совместной работы Anvil

anvil-relay [ОПЦИИ]

  --listen АДРЕС:ПОРТ  где слушать (по умолчанию 0.0.0.0:7878)
  --public ХОСТ        адрес или домен для ссылок (по умолчанию — найденный IP)
  --data ПАПКА         где хранить сертификат и ключ доступа
  --open               не требовать ключ: открыть сессию сможет любой
  --new-key            выпустить новый ключ доступа (старые адреса перестанут работать)
  --max-rooms N        предел одновременных сессий (256)
  --max-peers N        предел участников в сессии (32)
  --quiet              не печатать входы и выходы
  -V, --version        версия";

fn main() {
    if let Err(e) = run() {
        eprintln!("anvil-relay: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut listen: SocketAddr = SocketAddr::from(([0, 0, 0, 0], 7878));
    let mut public: Option<String> = None;
    let mut data: Option<PathBuf> = None;
    let (mut open, mut new_key, mut quiet) = (false, false, false);
    let (mut max_rooms, mut max_peers) = (256usize, 32usize);
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name}: нужно значение"));
        match a.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("anvil-relay {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--listen" => listen = val("--listen")?.parse().map_err(|_| "--listen: нужно АДРЕС:ПОРТ, например 0.0.0.0:7878")?,
            "--public" => public = Some(val("--public")?),
            "--data" => data = Some(PathBuf::from(val("--data")?)),
            "--open" => open = true,
            "--new-key" => new_key = true,
            "--quiet" => quiet = true,
            "--max-rooms" => max_rooms = val("--max-rooms")?.parse().map_err(|_| "--max-rooms: нужно число")?,
            "--max-peers" => max_peers = val("--max-peers")?.parse().map_err(|_| "--max-peers: нужно число")?,
            other => return Err(format!("неизвестный аргумент {other} (см. --help)")),
        }
    }
    let dir = data
        .or_else(|| directories::ProjectDirs::from("", "", "anvil-relay").map(|d| d.data_dir().to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("anvil-relay-data"));
    let identity = Identity::load_or_create(&dir)?;
    let key = if open {
        None
    } else {
        let kp = dir.join("access.key");
        let existing = std::fs::read_to_string(&kp).ok().map(|s| s.trim().to_string()).filter(|s| s.len() >= 16 && !new_key);
        Some(match existing {
            Some(k) => k,
            None => {
                let k = anvil_collab::random_hex::<16>().ok_or("недоступен генератор случайных чисел")?;
                anvil_relay::write_private(&kp, k.as_bytes())?;
                k
            }
        })
    };

    let mut cfg = Config::new(listen, identity);
    cfg.key = key.clone();
    cfg.max_rooms = max_rooms;
    cfg.max_peers = max_peers;
    if !quiet {
        cfg.log = Some(Arc::new(|s: &str| println!("{}", s)));
    }
    let relay = Relay::start(cfg).map_err(|e| format!("не удалось занять {listen}: {e}"))?;

    let guessed = public.is_none();
    let host = public.unwrap_or_else(|| anvil_collab::local_ip().map(|ip| ip.to_string()).unwrap_or_else(|| "АДРЕС_СЕРВЕРА".into()));
    let addr = RelayAddr { host, port: relay.addr().port(), fp: relay.fingerprint(), key: key.unwrap_or_default() };
    println!("Anvil relay {} слушает {}", env!("CARGO_PKG_VERSION"), relay.addr());
    println!("Отпечаток TLS: {}", anvil_collab::tls::fp_hex(&relay.fingerprint()));
    println!("Данные: {}", dir.display());
    println!();
    println!("Адрес для Anvil — у хозяина сессии: палитра команд → «Совместная работа: свой relay-сервер…»,");
    println!("вставьте строку relay ниже (или весь блок — в settings.toml):");
    println!();
    println!("[collab]");
    println!("relay = \"{}\"", addr.link());
    println!();
    if guessed {
        println!("Хост определён автоматически. Если сервер за NAT или есть домен — запустите с --public ВАШ_IP_ИЛИ_ДОМЕН.");
    }
    if open {
        println!("Внимание: --open — открыть сессию на этом relay может любой, кто знает адрес.");
    }
    println!("Не забудьте открыть порт {} (TCP) в файрволе.", relay.addr().port());
    // relay работает в фоновых потоках — главный просто спит
    loop {
        std::thread::park();
    }
}
