//! Интерфейс LSP: список дополнений, всплывающее описание символа, панель «Проблемы».

use anvil_lsp::markdown::Block;
use eframe::egui::{self, Align2, CursorIcon, Event, FontId, Id, Key, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use egui::text::{LayoutJob, TextFormat};

use super::{Icon, cr, icon, paint_highlighted, shadow, text};
use crate::app::{AnvilApp, Focus};
use crate::fonts;
use crate::lsp::{Hover, kind_badge};
use crate::theme::{mix, with_alpha};

impl AnvilApp {
    /// Клавиши, которые забирает открытый список дополнений.
    pub(crate) fn completion_keys(&mut self, events: Vec<Event>) -> Vec<Event> {
        let mut rest = Vec::with_capacity(events.len());
        for ev in events {
            if let Event::Key { key, pressed: true, modifiers, .. } = &ev {
                let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.command;
                let n = self.lsp.completion.as_ref().map(|c| c.shown.len()).unwrap_or(0);
                if n > 0 && plain {
                    let handled = match key {
                        Key::ArrowDown => {
                            if let Some(c) = self.lsp.completion.as_mut() {
                                c.sel = (c.sel + 1) % n;
                            }
                            true
                        }
                        Key::ArrowUp => {
                            if let Some(c) = self.lsp.completion.as_mut() {
                                c.sel = (c.sel + n - 1) % n;
                            }
                            true
                        }
                        Key::PageDown => {
                            if let Some(c) = self.lsp.completion.as_mut() {
                                c.sel = (c.sel + 8).min(n - 1);
                            }
                            true
                        }
                        Key::PageUp => {
                            if let Some(c) = self.lsp.completion.as_mut() {
                                c.sel = c.sel.saturating_sub(8);
                            }
                            true
                        }
                        Key::Enter | Key::Tab if !modifiers.shift => {
                            self.completion_accept();
                            true
                        }
                        Key::Escape => {
                            self.lsp.completion = None;
                            true
                        }
                        _ => false,
                    };
                    if handled {
                        continue;
                    }
                }
            }
            rest.push(ev);
        }
        rest
    }

    /// После ввода: перефильтровать открытый список или запросить новый.
    pub(crate) fn after_typing(&mut self, doc_id: u64, typed: Option<char>, deleted: bool, navigated: bool) {
        if typed.is_some() || deleted || navigated {
            self.lsp.hover = None;
        }
        let open = self.lsp.completion.as_ref().is_some_and(|c| c.doc == doc_id);
        if navigated && !deleted && typed.is_none() {
            self.lsp.completion = None;
            return;
        }
        if open && deleted {
            self.completion_refilter();
            return;
        }
        let Some(ch) = typed else { return };
        if !self.settings.lsp.enabled || !self.settings.lsp.completion {
            return;
        }
        let word = anvil_core::text::is_word(ch);
        let trigger = self.lsp.docs.get(&doc_id).and_then(|dl| self.lsp.servers.get(&dl.server)).is_some_and(|s| s.triggers.iter().any(|t| t.ends_with(ch)));
        if open {
            if word {
                let incomplete = self.lsp.completion.as_ref().is_some_and(|c| c.incomplete);
                self.completion_refilter();
                if incomplete {
                    self.lsp.want_completion = Some((doc_id, self.now));
                }
                return;
            }
            self.lsp.completion = None;
        }
        if self.settings.lsp.completion_auto && (word || trigger) {
            self.lsp.want_completion = Some((doc_id, self.now));
        }
    }

    /// Задержка над символом → запрос описания; уход указателя → скрыть.
    pub(crate) fn hover_probe(&mut self, doc_id: u64, at: Option<(egui::Pos2, usize)>) {
        let over_popup = self
            .lsp
            .hover
            .as_ref()
            .and_then(|_| self.ctx.input(|i| i.pointer.hover_pos()))
            .is_some_and(|p| self.hover_rect.is_some_and(|r| r.expand(6.0).contains(p)));
        let Some((p, ch)) = at else {
            self.lsp.hover_probe = None;
            if !over_popup && self.lsp.hover.as_ref().is_some_and(|h| h.from_mouse) {
                self.lsp.hover = None;
            }
            return;
        };
        if !self.settings.lsp.hover || self.ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let moved = self.lsp.hover_probe.is_none_or(|(q, ..)| (q - p).length() > 4.0);
        if moved {
            self.lsp.hover_probe = Some((p, self.now, ch, false));
            if !over_popup && self.lsp.hover.as_ref().is_some_and(|h| h.from_mouse && (h.anchor - p).length() > 24.0) {
                self.lsp.hover = None;
            }
            self.ctx.request_repaint_after(std::time::Duration::from_millis(self.settings.lsp.hover_delay_ms as u64 + 10));
            return;
        }
        let Some((q, t, at, sent)) = self.lsp.hover_probe else { return };
        if sent || self.now - t < self.settings.lsp.hover_delay_ms as f64 / 1000.0 {
            return;
        }
        self.lsp.hover_probe = Some((q, t, at, true));
        let diags: Vec<(u8, String)> =
            self.doc_diags(doc_id).into_iter().filter(|(a, b, _, _)| at >= *a && at <= (*b).max(*a + 1)).map(|(_, _, s, m)| (s, m)).take(4).collect();
        self.lsp.hover = Some(Hover { doc: doc_id, at, blocks: Vec::new(), diags, anchor: q, shown_at: self.now, from_mouse: true });
        let _ = self.lsp_hover_at(doc_id, at);
    }

    /// Всплывающие окна поверх всего: дополнение и описание.
    pub(crate) fn draw_lsp_popups(&mut self, ui: &mut egui::Ui) {
        self.hover_rect = None;
        let Some(g) = self.geom else { return };
        if self.active.and_then(|i| self.docs.get(i)).is_none_or(|d| d.id != g.doc) {
            return;
        }
        self.draw_completion(ui, g);
        self.draw_hover(ui, g);
    }

    fn draw_completion(&mut self, ui: &mut egui::Ui, g: crate::lsp::EditorGeom) {
        let Some(comp) = self.lsp.completion.as_ref() else { return };
        if comp.doc != g.doc || comp.shown.is_empty() || self.focus != Focus::Editor {
            return;
        }
        let c = self.colors.clone();
        let Some(d) = self.docs.iter().find(|d| d.id == g.doc) else { return };
        let rope = d.ed.rope();
        let anchor = comp.anchor.min(rope.len_chars());
        let l = rope.char_to_line(anchor);
        let col = anvil_core::text::display_col(rope, anchor, g.tab);
        let ax = g.ox + col as f32 * g.char_w;
        let ay = g.oy + l as f32 * g.line_h;
        let row_h = (g.font_size * 1.75).round();
        let rows = comp.shown.len().min(10);
        let sel = comp.sel;
        let first = sel.saturating_sub(rows.saturating_sub(1)).min(comp.shown.len().saturating_sub(rows));
        let w = 440.0f32;
        let cur = comp.items[comp.shown[sel].0].clone();
        let doc_text: String = {
            let mut t = String::new();
            if !cur.detail.is_empty() {
                t.push_str(&cur.detail);
            }
            let body: String = cur.doc.lines().filter(|l| !l.trim_start().starts_with("```")).take(4).collect::<Vec<_>>().join(" ");
            if !body.trim().is_empty() {
                if !t.is_empty() {
                    t.push_str(" — ");
                }
                t.push_str(body.trim());
            }
            t.chars().take(180).collect()
        };
        let doc_h = if doc_text.is_empty() { 0.0 } else { 30.0 };
        let h = rows as f32 * row_h + 8.0 + doc_h;
        let screen = ui.max_rect();
        let below = ay + g.line_h + 2.0;
        let y = if below + h > screen.bottom() - 30.0 { ay - h - 2.0 } else { below };
        let x = (ax - 34.0).clamp(screen.left() + 4.0, screen.right() - w - 4.0);
        let k = self.tw.since(comp.opened, self.spec(&self.settings.animations.hover.clone()));
        let r = Rect::from_min_size(pos2(x, y + (1.0 - k) * -4.0), vec2(w, h));
        let p = ui.painter().clone();
        shadow(&p, r, self.settings.theme.radius, with_alpha(c.shadow, 0.9 * k), 22);
        p.rect_filled(r, cr(self.settings.theme.radius * 0.8), with_alpha(c.popup, k));
        p.rect_stroke(r, cr(self.settings.theme.radius * 0.8), Stroke::new(1.0, with_alpha(c.border_strong, k)), StrokeKind::Inside);
        let font = FontId::new((g.font_size - 0.5).max(8.0), fonts::editor_family());
        let small = FontId::new((g.font_size - 2.0).max(8.0), fonts::ui_family());
        let sel_y = self.tw.value(("comp-sel", comp.doc), (sel - first) as f32 * row_h, self.spec(&self.settings.animations.hover.clone()));
        let lp = p.with_clip_rect(r.shrink(1.0));
        lp.rect_filled(Rect::from_min_size(pos2(r.left() + 4.0, r.top() + 4.0 + sel_y), vec2(w - 8.0, row_h)), cr(5.0), with_alpha(c.accent, 0.14 * k));
        let mut clicked = None;
        let entries: Vec<(usize, Vec<usize>)> = comp.shown.iter().skip(first).take(rows).cloned().collect();
        for (i, (idx, hl)) in entries.iter().enumerate() {
            let it = &comp.items[*idx];
            let rr = Rect::from_min_size(pos2(r.left() + 4.0, r.top() + 4.0 + i as f32 * row_h), vec2(w - 8.0, row_h));
            let cy = rr.center().y;
            let (letter, class) = kind_badge(it.kind);
            let ci = anvil_config::SYNTAX_KEYS.iter().position(|k| *k == class).unwrap_or(0) as u8;
            let kc = c.syn(ci).color;
            let badge = Rect::from_center_size(pos2(rr.left() + 14.0, cy), vec2(18.0, 18.0));
            lp.rect_filled(badge, cr(4.0), with_alpha(kc, 0.16 * k));
            text(&lp, badge.center(), Align2::CENTER_CENTER, letter, FontId::new(10.5, fonts::bold_family()), with_alpha(kc, k));
            let label_col = if it.deprecated {
                c.text_faint
            } else if first + i == sel {
                c.text
            } else {
                c.text_muted
            };
            let lw = paint_highlighted(ui.ctx(), &lp, pos2(rr.left() + 30.0, cy), &it.label, hl, &font, with_alpha(label_col, k), with_alpha(c.accent, k));
            if it.deprecated {
                lp.line_segment([pos2(rr.left() + 30.0, cy), pos2(rr.left() + 30.0 + lw, cy)], Stroke::new(1.0, c.text_faint));
            }
            if !it.detail.is_empty() {
                let room = rr.right() - (rr.left() + 30.0 + lw) - 24.0;
                if room > 40.0 {
                    let mut dtl: String = it.detail.lines().next().unwrap_or_default().to_string();
                    let max_chars = (room / (small.size * 0.55)) as usize;
                    if dtl.chars().count() > max_chars {
                        dtl = dtl.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…";
                    }
                    text(&lp, pos2(rr.right() - 8.0, cy), Align2::RIGHT_CENTER, &dtl, small.clone(), with_alpha(c.text_faint, k));
                }
            }
            if ui.interact(rr, Id::unique(("comp-item", i)), Sense::click()).clicked() {
                clicked = Some(first + i);
            }
        }
        if !doc_text.is_empty() {
            let dr = Rect::from_min_max(pos2(r.left(), r.bottom() - doc_h), r.max);
            lp.line_segment([dr.left_top(), dr.right_top()], Stroke::new(1.0, with_alpha(c.border, k)));
            let gal = ui.ctx().fonts_mut(|f| f.layout(doc_text.clone(), small.clone(), c.text_faint, w - 24.0));
            lp.with_clip_rect(dr).galley(pos2(dr.left() + 12.0, dr.top() + 7.0), gal, with_alpha(c.text_faint, k));
        }
        if let Some(i) = clicked {
            if let Some(cm) = self.lsp.completion.as_mut() {
                cm.sel = i;
            }
            self.completion_accept();
        }
        if k < 1.0 {
            self.tw.active = true;
        }
    }

    fn draw_hover(&mut self, ui: &mut egui::Ui, g: crate::lsp::EditorGeom) {
        let Some(h) = self.lsp.hover.as_ref() else { return };
        if h.doc != g.doc || (h.blocks.is_empty() && h.diags.is_empty()) {
            return;
        }
        let c = self.colors.clone();
        let w = 560.0f32.min(ui.max_rect().width() - 20.0);
        let font = FontId::new((g.font_size - 1.0).max(8.0), fonts::editor_family());
        let ui_font = FontId::new(12.5, fonts::ui_family());
        let ctx = ui.ctx().clone();
        // готовим строки: (галерея, отступ сверху)
        let mut galleys = Vec::new();
        for (sev, msg) in &h.diags {
            let col = match sev {
                1 => c.err,
                2 => c.warn,
                _ => c.accent,
            };
            galleys.push((ctx.fonts_mut(|f| f.layout(msg.clone(), ui_font.clone(), col, w - 46.0)), true, *sev));
        }
        let blocks = h.blocks.clone();
        for b in blocks.iter().take(8) {
            match b {
                Block::Code { lang, text } => {
                    let lang = anvil_syntax::by_id(if lang.is_empty() { "rust" } else { lang.as_str() }).unwrap_or(&anvil_syntax::LANGUAGES[0]);
                    let spans = self.hl.highlight(lang, text);
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = w - 24.0;
                    let mut last = 0;
                    let fmt = |col| TextFormat { font_id: font.clone(), color: col, ..Default::default() };
                    for sp in &spans {
                        if sp.start > last {
                            job.append(&text[last..sp.start], 0.0, fmt(c.text));
                        }
                        job.append(&text[sp.start..sp.end.min(text.len())], 0.0, fmt(c.syn(sp.class).color));
                        last = sp.end.min(text.len());
                    }
                    if last < text.len() {
                        job.append(&text[last..], 0.0, fmt(c.text));
                    }
                    galleys.push((ctx.fonts_mut(|f| f.layout_job(job)), false, 0));
                }
                Block::Text(t) => galleys.push((ctx.fonts_mut(|f| f.layout(t.clone(), ui_font.clone(), c.text_muted, w - 24.0)), false, 0)),
                Block::Rule => {}
            }
        }
        if galleys.is_empty() {
            return;
        }
        let content_h: f32 = galleys.iter().map(|(g, _, _)| g.size().y + 8.0).sum::<f32>() + 12.0;
        let h_px = content_h.min(340.0);
        let width = galleys.iter().map(|(g, d, _)| g.size().x + if *d { 46.0 } else { 26.0 }).fold(200.0f32, f32::max).min(w);
        let screen = ui.max_rect();
        let anchor = if h.anchor == egui::Pos2::ZERO { self.cursor_screen_pos().unwrap_or(screen.center()) } else { h.anchor };
        let above = anchor.y - h_px - 10.0;
        let y = if above > screen.top() + 40.0 { above } else { anchor.y + g.line_h + 6.0 };
        let x = (anchor.x - 20.0).clamp(screen.left() + 4.0, screen.right() - width - 4.0);
        let k = self.tw.since(h.shown_at, self.spec(&self.settings.animations.hover.clone()));
        let r = Rect::from_min_size(pos2(x, y), vec2(width, h_px));
        self.hover_rect = Some(r);
        let p = ui.painter().clone();
        shadow(&p, r, self.settings.theme.radius, with_alpha(c.shadow, 0.9 * k), 22);
        p.rect_filled(r, cr(self.settings.theme.radius * 0.8), with_alpha(c.popup, k));
        p.rect_stroke(r, cr(self.settings.theme.radius * 0.8), Stroke::new(1.0, with_alpha(c.border_strong, k)), StrokeKind::Inside);
        let cp = p.with_clip_rect(r.shrink(2.0));
        let mut yy = r.top() + 10.0;
        let ndiag = h.diags.len();
        for (i, (gal, is_diag, sev)) in galleys.into_iter().enumerate() {
            let gh = gal.size().y;
            if is_diag {
                let (ic, col) = match sev {
                    1 => (Icon::Error, c.err),
                    2 => (Icon::Warn, c.warn),
                    _ => (Icon::Info, c.accent),
                };
                icon(&cp, Rect::from_min_size(pos2(r.left() + 12.0, yy + 1.0), vec2(15.0, 15.0)), ic, with_alpha(col, k), 1.3);
                cp.galley(pos2(r.left() + 34.0, yy), gal, col);
            } else {
                cp.galley(pos2(r.left() + 12.0, yy), gal, c.text);
            }
            yy += gh + 8.0;
            if i + 1 == ndiag && ndiag > 0 && !blocks.is_empty() {
                cp.line_segment([pos2(r.left() + 8.0, yy - 4.0), pos2(r.right() - 8.0, yy - 4.0)], Stroke::new(1.0, c.border));
            }
        }
        if k < 1.0 {
            self.tw.active = true;
        }
    }

    // ------------------------------------------------------------------ панель «Проблемы»

    pub(crate) fn draw_problems(&mut self, ui: &mut egui::Ui, body: Rect) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(body);
        let mut rows: Vec<(String, std::path::PathBuf, anvil_lsp::lsp_types::Position, u8, String)> = Vec::new();
        for (k, list) in &self.lsp.diags {
            let path = std::path::PathBuf::from(k);
            for g in list {
                rows.push((k.clone(), path.clone(), g.range.start, g.severity, g.message.lines().next().unwrap_or_default().to_string()));
            }
        }
        rows.sort_by(|a, b| a.3.cmp(&b.3).then_with(|| a.0.cmp(&b.0)).then_with(|| a.2.line.cmp(&b.2.line)));
        let font = self.ui_font(12.0);
        let small = self.ui_font(11.0);
        let row_h = 24.0;
        if rows.is_empty() {
            let msg = if self.lsp.servers.is_empty() {
                "Языковые серверы не запущены — откройте файл с кодом"
            } else {
                "Проблем нет ✓"
            };
            text(&p, pos2(body.left() + 16.0, body.top() + 18.0), Align2::LEFT_CENTER, msg, font, c.text_faint);
            return;
        }
        let visible = ((body.height() - 6.0) / row_h).floor().max(1.0) as usize;
        if ui.rect_contains_pointer(body) {
            let d = super::wheel_delta(ui.ctx()).y;
            self.problems_scroll = (self.problems_scroll - d / row_h).clamp(0.0, rows.len().saturating_sub(visible) as f32);
        }
        let start = (self.problems_scroll.round() as usize).min(rows.len().saturating_sub(visible));
        let mut open = None;
        for (i, (key, _, pos, sev, msg)) in rows.iter().enumerate().skip(start).take(visible) {
            let y = body.top() + 4.0 + (i - start) as f32 * row_h;
            let rr = Rect::from_min_size(pos2(body.left(), y), vec2(body.width(), row_h));
            let resp = ui.interact(rr, Id::unique(("problem", i)), Sense::click());
            let ht = self.hover_t(("problem-h", i), resp.hovered());
            if ht > 0.01 {
                p.rect_filled(rr, egui::CornerRadius::ZERO, with_alpha(c.hover, ht * 1.4));
            }
            let (ic, col) = match sev {
                1 => (Icon::Error, c.err),
                2 => (Icon::Warn, c.warn),
                _ => (Icon::Info, c.accent),
            };
            icon(&p, Rect::from_min_size(pos2(rr.left() + 14.0, rr.center().y - 7.0), vec2(14.0, 14.0)), ic, col, 1.3);
            let name = self
                .lsp
                .diag_src
                .get(key)
                .map(|(p, _)| p.as_path())
                .unwrap_or(std::path::Path::new(key))
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let tr = text(&p, pos2(rr.left() + 36.0, rr.center().y), Align2::LEFT_CENTER, msg, font.clone(), mix(c.text_muted, c.text, ht));
            text(
                &p,
                pos2(tr.right() + 12.0, rr.center().y),
                Align2::LEFT_CENTER,
                &format!("{name}:{}:{}", pos.line + 1, pos.character + 1),
                small.clone(),
                c.text_faint,
            );
            if resp.clicked() {
                open = Some((key.clone(), *pos));
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
        }
        if let Some((key, pos)) = open {
            let (path, enc) = self.lsp.diag_src.get(&key).cloned().unwrap_or_else(|| (std::path::PathBuf::from(&key), anvil_lsp::Enc::Utf16));
            self.goto_location(&path, pos, enc);
        }
    }
}
