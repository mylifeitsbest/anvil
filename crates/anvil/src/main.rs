//! Anvil — быстрая и лёгкая IDE на Rust.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anim;
mod app;
mod commands;
mod doc;
mod fonts;
mod lsp;
mod session;
mod theme;
mod ui;
mod workspace;

use std::path::PathBuf;

/// Аргументы командной строки.
#[derive(Debug, Default, Clone)]
pub struct Args {
    /// Папка или файлы для открытия.
    pub paths: Vec<PathBuf>,
    /// Сделать снимок окна в BMP и выйти (для документации и проверок).
    pub screenshot: Option<PathBuf>,
    /// Через сколько кадров снимать.
    pub frames: u64,
    /// Выполнить команды палитры после запуска (через запятую) — для демонстраций.
    pub run: Vec<String>,
    /// На каком кадре выполнить `--run`.
    pub run_at: u64,
    /// Поставить курсор: строка:колонка (с единицы).
    pub goto: Option<(usize, usize)>,
    /// Сценарий для демонстраций и проверок: строки `at КАДР действие`.
    pub script: Vec<(u64, String)>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args { frames: 40, run_at: 4, ..Args::default() };
    let mut it = std::env::args_os().skip(1);
    while let Some(arg) = it.next() {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!(
                    "Anvil {}\n\nanvil [ПАПКА | ФАЙЛ...]\n\n  --screenshot ФАЙЛ.bmp  снимок окна и выход\n  --frames N             через сколько кадров снимать (по умолчанию 40)\n  --run команда,команда  выполнить команды после запуска
  --run-at N             на каком кадре выполнить --run (по умолчанию 4)
  --goto СТРОКА:КОЛОНКА  поставить курсор
  --script ФАЙЛ          сценарий: строки «at КАДР goto|type|run|shot|quit …»\n  --version              версия",
                    env!("CARGO_PKG_VERSION")
                );
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("anvil {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--screenshot" => a.screenshot = Some(it.next().map(PathBuf::from).ok_or("--screenshot: нужен путь")?),
            "--frames" => a.frames = it.next().and_then(|v| v.to_string_lossy().parse().ok()).ok_or("--frames: нужно число")?,
            "--script" => {
                let path = it.next().ok_or("--script: нужен файл")?;
                let text = std::fs::read_to_string(&path).map_err(|e| format!("--script: {e}"))?;
                for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
                    let rest = line.strip_prefix("at ").ok_or_else(|| format!("--script: строка должна начинаться с «at»: {line}"))?;
                    let (n, action) = rest.split_once(' ').ok_or_else(|| format!("--script: нет действия: {line}"))?;
                    a.script.push((n.parse().map_err(|_| format!("--script: не число: {n}"))?, action.to_string()));
                }
            }
            "--run-at" => a.run_at = it.next().and_then(|v| v.to_string_lossy().parse().ok()).ok_or("--run-at: нужно число")?,
            "--goto" => {
                let v = it.next().ok_or("--goto: нужно СТРОКА:КОЛОНКА")?.to_string_lossy().to_string();
                let (l, c) = v.split_once(':').unwrap_or((v.as_str(), "1"));
                a.goto = Some((l.parse().map_err(|_| "--goto: строка")?, c.parse().map_err(|_| "--goto: колонка")?));
            }
            "--run" => a.run = it.next().map(|v| v.to_string_lossy().split(',').map(|s| s.trim().to_string()).collect()).ok_or("--run: нужен список")?,
            _ => a.paths.push(PathBuf::from(arg)),
        }
    }
    Ok(a)
}

fn main() -> eframe::Result<()> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let paths = anvil_config::Paths::detect();
    let ensure_err = paths.ensure().err().map(|e| format!("не удалось создать папку настроек {}: {e}", paths.dir.display()));
    let root = args.paths.first().map(|p| if p.is_dir() { p.clone() } else { p.parent().map(PathBuf::from).unwrap_or_default() });
    let project = root.as_ref().map(|r| r.join(".anvil").join("settings.toml"));
    let mut loaded = anvil_config::load_settings(&paths.settings, project.as_deref());
    loaded.warnings.extend(ensure_err);
    let s = &loaded.settings.window;

    let icon = egui::IconData { rgba: include_bytes!("../../../assets/icon/anvil-64.rgba").to_vec(), width: 64, height: 64 };
    let viewport = egui::ViewportBuilder::default()
        .with_title("Anvil")
        .with_app_id("anvil")
        .with_inner_size([s.width.max(640.0), s.height.max(400.0)])
        .with_min_inner_size([560.0, 360.0])
        .with_icon(icon)
        .with_decorations(!s.custom_titlebar)
        .with_transparent(s.opacity < 0.999)
        .with_maximized(s.start_maximized);
    // снимки и сценарии рисуются в окне за пределами экрана — не мешают работать и не ловят случайные клики
    let headless = args.screenshot.is_some() || !args.script.is_empty();
    let viewport = if headless { viewport.with_position([-6000.0, 80.0]).with_taskbar(false).with_active(false) } else { viewport };
    let mut options = eframe::NativeOptions { viewport, centered: !headless, ..Default::default() };
    options.glow_options.vsync = s.vsync;
    #[cfg(feature = "wgpu")]
    {
        options.renderer = eframe::Renderer::Wgpu;
    }
    eframe::run_native("Anvil", options, Box::new(move |cc| Ok(Box::new(app::AnvilApp::new(cc, paths, loaded, args)))))
}

pub use eframe::egui;
