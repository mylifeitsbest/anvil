//! Нижняя панель «Вывод»: результат запуска и установки тулчейнов.

use eframe::egui::{self, Align2, CursorIcon, FontId, Id, Rect, Sense, Stroke, pos2, vec2};

use super::{Icon, icon_button, text};
use crate::app::{AnvilApp, LineKind};
use crate::commands::Cmd;
use crate::fonts;

impl AnvilApp {
    pub(crate) fn draw_panel(&mut self, ui: &mut egui::Ui, rect: Rect, k: f32) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, egui::CornerRadius::ZERO, c.panel);
        p.line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, c.border));
        if k < 0.98 {
            self.tw.active = true;
        }
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), 32.0));
        let cy = head.center().y;
        let r = text(&p, pos2(head.left() + 14.0, cy), Align2::LEFT_CENTER, "ВЫВОД", self.bold_font(10.5), c.text);
        p.rect_filled(Rect::from_min_size(pos2(r.left(), head.bottom() - 3.0), vec2(r.width(), 1.5)), super::cr(1.0), c.accent);
        let mut x = r.right() + 16.0;
        if !self.panel.title.is_empty() {
            let t = text(&p, pos2(x, cy), Align2::LEFT_CENTER, &self.panel.title, self.ui_font(11.5), c.text_muted);
            x = t.right() + 12.0;
        }
        match (&self.panel.run, self.panel.exit) {
            (Some(_), _) => {
                let secs = self.now - self.panel.started;
                let a = 0.5 + 0.5 * ((self.now * 6.0) as f32).sin();
                p.circle_filled(pos2(x + 4.0, cy), 3.5, crate::theme::with_alpha(c.accent, 0.4 + 0.6 * a));
                text(&p, pos2(x + 13.0, cy), Align2::LEFT_CENTER, &format!("{secs:.1} с"), self.ui_font(11.0), c.text_faint);
            }
            (None, Some((code, _))) => {
                let ok = code == Some(0);
                p.circle_filled(pos2(x + 4.0, cy), 3.5, if ok { c.ok } else { c.err });
            }
            _ => {}
        }
        // кнопки справа
        let mut bx = head.right() - 36.0;
        let btn = |app: &mut AnvilApp, ui: &mut egui::Ui, bx: f32, key: &'static str, ic: Icon| -> bool {
            let br = Rect::from_min_size(pos2(bx, head.top() + 4.0), vec2(26.0, 24.0));
            let ht = app.hover_t(key, ui.rect_contains_pointer(br));
            let cc = app.colors.clone();
            icon_button(ui, br, Id::unique(key), ic, &cc, ht, false).clicked()
        };
        if btn(self, ui, bx, "panel-close", Icon::Close) {
            self.exec(Cmd::ViewPanel);
        }
        bx -= 30.0;
        if self.panel.run.is_some() {
            if btn(self, ui, bx, "panel-stop", Icon::Stop) {
                self.exec(Cmd::RunStop);
            }
        } else if btn(self, ui, bx, "panel-run", Icon::Play) {
            self.exec(Cmd::RunFile);
        }
        bx -= 30.0;
        if btn(self, ui, bx, "panel-copy", Icon::Files) {
            let all: Vec<String> = self.panel.lines.iter().map(|l| l.text.clone()).collect();
            ui.ctx().copy_text(all.join("\n"));
            self.status("Вывод скопирован");
        }

        // строки
        let body = Rect::from_min_max(pos2(rect.left(), head.bottom() + 2.0), rect.max);
        let font = FontId::new((self.settings.editor.font_size * 0.88).round(), fonts::editor_family());
        let lh = (font.size * 1.5).round();
        let bp = ui.painter().with_clip_rect(body);
        let n = self.panel.lines.len();
        let visible = (body.height() / lh).floor().max(1.0) as usize;
        if ui.rect_contains_pointer(body) {
            let d = super::wheel_delta(ui.ctx()).y;
            self.panel.scroll = (self.panel.scroll + d / lh).clamp(0.0, n.saturating_sub(visible) as f32);
            if ui.input(|i| i.pointer.hover_pos()).is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::Text);
            }
        }
        let end = n.saturating_sub(self.panel.scroll.round() as usize);
        let start = end.saturating_sub(visible);
        if n == 0 {
            let hint = match self.keymap.label(Cmd::RunFile) {
                Some(k) => format!("Нажмите {k}, чтобы собрать и запустить текущий файл"),
                None => "Запустите файл из палитры команд".into(),
            };
            text(&bp, pos2(body.left() + 14.0, body.top() + lh), Align2::LEFT_CENTER, &hint, self.ui_font(12.0), c.text_faint);
        }
        for (i, line) in self.panel.lines[start..end].iter().enumerate() {
            let col = match line.kind {
                LineKind::Out => c.text_muted,
                LineKind::Err => c.warn,
                LineKind::Info => c.accent,
                LineKind::Ok => c.ok,
                LineKind::Fail => c.err,
            };
            let shown: String = line.text.chars().take(600).collect();
            text(&bp, pos2(body.left() + 14.0, body.top() + 4.0 + i as f32 * lh + lh / 2.0), Align2::LEFT_CENTER, &shown, font.clone(), col);
        }
        let _ = ui.interact(body, Id::unique("panel-body"), Sense::hover());
    }
}
