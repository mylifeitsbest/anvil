//! Плашка обновления в углу окна и значок в строке состояния.

use std::sync::atomic::Ordering;

use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Painter, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::{Icon, cr, icon, shadow, text, text_width};
use crate::app::AnvilApp;
use crate::theme::{mix, with_alpha};
use crate::update::{Method, NoteLine, Phase, fmt_mb, plain_notes};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Act {
    Primary,
    Secondary,
    Close,
    Notes,
    Page,
}

impl AnvilApp {
    /// Кнопка плашки: основная (акцент) или тихая. Возвращает «нажата».
    fn card_button(&mut self, ui: &mut egui::Ui, p: &Painter, r: Rect, label: &str, primary: bool, k: f32) -> bool {
        let c = self.colors.clone();
        let resp = ui.interact(r, Id::unique(("update-btn", label)), Sense::click());
        let ht = self.hover_t(("update-btn", label), resp.hovered());
        let radius = cr(self.settings.theme.radius * 0.75);
        if primary {
            p.rect_filled(r, radius, with_alpha(mix(c.accent, mix(c.accent, Color32::WHITE, 0.15), ht), k));
            text(p, r.center(), Align2::CENTER_CENTER, label, self.bold_font(11.5), with_alpha(c.on_accent, k));
        } else {
            p.rect_filled(r, radius, with_alpha(mix(c.hover, with_alpha(c.text, 0.09), ht), k));
            p.rect_stroke(r, radius, Stroke::new(1.0, with_alpha(mix(c.border, c.border_strong, ht), k)), StrokeKind::Inside);
            text(p, r.center(), Align2::CENTER_CENTER, label, self.ui_font(11.5), with_alpha(c.text_muted, k));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp.clicked()
    }

    /// Ссылка-текст. Возвращает «нажата».
    fn card_link(&mut self, ui: &mut egui::Ui, p: &Painter, pos: egui::Pos2, anchor: Align2, label: &str, k: f32) -> bool {
        let c = self.colors.clone();
        let r = text(p, pos, anchor, label, self.ui_font(11.5), with_alpha(c.accent, k));
        let resp = ui.interact(r.expand(3.0), Id::unique(("update-link", label)), Sense::click());
        if resp.hovered() {
            p.line_segment([pos2(r.left(), r.bottom()), r.right_bottom()], Stroke::new(1.0, with_alpha(c.accent, 0.6 * k)));
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        resp.clicked()
    }

    /// Плашка «Доступна новая версия» над строкой состояния.
    /// Возвращает, докуда снизу её занимает плашка — выше идут уведомления.
    pub(crate) fn draw_update_card(&mut self, ui: &mut egui::Ui, area: Rect) -> f32 {
        let spec = self.spec(&self.settings.animations.toasts.clone());
        let k = self.tw.value("update-card", if self.update.card_visible() { 1.0 } else { 0.0 }, spec);
        let Some(rel) = self.update.release.clone() else { return area.bottom() };
        if k <= 0.001 {
            return area.bottom();
        }
        let c = self.colors.clone();
        let p = ui.painter().clone();
        let radius = self.settings.theme.radius;
        let w = 348.0f32.min(area.width() - 28.0);
        let pad = 14.0;
        let col_x = 38.0; // текст после значка
        let text_w = w - col_x - pad;
        let phase = self.update.phase.clone();
        let asset = self.update.method.asset_name().and_then(|n| rel.asset(n)).cloned();
        let can_install = asset.is_some() && !matches!(self.update.method, Method::Manual(_));

        let (ic, ic_col, title, sub) = match &phase {
            Phase::Downloading => (Icon::Download, c.accent, format!("Загрузка Anvil {}", rel.version), String::new()),
            Phase::Ready(_) => (
                Icon::Check,
                c.ok,
                format!("Anvil {} готов к установке", rel.version),
                "Перезапуск займёт несколько секунд, открытые файлы сохранятся".to_string(),
            ),
            Phase::Installing => (Icon::Refresh, c.accent, format!("Установка Anvil {}…", rel.version), "Anvil откроется снова сам".to_string()),
            Phase::Failed(e) => (Icon::Warn, c.warn, "Не удалось обновить".to_string(), e.clone()),
            _ => {
                let mut sub = format!("У вас {}", self.update.current);
                match (&asset, &self.update.method) {
                    (_, Method::Manual(why)) => sub.push_str(&format!(" · {why}")),
                    (Some(a), _) if a.size > 0 => sub.push_str(&format!(" · {}", fmt_mb(a.size))),
                    _ => {}
                }
                (Icon::Download, c.accent, format!("Доступна Anvil {}", rel.version), sub)
            }
        };
        let (primary, secondary): (Option<&str>, Option<&str>) = match &phase {
            Phase::Downloading => (None, Some("Отмена")),
            Phase::Ready(_) => (Some("Перезапустить"), Some("Позже")),
            Phase::Installing => (None, None),
            Phase::Failed(_) => (Some("Повторить"), Some("Скачать вручную")),
            _ if can_install => (Some("Обновить"), Some("Позже")),
            _ => (Some("Открыть релиз"), Some("Позже")),
        };
        let notes_allowed = !matches!(phase, Phase::Downloading | Phase::Installing | Phase::Failed(_));

        // высота по содержимому
        let title_font = self.bold_font(12.5);
        let sub_font = self.ui_font(11.5);
        let sub_g = (!sub.is_empty()).then(|| ui.ctx().fonts_mut(|f| f.layout(sub.clone(), sub_font.clone(), c.text_muted, text_w)));
        let notes_g = (self.update.notes && notes_allowed).then(|| {
            let mut lines = plain_notes(&rel.notes, 12);
            if lines.is_empty() {
                lines.push(NoteLine { heading: false, text: "Описания нет — подробности на странице релиза".into() });
            }
            // заголовки — ярче и жирнее, остальное приглушённо
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = text_w;
            for (i, l) in lines.iter().enumerate() {
                let (font_id, color) = if l.heading { (self.bold_font(11.5), c.text) } else { (self.ui_font(11.5), c.text_muted) };
                let sep = if i + 1 < lines.len() { "\n" } else { "" };
                job.append(&format!("{}{sep}", l.text), 0.0, egui::TextFormat { font_id, color: with_alpha(color, k), ..Default::default() });
            }
            ui.ctx().fonts_mut(|f| f.layout_job(job))
        });
        let mut h = 12.0 + 18.0;
        if let Some(g) = &sub_g {
            h += 4.0 + g.size().y;
        }
        if phase == Phase::Downloading {
            h += 10.0 + 4.0 + 8.0 + 14.0;
        }
        if let Some(g) = &notes_g {
            h += 10.0 + g.size().y.min(220.0) + 8.0 + 14.0;
        }
        if primary.is_some() || secondary.is_some() {
            h += 12.0 + 26.0;
        }
        h += 12.0;
        let h = self.tw.value("update-card-h", h, spec);

        let r = Rect::from_min_size(pos2(area.right() - 14.0 - w + (1.0 - k) * 40.0, area.bottom() - 14.0 - h), vec2(w, h));
        // плашка перекрывает редактор: клики по ней не уходят в текст
        let _bg = ui.interact(r, Id::unique("update-card"), Sense::click());
        if self.settings.ui.shadows {
            shadow(&p, r, radius, with_alpha(c.shadow, k * 0.9), 28);
        }
        p.rect_filled(r, cr(radius), with_alpha(c.popup, k));
        p.rect_stroke(r, cr(radius), Stroke::new(1.0, with_alpha(c.border_strong, k)), StrokeKind::Inside);
        let p = p.with_clip_rect(r);
        // тонкая полоска акцента сверху
        p.rect_filled(Rect::from_min_size(r.min + vec2(radius.max(6.0), 0.0), vec2(w - radius.max(6.0) * 2.0, 2.0)), cr(1.0), with_alpha(ic_col, 0.8 * k));

        let mut act = None;
        let x0 = r.left() + col_x;
        let mut y = r.top() + 12.0;
        // значок в кружке
        let ic_c = pos2(r.left() + pad + 8.0, y + 9.0);
        p.circle_filled(ic_c, 12.0, with_alpha(ic_col, 0.14 * k));
        if phase == Phase::Downloading || phase == Phase::Installing {
            // крутится, пока идёт работа
            let t = self.now as f32 * 3.0;
            let arc: Vec<egui::Pos2> = (0..=16)
                .map(|i| {
                    let a = t + i as f32 / 16.0 * 4.2;
                    ic_c + vec2(a.cos(), a.sin()) * 12.0
                })
                .collect();
            p.add(egui::Shape::line(arc, Stroke::new(1.5, with_alpha(ic_col, 0.8 * k))));
            self.tw.active = true;
        }
        icon(&p, Rect::from_center_size(ic_c, vec2(13.0, 13.0)), ic, with_alpha(ic_col, k), 1.4);
        text(&p, pos2(x0, y + 9.0), Align2::LEFT_CENTER, &title, title_font, with_alpha(c.text, k));

        // ✕ — убрать до следующего раза (значок останется в строке состояния)
        if phase != Phase::Installing {
            let xr = Rect::from_center_size(pos2(r.right() - 18.0, y + 9.0), vec2(22.0, 22.0));
            let resp = ui.interact(xr, Id::unique("update-close"), Sense::click());
            let ht = self.hover_t("update-close", resp.hovered());
            if ht > 0.01 {
                p.rect_filled(xr, cr(5.0), with_alpha(c.hover, ht * 1.6 * k));
            }
            icon(
                &p,
                Rect::from_center_size(xr.center(), vec2(10.0, 10.0)),
                Icon::Close,
                with_alpha(if resp.hovered() { c.text } else { c.text_faint }, k),
                1.3,
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.on_hover_text("Скрыть — значок останется в строке состояния").clicked() {
                act = Some(Act::Close);
            }
        }
        y += 18.0;

        if let Some(g) = sub_g {
            y += 4.0;
            let gh = g.size().y;
            p.galley(pos2(x0, y), g, with_alpha(c.text_muted, k));
            y += gh;
        }

        if phase == Phase::Downloading {
            let got = self.update.progress.got.load(Ordering::Relaxed);
            let total = self.update.progress.total.load(Ordering::Relaxed).max(1);
            let frac = (got as f32 / total as f32).clamp(0.0, 1.0);
            let frac = self.tw.value("update-progress", frac, self.spec(&self.settings.animations.panels.clone()));
            y += 10.0;
            let bar = Rect::from_min_size(pos2(x0, y), vec2(text_w, 4.0));
            p.rect_filled(bar, cr(2.0), with_alpha(c.text, 0.08 * k));
            p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * frac, 4.0)), cr(2.0), with_alpha(c.accent, k));
            y += 4.0 + 8.0;
            let label = format!("{} из {} · {:.0}%", fmt_mb(got), fmt_mb(total), frac * 100.0);
            text(&p, pos2(x0, y + 7.0), Align2::LEFT_CENTER, &label, self.ui_font(11.0), with_alpha(c.text_faint, k));
            y += 14.0;
        }

        if let Some(g) = notes_g {
            y += 10.0;
            let gh = g.size().y.min(220.0);
            p.with_clip_rect(Rect::from_min_size(pos2(x0, y), vec2(text_w, gh))).galley(pos2(x0, y), g, with_alpha(c.text, k));
            y += gh + 8.0;
            if self.card_link(ui, &p, pos2(x0, y + 7.0), Align2::LEFT_CENTER, "Открыть релиз на GitHub ↗", k) {
                act = Some(Act::Page);
            }
            y += 14.0;
        }

        if primary.is_some() || secondary.is_some() {
            y += 12.0;
            let mut bx = x0;
            for (label, is_primary, a) in [(primary, true, Act::Primary), (secondary, false, Act::Secondary)] {
                let Some(label) = label else { continue };
                let bw = text_width(ui.ctx(), label, &self.bold_font(11.5)) + 26.0;
                let b = Rect::from_min_size(pos2(bx, y), vec2(bw, 26.0));
                if self.card_button(ui, &p, b, label, is_primary, k) {
                    act = Some(a);
                }
                bx = b.right() + 8.0;
            }
            if notes_allowed {
                let label = if self.update.notes { "Свернуть" } else { "Что нового" };
                if bx + text_width(ui.ctx(), label, &self.ui_font(11.5)) < r.right() - pad
                    && self.card_link(ui, &p, pos2(r.right() - pad, y + 13.0), Align2::RIGHT_CENTER, label, k)
                {
                    act = Some(Act::Notes);
                }
            }
        }

        match act {
            Some(Act::Primary) => match phase {
                Phase::Ready(_) => self.update_restart(),
                Phase::Failed(_) => {
                    self.update.phase = crate::update::Phase::Available;
                    self.update_download();
                }
                _ => self.update_download(),
            },
            Some(Act::Secondary) => match phase {
                Phase::Downloading => self.update_cancel(),
                Phase::Failed(_) => self.update_open_page(),
                _ => self.update.hidden = true,
            },
            Some(Act::Close) => self.update.hidden = true,
            Some(Act::Notes) => self.update.notes = !self.update.notes,
            Some(Act::Page) => self.update_open_page(),
            None => {}
        }
        egui::lerp(area.bottom()..=r.top() + 6.0, k)
    }

    /// Значок обновления в строке состояния (справа налево от `rx`), пока плашка скрыта.
    pub(crate) fn draw_update_status(&mut self, ui: &mut egui::Ui, p: &Painter, rx: f32, rect: Rect) -> f32 {
        let Some(rel) = &self.update.release else { return rx };
        if self.update.card_visible() || matches!(self.update.phase, Phase::Idle | Phase::Checking) {
            return rx;
        }
        let c = self.colors.clone();
        let (ic, label, col) = match &self.update.phase {
            Phase::Downloading => {
                let got = self.update.progress.got.load(Ordering::Relaxed);
                let total = self.update.progress.total.load(Ordering::Relaxed).max(1);
                (Icon::Download, format!("Загрузка {:.0}%", got as f64 * 100.0 / total as f64), c.text_muted)
            }
            Phase::Ready(_) => (Icon::Refresh, format!("Перезапустить — Anvil {}", rel.version), c.accent),
            Phase::Installing => (Icon::Refresh, "Установка…".to_string(), c.text_muted),
            Phase::Failed(_) => (Icon::Warn, "Обновление не удалось".to_string(), c.warn),
            _ => (Icon::Download, format!("Anvil {}", rel.version), c.accent),
        };
        let font = self.ui_font(11.0);
        let cy = rect.center().y;
        let w = text_width(ui.ctx(), &label, &font) + 18.0;
        let zone = Rect::from_min_max(pos2(rx - w - 4.0, rect.top() + 2.0), pos2(rx + 4.0, rect.bottom() - 2.0));
        let resp = ui.interact(zone, Id::unique("status-update"), Sense::click());
        let bg = if resp.hovered() { c.hover } else { with_alpha(col, 0.10) };
        p.rect_filled(zone, cr(4.0), bg);
        icon(p, Rect::from_min_size(pos2(rx - w, cy - 6.0), vec2(12.0, 12.0)), ic, col, 1.3);
        text(p, pos2(rx, cy), Align2::RIGHT_CENTER, &label, font, col);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.on_hover_text("Показать плашку обновления").clicked() {
            self.update.hidden = false;
        }
        zone.left() - 12.0
    }
}
