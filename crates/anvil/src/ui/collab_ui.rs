//! Совместная работа в заголовке и строке состояния: «Поделиться», «В эфире», аватары участников.

use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Painter, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use super::{Icon, cr, icon, text, text_width};
use crate::app::AnvilApp;
use crate::collab::{Phase, color32, initial};
use crate::commands::Cmd;
use crate::theme::{mix, with_alpha};

/// Цвет текста поверх цветного фона.
pub fn on_color(c: Color32) -> Color32 {
    let l = 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
    if l > 150.0 { Color32::from_rgb(18, 18, 22) } else { Color32::WHITE }
}

impl AnvilApp {
    /// «● В эфире · 3» после названия проекта. Возвращает новую левую границу.
    pub(crate) fn draw_collab_live(&mut self, ui: &mut egui::Ui, p: &Painter, x: f32, cy: f32) -> f32 {
        let Some(s) = &self.collab.session else { return x };
        let c = self.colors.clone();
        let font = self.ui_font(11.5);
        let (label, dot) = match s.phase {
            Phase::Live => (format!("В эфире · {}", s.peers.len() + 1), c.ok),
            Phase::Connecting => ("Подключение…".to_string(), c.text_faint),
        };
        let w = text_width(ui.ctx(), &label, &font) + 22.0;
        let r = Rect::from_min_size(pos2(x, cy - 10.0), vec2(w, 20.0));
        let resp = ui.interact(r, Id::unique("title-live"), Sense::click());
        let ht = self.hover_t("title-live", resp.hovered());
        if ht > 0.01 {
            p.rect_filled(r, cr(self.settings.theme.radius * 0.7), with_alpha(c.hover, ht * 1.4));
        }
        p.circle_filled(pos2(r.left() + 9.0, cy), 3.5, dot);
        p.circle_stroke(pos2(r.left() + 9.0, cy), 6.0, Stroke::new(1.0, with_alpha(dot, 0.3)));
        text(p, pos2(r.left() + 19.0, cy), Align2::LEFT_CENTER, &label, font, c.text_muted);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.clicked() {
            self.exec(Cmd::CollabSession);
        }
        r.right() + 8.0
    }

    /// «Поделиться» или аватары участников справа в заголовке. Возвращает новую правую границу.
    pub(crate) fn draw_collab_title(&mut self, ui: &mut egui::Ui, p: &Painter, right: f32, cy: f32) -> f32 {
        let c = self.colors.clone();
        let radius = self.settings.theme.radius * 0.75;
        let Some(s) = &self.collab.session else {
            if !self.settings.collab.share_button {
                return right;
            }
            let label = "Поделиться";
            let w = text_width(ui.ctx(), label, &self.ui_font(11.5)) + 34.0;
            let b = Rect::from_min_size(pos2(right - w, cy - 12.0), vec2(w, 24.0));
            let resp = ui.interact(b, Id::unique("title-share"), Sense::click());
            let ht = self.hover_t("title-share", resp.hovered());
            p.rect_filled(b, cr(radius), with_alpha(c.accent, 0.13 + 0.09 * ht));
            p.rect_stroke(b, cr(radius), Stroke::new(1.0, with_alpha(c.accent, 0.25 + 0.25 * ht)), StrokeKind::Inside);
            icon(p, Rect::from_min_size(pos2(b.left() + 8.0, cy - 6.5), vec2(13.0, 13.0)), Icon::Users, c.accent, 1.2);
            text(p, pos2(b.left() + 25.0, cy), Align2::LEFT_CENTER, label, self.bold_font(11.5), c.accent);
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            let resp = resp.on_hover_text("Открыть сессию совместной работы: ссылка-приглашение скопируется в буфер обмена");
            if resp.clicked() {
                self.exec(Cmd::CollabShare);
            }
            return b.left() - 8.0;
        };
        // аватары: я и участники, внахлёст
        let me_name = crate::collab::my_name(&self.settings.collab);
        let mut people: Vec<(Option<u32>, String, Color32, &'static str)> =
            vec![(None, me_name.clone(), crate::collab::color32(crate::collab::my_color(&self.settings.collab, &me_name)), s.role.label())];
        for pv in s.peers.values() {
            people.push((Some(pv.info.id), pv.info.name.clone(), color32(pv.info.color), pv.info.role.label()));
        }

        // кнопка «Сессия»
        let label = "Сессия";
        let w = text_width(ui.ctx(), label, &self.ui_font(11.5)) + 34.0;
        let b = Rect::from_min_size(pos2(right - w, cy - 12.0), vec2(w, 24.0));
        let resp = ui.interact(b, Id::unique("title-session"), Sense::click());
        let ht = self.hover_t("title-session", resp.hovered());
        p.rect_filled(b, cr(radius), mix(c.hover, with_alpha(c.text, 0.09), ht));
        p.rect_stroke(b, cr(radius), Stroke::new(1.0, mix(c.border, c.border_strong, ht)), StrokeKind::Inside);
        icon(p, Rect::from_min_size(pos2(b.left() + 8.0, cy - 6.5), vec2(13.0, 13.0)), Icon::Users, c.text_muted, 1.2);
        text(p, pos2(b.left() + 25.0, cy), Align2::LEFT_CENTER, label, self.ui_font(11.5), c.text_muted);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let open = resp.clicked();
        let extra = people.len().saturating_sub(5);
        people.truncate(5);
        let d = 22.0;
        let step = d - 6.0;
        let mut x = b.left() - 10.0 - d / 2.0;
        let mut goto = None;
        if extra > 0 {
            let ctr = pos2(x, cy);
            p.circle_filled(ctr, d / 2.0, c.hover);
            p.circle_stroke(ctr, d / 2.0, Stroke::new(2.0, c.titlebar));
            text(p, ctr, Align2::CENTER_CENTER, &format!("+{extra}"), self.bold_font(9.5), c.text_muted);
            x -= step;
        }
        for (id, name, col, role) in people.into_iter().rev() {
            let ctr = pos2(x, cy);
            let r = Rect::from_center_size(ctr, vec2(d, d));
            let resp = ui.interact(r, Id::unique(("avatar", id)), Sense::click());
            p.circle_filled(ctr, d / 2.0, col);
            p.circle_stroke(ctr, d / 2.0 + 0.5, Stroke::new(2.0, c.titlebar));
            text(p, ctr, Align2::CENTER_CENTER, &initial(&name), self.bold_font(10.0), super::collab_ui::on_color(col));
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            let tip = if id.is_some() { format!("{name} · {role} — перейти к курсору") } else { format!("{name} (вы) · {role}") };
            if resp.on_hover_text(tip).clicked() {
                goto = Some(id);
            }
            x -= step;
        }
        let left = x + step - d / 2.0 - 8.0;
        if open {
            self.exec(Cmd::CollabSession);
        }
        match goto {
            Some(Some(id)) => self.collab_goto_peer(id),
            Some(None) => self.exec(Cmd::CollabSession),
            None => {}
        }
        left
    }

    /// «🔒 TLS · 👥 3 в сессии» в строке состояния (справа налево от `rx`). Возвращает новую правую границу.
    pub(crate) fn draw_collab_status(&mut self, ui: &mut egui::Ui, p: &Painter, rx: f32, rect: Rect) -> f32 {
        let Some(s) = &self.collab.session else { return rx };
        let c = self.colors.clone();
        let font = self.ui_font(11.0);
        let cy = rect.center().y;
        let readonly = self.doc().is_some_and(|d| d.ed.readonly);
        let count = s.peers.len() + 1;
        let live = s.phase == Phase::Live;
        let mut x = rx;
        let start = x;
        let mut item = |p: &Painter, ic: Icon, label: &str, col: Color32| {
            let w = text_width(ui.ctx(), label, &font);
            text(p, pos2(x, cy), Align2::RIGHT_CENTER, label, font.clone(), col);
            icon(p, Rect::from_min_size(pos2(x - w - 16.0, cy - 6.0), vec2(12.0, 12.0)), ic, col, 1.2);
            x -= w + 16.0 + 14.0;
        };
        if live {
            item(p, Icon::Users, &format!("{count} в сессии"), c.text_muted);
            item(p, Icon::Lock, "TLS", c.text_faint);
        } else {
            item(p, Icon::Users, "подключение…", c.text_faint);
        }
        if readonly {
            item(p, Icon::Lock, "только чтение", c.warn);
        }
        let zone = Rect::from_min_max(pos2(x + 8.0, rect.top() + 2.0), pos2(start + 4.0, rect.bottom() - 2.0));
        let resp = ui.interact(zone, Id::unique("status-collab"), Sense::click());
        if resp.hovered() {
            p.rect_filled(zone, cr(4.0), with_alpha(c.hover, 0.6));
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if resp.clicked() {
            self.exec(Cmd::CollabSession);
        }
        x - 2.0
    }

    /// Гость: список общих файлов хозяина в боковой панели.
    pub(crate) fn draw_session_files(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let Some(s) = &self.collab.session else { return };
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        let host = if s.host_name.is_empty() { "хозяин".to_string() } else { s.host_name.clone() };
        let active_local = self.active.and_then(|i| self.docs.get(i)).map(|d| d.id);
        let files: Vec<(u64, String, bool, bool, Vec<Color32>)> = s
            .docs
            .iter()
            .map(|(id, d)| {
                let viewers = s.peers.values().filter(|pv| pv.doc == Some(*id)).map(|pv| color32(pv.info.color)).collect();
                (*id, d.path.clone(), d.synced, d.local.is_some() && d.local == active_local, viewers)
            })
            .collect();
        let mut y = rect.top() + 10.0;
        text(&p, pos2(rect.left() + 14.0, y + 6.0), Align2::LEFT_CENTER, &format!("ФАЙЛЫ СЕССИИ · {host}"), self.bold_font(10.0), c.text_faint);
        y += 20.0;
        if files.is_empty() {
            let msg = if s.phase == Phase::Live { "хозяин ещё не открыл файлы" } else { "подключение…" };
            text(&p, pos2(rect.left() + 14.0, y + 10.0), Align2::LEFT_CENTER, msg, self.ui_font(11.5), c.text_faint);
            return;
        }
        let row_h = (24.0 * self.settings.ui.density.factor()).round();
        let font = self.ui_font(12.0);
        let small = self.ui_font(10.5);
        let mut open = None;
        for (id, path, synced, active, viewers) in files {
            let r = Rect::from_min_size(pos2(rect.left() + 6.0, y), vec2(rect.width() - 12.0, row_h));
            let resp = ui.interact(r, Id::unique(("session-file", id)), Sense::click());
            let ht = self.hover_t(("session-file", id), resp.hovered());
            if active {
                p.rect_filled(r, cr(self.settings.theme.radius * 0.7), with_alpha(c.accent, 0.12));
            } else if ht > 0.01 {
                p.rect_filled(r, cr(self.settings.theme.radius * 0.7), with_alpha(c.hover, ht * 1.4));
            }
            let (dir, name) = match path.rsplit_once('/') {
                Some((d, n)) => (d.to_string(), n.to_string()),
                None => (String::new(), path.clone()),
            };
            let cy = r.center().y;
            let bw = self.file_badge(&p, r.left() + 8.0, cy, &name);
            let col = if synced { if active { c.text } else { c.text_muted } } else { c.text_faint };
            let nr = text(&p, pos2(r.left() + 10.0 + bw, cy), Align2::LEFT_CENTER, &name, font.clone(), col);
            let mut x = nr.right() + 8.0;
            if !dir.is_empty() {
                let dr = text(&p, pos2(x, cy), Align2::LEFT_CENTER, &dir, small.clone(), c.text_faint);
                x = dr.right() + 8.0;
            }
            for (k, vc) in viewers.iter().take(4).enumerate() {
                p.circle_filled(pos2(x + 3.0 + k as f32 * 6.0, cy), 3.0, *vc);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if resp.clicked() {
                open = Some(id);
            }
            y += row_h;
        }
        if let Some(id) = open {
            self.collab_open_doc(id);
        }
    }
}
