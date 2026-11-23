//! Заголовок окна, вкладки, «хлебные крошки», строка состояния, уведомления, приветствие.

use anvil_config::{FileIcons, LineNumbers, TabStyle, Visibility};
use eframe::egui::{self, Align2, Color32, CursorIcon, FontId, Id, Rect, ResizeDirection, Sense, Stroke, StrokeKind, ViewportCommand, pos2, vec2};

use super::{Icon, cr, icon, icon_button, logo, shadow, text, text_width};
use crate::app::{AnvilApp, Focus, ToastKind};
use crate::commands::Cmd;
use crate::fonts;
use crate::theme::{mix, with_alpha};

pub fn file_color(name: &str) -> Color32 {
    let ext = name.rsplit('.').next().unwrap_or_default().to_lowercase();
    let hex = match ext.as_str() {
        "rs" => 0xe8835a,
        "py" | "pyw" => 0x4c8fd6,
        "js" | "mjs" | "cjs" | "jsx" => 0xe6c84f,
        "ts" | "tsx" | "mts" => 0x3f8fe0,
        "c" | "h" => 0x7e93c9,
        "cpp" | "cc" | "hpp" | "cxx" => 0xa77cd9,
        "go" => 0x52c3d9,
        "java" | "kt" | "kts" => 0xe0884a,
        "cs" | "fs" => 0x8f6be0,
        "html" | "htm" | "vue" | "svelte" => 0xe0643f,
        "css" | "scss" | "less" => 0x5b8de8,
        "json" | "jsonc" => 0xc9b458,
        "md" | "markdown" => 0x6ea8ff,
        "toml" | "yaml" | "yml" | "ini" => 0x9a9aa6,
        "zig" => 0xf0a03c,
        "rb" => 0xd9534f,
        "php" => 0x7f86c9,
        "lua" => 0x4a6fd6,
        "sh" | "bash" | "ps1" | "bat" | "cmd" => 0x6fbf73,
        _ => 0x8a8a96,
    };
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl AnvilApp {
    pub(crate) fn ui_font(&self, size: f32) -> FontId {
        FontId::new(size * self.settings.ui.font_size / 13.0, fonts::ui_family())
    }

    pub(crate) fn bold_font(&self, size: f32) -> FontId {
        FontId::new(size * self.settings.ui.font_size / 13.0, fonts::bold_family())
    }

    pub(crate) fn hover_t(&mut self, id: impl std::hash::Hash, hovered: bool) -> f32 {
        let spec = self.spec(&self.settings.animations.hover.clone());
        self.tw.value(id, if hovered { 1.0 } else { 0.0 }, spec)
    }

    /// Значок файла по настройке: точка, буква или ничего. Возвращает ширину.
    pub(crate) fn file_badge(&self, p: &egui::Painter, x: f32, y: f32, name: &str) -> f32 {
        let c = file_color(name);
        match self.settings.ui.file_icons {
            FileIcons::Dots => {
                p.rect_filled(Rect::from_center_size(pos2(x + 4.0, y), vec2(7.0, 7.0)), cr(2.0), c);
                12.0
            }
            FileIcons::Letters => {
                let letter: String = name.rsplit('.').next().unwrap_or("?").chars().take(2).collect::<String>().to_uppercase();
                text(p, pos2(x + 6.0, y), Align2::CENTER_CENTER, &letter, FontId::new(8.5, fonts::bold_family()), c);
                15.0
            }
            FileIcons::None => 0.0,
        }
    }

    // ------------------------------------------------------------------ заголовок окна

    pub(crate) fn draw_titlebar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let c = self.colors.clone();
        let p = ui.painter().clone();
        p.rect_filled(rect, egui::CornerRadius::ZERO, c.titlebar);
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, c.border));

        // фон — перетаскивание окна; кнопки регистрируются позже и перекрывают его
        let drag = ui.interact(rect, Id::unique("titlebar-drag"), Sense::click_and_drag());
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        if drag.double_clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }

        let cy = rect.center().y;
        let mut x = rect.left() + 14.0;
        logo(&p, Rect::from_min_size(pos2(x, cy - 9.0), vec2(18.0, 18.0)), c.accent);
        x += 26.0;
        let name = if self.ws.name.is_empty() { "Anvil".to_string() } else { self.ws.name.clone() };
        let r = text(&p, pos2(x, cy), Align2::LEFT_CENTER, &name, self.bold_font(12.5), c.text);
        x = r.right() + 8.0;
        if let Some(b) = self.ws.branch.clone() {
            text(&p, pos2(x, cy), Align2::LEFT_CENTER, "/", self.ui_font(12.0), c.text_faint);
            x += 12.0;
            let w = text_width(ui.ctx(), &b, &self.ui_font(11.5)) + 28.0;
            let pill = Rect::from_min_size(pos2(x, cy - 10.0), vec2(w, 20.0));
            p.rect_filled(pill, cr(self.settings.theme.radius * 0.7), c.hover);
            icon(&p, Rect::from_min_size(pos2(pill.left() + 5.0, cy - 6.5), vec2(13.0, 13.0)), Icon::Branch, c.text_muted, 1.2);
            text(&p, pos2(pill.left() + 21.0, cy), Align2::LEFT_CENTER, &b, self.ui_font(11.5), c.text_muted);
        }

        // поиск по центру
        let sw = (rect.width() * 0.34).clamp(200.0, 420.0);
        let search = Rect::from_center_size(pos2(rect.center().x, cy), vec2(sw, 24.0));
        let sr = ui.interact(search, Id::unique("title-search"), Sense::click());
        let ht = self.hover_t("title-search", sr.hovered());
        p.rect_filled(search, cr(self.settings.theme.radius * 0.75), mix(c.hover, with_alpha(c.text, 0.09), ht));
        p.rect_stroke(search, cr(self.settings.theme.radius * 0.75), Stroke::new(1.0, mix(c.border, c.border_strong, ht)), StrokeKind::Inside);
        icon(&p, Rect::from_min_size(pos2(search.left() + 9.0, cy - 6.5), vec2(13.0, 13.0)), Icon::Search, c.text_faint, 1.3);
        text(&p, pos2(search.left() + 30.0, cy), Align2::LEFT_CENTER, "Поиск файлов и команд", self.ui_font(11.5), c.text_faint);
        let kb = self.keymap.label(Cmd::PaletteFiles).unwrap_or_default();
        self.kbd(&p, pos2(search.right() - 8.0, cy), &kb, true);
        if sr.clicked() {
            self.exec(Cmd::PaletteFiles);
        }
        if sr.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        // справа: запуск, настройки, окно
        let bw = 46.0;
        let mut rx = rect.right();
        for (k, ic) in [(2, Icon::Close), (1, Icon::Max), (0, Icon::Min)] {
            rx -= bw;
            let b = Rect::from_min_max(pos2(rx, rect.top()), pos2(rx + bw, rect.bottom()));
            let resp = ui.interact(b, Id::unique(("winctl", k)), Sense::click());
            let ht = self.hover_t(("winctl", k), resp.hovered());
            let bg = if k == 2 { with_alpha(Color32::from_rgb(232, 17, 35), ht) } else { with_alpha(c.hover, ht * 1.6) };
            p.rect_filled(b, egui::CornerRadius::ZERO, bg);
            let col = if k == 2 && ht > 0.5 { Color32::WHITE } else { c.text_muted };
            icon(&p, Rect::from_center_size(b.center(), vec2(12.0, 12.0)), ic, col, 1.1);
            if resp.clicked() {
                match k {
                    0 => ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true)),
                    1 => ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
                    _ => ui.ctx().send_viewport_cmd(ViewportCommand::Close),
                }
            }
        }
        rx -= 8.0;
        let gear = Rect::from_min_size(pos2(rx - 28.0, cy - 13.0), vec2(28.0, 26.0));
        let ht = self.hover_t("title-gear", ui.rect_contains_pointer(gear));
        if icon_button(ui, gear, Id::unique("title-gear"), Icon::Gear, &c, ht, false).clicked() {
            self.exec(Cmd::SettingsOpen);
        }
        rx -= 34.0;
        let can_run = self.doc().is_some_and(|d| d.path.is_some() && self.registry.for_language(d.lang.id).next().is_some());
        if can_run || self.panel.run.is_some() {
            let running = self.panel.run.is_some();
            let label = if running { "Стоп" } else { "Запуск" };
            let w = text_width(ui.ctx(), label, &self.ui_font(11.5)) + 34.0;
            let b = Rect::from_min_size(pos2(rx - w, cy - 12.0), vec2(w, 24.0));
            let resp = ui.interact(b, Id::unique("title-run"), Sense::click());
            let ht = self.hover_t("title-run", resp.hovered());
            let fill = if running { with_alpha(c.err, 0.18 + 0.1 * ht) } else { mix(c.accent, mix(c.accent, Color32::WHITE, 0.15), ht) };
            let fg = if running { c.err } else { c.on_accent };
            p.rect_filled(b, cr(self.settings.theme.radius * 0.75), fill);
            icon(&p, Rect::from_min_size(pos2(b.left() + 8.0, cy - 6.5), vec2(13.0, 13.0)), if running { Icon::Stop } else { Icon::Play }, fg, 1.3);
            text(&p, pos2(b.left() + 25.0, cy), Align2::LEFT_CENTER, label, self.bold_font(11.5), fg);
            if resp.clicked() {
                self.exec(if running { Cmd::RunStop } else { Cmd::RunFile });
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
        }
    }

    pub(crate) fn kbd(&self, p: &egui::Painter, right_center: egui::Pos2, label: &str, small: bool) -> f32 {
        if label.is_empty() {
            return 0.0;
        }
        let c = &self.colors;
        let font = self.ui_font(if small { 10.0 } else { 10.5 });
        let mut x = right_center.x;
        let parts: Vec<&str> = label.split(['+', ' ']).filter(|s| !s.is_empty()).collect();
        for part in parts.iter().rev() {
            let w = text_width(&self.ctx, part, &font) + 10.0;
            let r = Rect::from_min_size(pos2(x - w, right_center.y - 9.0), vec2(w, 18.0));
            p.rect_filled(r, cr(4.0), with_alpha(c.text, 0.05));
            p.rect_stroke(r, cr(4.0), Stroke::new(1.0, c.border_strong), StrokeKind::Inside);
            text(p, r.center(), Align2::CENTER_CENTER, part, font.clone(), c.text_muted);
            x -= w + 3.0;
        }
        right_center.x - x
    }

    // ------------------------------------------------------------------ вкладки

    pub(crate) fn draw_tabs(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, egui::CornerRadius::ZERO, c.tabs);
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, c.border));
        let font = self.ui_font(12.0);
        let spec = self.spec(&self.settings.animations.tabs.clone());
        let ind_spec = self.spec(&self.settings.animations.tab_indicator.clone());
        let style = self.settings.ui.tab_style;
        let radius = self.settings.theme.radius;

        let widths: Vec<f32> = self.docs.iter().map(|d| text_width(ui.ctx(), &d.name, &font) + 58.0).collect();
        let total: f32 = widths.iter().sum();
        let mut offset = 0.0;
        if let Some(a) = self.active {
            let right: f32 = widths[..=a.min(widths.len().saturating_sub(1))].iter().sum();
            if right > rect.width() - 60.0 {
                offset = (right - rect.width() + 60.0).min((total - rect.width()).max(0.0));
            }
        }
        let offset = self.tw.value("tabs-offset", offset, ind_spec);

        let mut x = rect.left() - offset;
        let mut close_req = None;
        let mut activate = None;
        let mut ind = None;
        for i in 0..self.docs.len() {
            let (id, name, dirty) = (self.docs[i].id, self.docs[i].name.clone(), self.docs[i].dirty());
            let enter = self.tw.enter(("tab-in", id), 0.0, 1.0, spec);
            let w = widths[i] * enter;
            let xx = self.tw.value(("tab-x", id), x, spec);
            let tr = Rect::from_min_size(pos2(xx, rect.top()), vec2(w, rect.height() - 1.0));
            x += w;
            let active = self.active == Some(i);
            let resp = ui.interact(tr.intersect(rect), Id::unique(("tab", id)), Sense::click());
            let ht = self.hover_t(("tab-h", id), resp.hovered());
            match style {
                TabStyle::Pill => {
                    let pill = tr.shrink2(vec2(3.0, 5.0));
                    if active {
                        p.rect_filled(pill, cr(radius), with_alpha(c.accent, 0.14));
                    } else if ht > 0.01 {
                        p.rect_filled(pill, cr(radius), with_alpha(c.hover, ht * 1.5));
                    }
                }
                TabStyle::Classic | TabStyle::Underline => {
                    if active {
                        p.rect_filled(tr, egui::CornerRadius::ZERO, c.tab_active);
                    } else if ht > 0.01 {
                        p.rect_filled(tr, egui::CornerRadius::ZERO, with_alpha(c.hover, ht));
                    }
                    p.line_segment([tr.right_top(), tr.right_bottom()], Stroke::new(1.0, c.border));
                }
            }
            if active {
                ind = Some((tr.left(), tr.width()));
            }
            let alpha = enter;
            let cy = tr.center().y;
            let bw = self.file_badge(&p, tr.left() + 12.0, cy, &name);
            let col = if active { c.text } else { mix(c.text_faint, c.text_muted, ht) };
            text(&p, pos2(tr.left() + 14.0 + bw, cy), Align2::LEFT_CENTER, &name, font.clone(), with_alpha(col, alpha));
            // крестик или точка «не сохранено»
            let xr = Rect::from_center_size(pos2(tr.right() - 16.0, cy), vec2(18.0, 18.0));
            let xresp = ui.interact(xr.intersect(rect), Id::unique(("tab-x-btn", id)), Sense::click());
            let show_x = match self.settings.ui.tab_close_button {
                Visibility::Always => true,
                Visibility::Hover => resp.hovered() || xresp.hovered() || active,
                Visibility::Never => false,
            };
            if xresp.hovered() {
                p.rect_filled(xr, cr(4.0), c.hover);
            }
            if dirty && !xresp.hovered() {
                p.circle_filled(xr.center(), 3.5, c.text_muted);
            } else if show_x {
                icon(&p, xr.shrink(4.0), Icon::Close, if xresp.hovered() { c.text } else { c.text_faint }, 1.2);
            }
            if xresp.clicked() || resp.middle_clicked() {
                close_req = Some(i);
            } else if resp.clicked() {
                activate = Some(i);
            }
        }
        if let Some((ix, iw)) = ind {
            let x = self.tw.value("tab-ind-x", ix, ind_spec);
            let w = self.tw.value("tab-ind-w", iw, ind_spec);
            match style {
                TabStyle::Underline => {
                    p.rect_filled(Rect::from_min_size(pos2(x + 10.0, rect.bottom() - 2.0), vec2((w - 20.0).max(0.0), 2.0)), cr(1.0), c.accent);
                }
                TabStyle::Classic => {
                    p.rect_filled(Rect::from_min_size(pos2(x, rect.top()), vec2(w, 2.0)), egui::CornerRadius::ZERO, c.accent);
                }
                TabStyle::Pill => {}
            }
        }
        // колесо мыши — листать вкладки
        if ui.rect_contains_pointer(rect) {
            let d = super::wheel_delta(ui.ctx());
            if d.y.abs() > 0.0
                && self.docs.len() > 1
                && let Some(a) = self.active
            {
                let n = self.docs.len();
                activate = Some(if d.y < 0.0 { (a + 1).min(n - 1) } else { a.saturating_sub(1) });
            }
        }
        if let Some(i) = close_req {
            self.close_tab(i, false);
        } else if let Some(i) = activate {
            self.activate(i);
        }
    }

    pub(crate) fn draw_breadcrumbs(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(d) = self.doc() else { return };
        let c = &self.colors;
        let p = ui.painter().with_clip_rect(rect);
        let rel = match &d.path {
            Some(path) => self.ws.relative(path),
            None => d.name.clone(),
        };
        let parts: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
        let font = self.ui_font(11.5);
        let mut x = rect.left() + 14.0;
        let cy = rect.center().y;
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            let r = text(&p, pos2(x, cy), Align2::LEFT_CENTER, part, font.clone(), if last { c.text_muted } else { c.text_faint });
            x = r.right() + 6.0;
            if !last {
                icon(&p, Rect::from_min_size(pos2(x - 2.0, cy - 5.0), vec2(10.0, 10.0)), Icon::ChevronRight, c.text_faint, 1.1);
                x += 12.0;
            }
        }
    }

    // ------------------------------------------------------------------ строка состояния

    pub(crate) fn draw_statusbar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, egui::CornerRadius::ZERO, c.statusbar);
        p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, c.border));
        let font = self.ui_font(11.0);
        let cy = rect.center().y;
        let mut x = rect.left() + 12.0;

        if let Some(b) = self.ws.branch.clone() {
            icon(&p, Rect::from_min_size(pos2(x, cy - 6.0), vec2(12.0, 12.0)), Icon::Branch, c.text_faint, 1.2);
            let r = text(&p, pos2(x + 16.0, cy), Align2::LEFT_CENTER, &b, font.clone(), c.text_muted);
            x = r.right() + 16.0;
        }
        // ошибки и предупреждения → панель «Проблемы»
        if !self.lsp.servers.is_empty() {
            let (e, w) = self.diag_counts();
            let start = x;
            icon(&p, Rect::from_min_size(pos2(x, cy - 6.0), vec2(12.0, 12.0)), Icon::Error, if e > 0 { c.err } else { c.text_faint }, 1.2);
            let r1 = text(&p, pos2(x + 16.0, cy), Align2::LEFT_CENTER, &e.to_string(), font.clone(), if e > 0 { c.err } else { c.text_muted });
            x = r1.right() + 10.0;
            icon(&p, Rect::from_min_size(pos2(x, cy - 6.0), vec2(12.0, 12.0)), Icon::Warn, if w > 0 { c.warn } else { c.text_faint }, 1.2);
            let r2 = text(&p, pos2(x + 16.0, cy), Align2::LEFT_CENTER, &w.to_string(), font.clone(), if w > 0 { c.warn } else { c.text_muted });
            let zone = Rect::from_min_max(pos2(start - 4.0, rect.top() + 2.0), pos2(r2.right() + 4.0, rect.bottom() - 2.0));
            let resp = ui.interact(zone, Id::unique("status-problems"), Sense::click());
            if resp.hovered() {
                p.rect_filled(zone, cr(4.0), c.hover);
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.clicked() {
                self.exec(Cmd::ViewProblems);
            }
            x = r2.right() + 16.0;
        }
        // тулчейн для текущего файла
        if let Some(d) = self.doc() {
            let lang = d.lang.id;
            let packs: Vec<_> = self.registry.for_language(lang).collect();
            if !packs.is_empty() {
                let found = packs.iter().find(|pk| self.tools.found.get(&pk.id).is_some_and(Option::is_some));
                let (label, col, dot) = match found {
                    Some(pk) => (pk.name.clone(), c.text_muted, c.ok),
                    None if self.tools.scanned => ("компилятор не найден".to_string(), c.warn, c.warn),
                    None => ("поиск тулчейна…".to_string(), c.text_faint, c.text_faint),
                };
                let w = text_width(ui.ctx(), &label, &font) + 20.0;
                let r = Rect::from_min_size(pos2(x - 4.0, rect.top() + 2.0), vec2(w + 4.0, rect.height() - 4.0));
                let resp = ui.interact(r, Id::unique("status-tool"), Sense::click());
                if resp.hovered() {
                    p.rect_filled(r, cr(4.0), c.hover);
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                p.circle_filled(pos2(x + 4.0, cy), 3.0, dot);
                text(&p, pos2(x + 13.0, cy), Align2::LEFT_CENTER, &label, font.clone(), col);
                if resp.clicked() {
                    self.exec(if found.is_some() { Cmd::RunFile } else { Cmd::ViewToolchains });
                }
                x += w + 12.0;
            }
        }
        if let Some(run) = &self.panel.run {
            let t = (self.now * 2.0) as f32;
            let a = 0.5 + 0.5 * (t * std::f32::consts::PI).sin();
            p.circle_filled(pos2(x + 4.0, cy), 3.5, with_alpha(c.accent, 0.4 + 0.6 * a));
            let r = text(&p, pos2(x + 13.0, cy), Align2::LEFT_CENTER, &format!("Выполняется: {}", run.title), font.clone(), c.text_muted);
            x = r.right() + 14.0;
        }
        if let Some((m, t)) = self.msg.clone() {
            let age = (self.now - t) as f32;
            let a = if age < 3.0 { 1.0 } else { (1.0 - (age - 3.0) / 0.6).max(0.0) };
            if a > 0.0 {
                p.circle_filled(pos2(x + 3.0, cy), 3.0, with_alpha(c.accent, a));
                text(&p, pos2(x + 12.0, cy), Align2::LEFT_CENTER, &m, font.clone(), with_alpha(c.text_muted, a));
                if a < 1.0 {
                    self.tw.active = true;
                }
            } else {
                self.msg = None;
            }
        }

        // справа налево
        let mut rx = rect.right() - 12.0;
        let mut right = |p: &egui::Painter, s: &str, col: Color32, ctx: &egui::Context| {
            let w = text_width(ctx, s, &font);
            text(p, pos2(rx, cy), Align2::RIGHT_CENTER, s, font.clone(), col);
            rx -= w + 16.0;
        };
        if self.settings.ui.show_fps {
            let avg = self.frame_times.iter().sum::<f32>() / self.frame_times.len().max(1) as f32;
            right(&p, &format!("{:.0} fps", 1.0 / avg.max(0.001)), c.accent, ui.ctx());
        }
        if let Some((s, _)) = self.chord {
            right(&p, &format!("{} …", s.label()), c.accent, ui.ctx());
        }
        if let Some((label, pct, busy, failed)) = self.lsp_status_label() {
            let label = match pct {
                Some(p) => format!("{label} {p}%"),
                None => label,
            };
            let col = if failed {
                c.warn
            } else if busy {
                c.text_muted
            } else {
                c.text_faint
            };
            let label = if busy {
                format!("◌ {label}")
            } else if failed {
                label
            } else {
                format!("✓ {label}")
            };
            right(&p, &label, col, ui.ctx());
        }
        if let Some(d) = self.doc() {
            right(&p, d.lang.name, c.text_muted, ui.ctx());
            right(&p, d.ed.buffer.line_ending.label(), c.text_faint, ui.ctx());
            right(&p, "UTF-8", c.text_faint, ui.ctx());
            let indent = if d.ed.cfg.insert_spaces { format!("Пробелы: {}", d.ed.cfg.tab_size) } else { format!("Табы: {}", d.ed.cfg.tab_size) };
            right(&p, &indent, c.text_faint, ui.ctx());
            let (l, col) = d.ed.cursor_line_col();
            let n = d.ed.sel.len();
            let pos = if n > 1 {
                format!("Стр {}, Кол {}  ·  курсоров: {n}", l + 1, col + 1)
            } else {
                format!("Стр {}, Кол {}", l + 1, col + 1)
            };
            right(&p, &pos, c.text_muted, ui.ctx());
            let _ = LineNumbers::On;
        }
    }

    // ------------------------------------------------------------------ уведомления

    pub(crate) fn draw_toasts(&mut self, ui: &mut egui::Ui, area: Rect) {
        let spec = self.spec(&self.settings.animations.toasts.clone());
        let c = self.colors.clone();
        let p = ui.painter().clone();
        let font = self.ui_font(12.0);
        let mut y = area.bottom() - 14.0;
        let mut dismiss = None;
        let toasts: Vec<(u64, String, ToastKind, f64, f32)> = self.toasts.iter().rev().map(|t| (t.id, t.text.clone(), t.kind, t.born, t.ttl)).collect();
        for (id, msg, kind, born, ttl) in toasts {
            let w = 360.0f32.min(area.width() - 28.0);
            let galley = ui.ctx().fonts_mut(|f| f.layout(msg.clone(), font.clone(), c.text, w - 48.0));
            let h = galley.size().y + 20.0;
            let k_in = self.tw.since(born, spec);
            let age = (self.now - born) as f32;
            let k_out = if age > ttl { ((age - ttl) / 0.4).clamp(0.0, 1.0) } else { 0.0 };
            let k = k_in * (1.0 - k_out);
            if k <= 0.001 {
                continue;
            }
            let yy = self.tw.value(("toast-y", id), y - h, spec);
            let r = Rect::from_min_size(pos2(area.right() - 14.0 - w + (1.0 - k) * 40.0, yy), vec2(w, h));
            shadow(&p, r, self.settings.theme.radius, with_alpha(c.shadow, k * 0.8), 24);
            p.rect_filled(r, cr(self.settings.theme.radius), with_alpha(c.popup, k));
            p.rect_stroke(r, cr(self.settings.theme.radius), Stroke::new(1.0, with_alpha(c.border_strong, k)), StrokeKind::Inside);
            let (ic, col) = match kind {
                ToastKind::Info => (Icon::Info, c.accent),
                ToastKind::Ok => (Icon::Check, c.ok),
                ToastKind::Warn => (Icon::Warn, c.warn),
                ToastKind::Error => (Icon::Error, c.err),
            };
            icon(&p, Rect::from_min_size(pos2(r.left() + 12.0, r.top() + 11.0), vec2(16.0, 16.0)), ic, with_alpha(col, k), 1.4);
            p.galley(pos2(r.left() + 38.0, r.top() + 10.0), galley, with_alpha(c.text, k));
            let resp = ui.interact(r, Id::unique(("toast", id)), Sense::click());
            if resp.clicked() {
                dismiss = Some(id);
            }
            y = yy - 8.0;
        }
        if let Some(id) = dismiss
            && let Some(t) = self.toasts.iter_mut().find(|t| t.id == id)
        {
            t.ttl = ((self.now - t.born) as f32).min(t.ttl);
        }
    }

    // ------------------------------------------------------------------ рамка окна

    pub(crate) fn window_edges(&mut self, ui: &mut egui::Ui, full: Rect) {
        if ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let e = 5.0;
        let zones = [
            (Rect::from_min_max(full.left_top(), pos2(full.left() + e, full.bottom())), ResizeDirection::West, CursorIcon::ResizeHorizontal),
            (Rect::from_min_max(pos2(full.right() - e, full.top()), full.right_bottom()), ResizeDirection::East, CursorIcon::ResizeHorizontal),
            (Rect::from_min_max(full.left_top(), pos2(full.right(), full.top() + e)), ResizeDirection::North, CursorIcon::ResizeVertical),
            (Rect::from_min_max(pos2(full.left(), full.bottom() - e), full.right_bottom()), ResizeDirection::South, CursorIcon::ResizeVertical),
            (Rect::from_min_size(full.left_top(), vec2(e * 2.0, e * 2.0)), ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
            (
                Rect::from_min_size(pos2(full.right() - e * 2.0, full.bottom() - e * 2.0), vec2(e * 2.0, e * 2.0)),
                ResizeDirection::SouthEast,
                CursorIcon::ResizeNwSe,
            ),
            (Rect::from_min_size(pos2(full.right() - e * 2.0, full.top()), vec2(e * 2.0, e * 2.0)), ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
            (Rect::from_min_size(pos2(full.left(), full.bottom() - e * 2.0), vec2(e * 2.0, e * 2.0)), ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
        ];
        for (i, (r, dir, cur)) in zones.into_iter().enumerate() {
            let resp = ui.interact(r, Id::unique(("edge", i)), Sense::drag());
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(cur);
            }
            if resp.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        }
        let p = ui.painter();
        p.rect_stroke(full, egui::CornerRadius::ZERO, Stroke::new(1.0, self.colors.border_strong), StrokeKind::Inside);
    }

    // ------------------------------------------------------------------ приветствие

    pub(crate) fn draw_welcome(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        let w = 420.0f32.min(rect.width() - 40.0);
        let x = rect.center().x - w / 2.0;
        let mut y = rect.top() + (rect.height() * 0.16).max(30.0);
        let k = self.tw.since(self.launched + 0.1, self.spec(&self.settings.animations.launch.clone()));
        logo(&p, Rect::from_min_size(pos2(x, y + (1.0 - k) * 8.0), vec2(40.0, 40.0)), with_alpha(c.accent, k));
        text(&p, pos2(x + 54.0, y + 12.0), Align2::LEFT_CENTER, "Anvil", self.bold_font(24.0), with_alpha(c.text, k));
        text(
            &p,
            pos2(x + 54.0, y + 34.0),
            Align2::LEFT_CENTER,
            &format!("v{} · лёгкая IDE на Rust", env!("CARGO_PKG_VERSION")),
            self.ui_font(12.0),
            with_alpha(c.text_faint, k),
        );
        y += 78.0;
        text(&p, pos2(x, y), Align2::LEFT_CENTER, "НАЧАТЬ", self.bold_font(10.5), c.text_faint);
        y += 14.0;
        let actions = [
            (Icon::Folder, "Открыть папку", Cmd::FileOpenFolder),
            (Icon::Files, "Открыть файл", Cmd::FileOpen),
            (Icon::Plus, "Новый файл", Cmd::FileNew),
            (Icon::Search, "Палитра команд", Cmd::PaletteCommands),
            (Icon::Gear, "Настройки (settings.toml)", Cmd::SettingsOpen),
            (Icon::Download, "Перенести настройки из VS Code", Cmd::ImportVscodeSettings),
            (Icon::Blocks, "Тулчейны: 40+ компиляторов", Cmd::ViewToolchains),
        ];
        for (i, (ic, label, cmd)) in actions.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x - 8.0, y), vec2(w + 16.0, 30.0));
            let resp = ui.interact(r, Id::unique(("welcome", i)), Sense::click());
            let ht = self.hover_t(("welcome", i), resp.hovered());
            let kk = self.tw.since(self.launched + 0.15 + i as f64 * 0.04, self.spec(&self.settings.animations.tree.clone()));
            let dx = (1.0 - kk) * -10.0;
            if ht > 0.01 {
                p.rect_filled(r, cr(self.settings.theme.radius * 0.8), with_alpha(c.hover, ht * 1.6));
            }
            icon(&p, Rect::from_min_size(pos2(x + dx, y + 7.0), vec2(16.0, 16.0)), *ic, with_alpha(mix(c.text_muted, c.accent, ht), kk), 1.4);
            text(&p, pos2(x + 26.0 + dx, y + 15.0), Align2::LEFT_CENTER, label, self.ui_font(13.0), with_alpha(mix(c.text_muted, c.text, ht), kk));
            let kb = self.keymap.label(*cmd).unwrap_or_default();
            self.kbd(&p, pos2(x + w, y + 15.0), &kb, true);
            if resp.clicked() {
                self.exec(*cmd);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            y += 32.0;
        }
        let recent: Vec<_> = self.session.recent.iter().filter(|r| r.is_dir()).take(6).cloned().collect();
        if !recent.is_empty() {
            y += 16.0;
            text(&p, pos2(x, y), Align2::LEFT_CENTER, "НЕДАВНИЕ", self.bold_font(10.5), c.text_faint);
            y += 14.0;
            let mut open = None;
            for (i, r) in recent.iter().enumerate() {
                let row = Rect::from_min_size(pos2(x - 8.0, y), vec2(w + 16.0, 28.0));
                let resp = ui.interact(row, Id::unique(("recent", i)), Sense::click());
                let ht = self.hover_t(("recent", i), resp.hovered());
                if ht > 0.01 {
                    p.rect_filled(row, cr(self.settings.theme.radius * 0.8), with_alpha(c.hover, ht * 1.6));
                }
                let name = r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let nr = text(&p, pos2(x, y + 14.0), Align2::LEFT_CENTER, &name, self.ui_font(12.5), mix(c.text_muted, c.text, ht));
                text(&p, pos2(nr.right() + 10.0, y + 14.0), Align2::LEFT_CENTER, &r.to_string_lossy(), self.ui_font(11.0), c.text_faint);
                if resp.clicked() {
                    open = Some(r.clone());
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                y += 28.0;
            }
            if let Some(r) = open {
                self.open_folder(&r);
            }
        }
        if self.focus == Focus::Editor {
            // в пустом редакторе вводить некуда
        }
    }
}
