//! Редактор кода: ввод, мышь, прокрутка, отрисовка текста, курсоры, миникарта, эффекты.

use anvil_config::{CurrentLine, CursorBlink, CursorStyle, LineNumbers, TypingEffect, Visibility, Whitespace};
use anvil_core::{EditKind, Move, text as ctext};
use eframe::egui::{self, Align2, Color32, CursorIcon, Event, FontId, Id, Key, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, pos2, vec2};
use egui::text::{LayoutJob, TextFormat};

use super::{cr, text};
use crate::anim::{ease, smooth_towards};
use crate::app::{AnvilApp, Focus, Particle};
use crate::doc::CaretAnim;
use crate::fonts;
use crate::theme::{mix, with_alpha};

fn caret_pos(a: &CaretAnim, now: f64, dur: f32) -> Pos2 {
    if dur <= 0.0 {
        return a.to;
    }
    let k = ease(anvil_config::Easing::OutCubic, ((now - a.start) as f32 / dur).clamp(0.0, 1.0));
    a.from + (a.to - a.from) * k
}

fn blink_value(kind: CursorBlink, t: f32, period: f32, timeout: f32) -> (f32, f32) {
    if kind == CursorBlink::Solid || t < 0.5 || (timeout > 0.0 && t > timeout) {
        return (1.0, 1.0);
    }
    let ph = ((t - 0.5) / period).fract();
    let wave = 0.5 + 0.5 * (ph * std::f32::consts::TAU).cos();
    match kind {
        CursorBlink::Blink => (if ph < 0.5 { 1.0 } else { 0.0 }, 1.0),
        CursorBlink::Smooth => {
            let x = ((wave - 0.15) / 0.7).clamp(0.0, 1.0);
            (x * x * (3.0 - 2.0 * x), 1.0)
        }
        CursorBlink::Phase => (wave.sqrt(), 1.0),
        CursorBlink::Expand => (1.0, 0.35 + 0.65 * wave),
        CursorBlink::Solid => (1.0, 1.0),
    }
}

fn rng(seed: &mut u64) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed % 10_000) as f32 / 10_000.0
}

impl AnvilApp {
    pub(crate) fn draw_editor_area(&mut self, ui: &mut egui::Ui, rect: Rect, events: &[Event], zen: bool) {
        if self.docs.is_empty() {
            self.draw_welcome(ui, rect);
            return;
        }
        let dens = self.settings.ui.density.factor();
        let tab_h = if zen { 0.0 } else { (self.settings.ui.tab_height * dens).round() };
        let crumbs_h = if zen || !self.settings.ui.breadcrumbs { 0.0 } else { (24.0 * dens).round() };
        let tabs = Rect::from_min_size(rect.min, vec2(rect.width(), tab_h));
        let crumbs = Rect::from_min_size(pos2(rect.left(), tabs.bottom()), vec2(rect.width(), crumbs_h));
        let ed = Rect::from_min_max(pos2(rect.left(), crumbs.bottom()), rect.max);
        if tab_h > 0.0 {
            self.draw_tabs(ui, tabs);
        }
        if crumbs_h > 0.0 {
            self.draw_breadcrumbs(ui, crumbs);
        }
        if self.active.is_some() {
            self.draw_editor(ui, ed, events, zen);
        }
    }

    fn draw_editor(&mut self, ui: &mut egui::Ui, rect: Rect, events: &[Event], zen: bool) {
        let Some(idx) = self.active else { return };
        if idx >= self.docs.len() {
            return;
        }
        let now = self.now;
        let dt = self.dt;
        let s = self.settings.clone();
        let c = self.colors.clone();
        let ctx = ui.ctx().clone();
        let focused = self.focus == Focus::Editor;
        {
            let (docs, hl) = (&mut self.docs, &mut self.hl);
            docs[idx].ensure_highlight(hl, true);
        }

        // ---------------------------------------------------------------- метрики
        let fs = s.editor.font_size.clamp(6.0, 72.0);
        let font = FontId::new(fs, fonts::editor_family());
        let (glyph_w, row_h) = ctx.fonts_mut(|f| (f.glyph_width(&font, '0'), f.row_height(&font)));
        let char_w = (glyph_w + s.editor.letter_spacing).max(1.0);
        let line_h = (fs * s.editor.line_height).round().max(row_h.ceil());
        let small = FontId::new((fs * 0.86).round(), fonts::editor_family());
        let tab = s.editor.tab_size.clamp(1, 16) as usize;

        let anim_on = s.animations.enabled;
        let caret_dur = if s.cursor.smooth && anim_on { s.cursor.smooth_ms as f32 / 1000.0 / s.animations.speed.max(0.05) } else { 0.0 };
        let trail_dur = if s.cursor.trail && anim_on { s.cursor.trail_ms as f32 / 1000.0 / s.animations.speed.max(0.05) } else { 0.0 };
        let sel_spec = self.spec(&s.animations.selection);
        let cl_spec = self.spec(&s.animations.current_line);

        let total = self.docs[idx].ed.rope().len_lines().max(1);
        let digits = (total as f32).log10().floor() as usize + 1;
        let gutter_w = match s.editor.line_numbers {
            LineNumbers::Off => 18.0,
            _ => digits.max(3) as f32 * glyph_w * 0.86 + 34.0,
        };
        let mm_w = if s.editor.minimap.enabled && !zen && rect.width() > 500.0 { s.editor.minimap.width.clamp(30.0, 200.0) } else { 0.0 };
        let text_rect = Rect::from_min_max(pos2(rect.left() + gutter_w, rect.top()), pos2(rect.right() - mm_w, rect.bottom()));
        let mut x0 = text_rect.left() + 8.0;
        if s.editor.max_width > 0 {
            let col_w = s.editor.max_width as f32 * char_w;
            if text_rect.width() > col_w + 60.0 {
                x0 = text_rect.center().x - col_w / 2.0;
            }
        }
        let gutter_right = x0 - 14.0;
        let top = rect.top() + s.editor.padding_top;
        let view_h = rect.height();
        let view_lines = (view_h / line_h).floor().max(1.0) as usize;
        let beyond = if s.editor.scroll_beyond_last_line { view_h - line_h * 2.0 } else { 0.0 };
        let max_y = ((total as f32) * line_h + beyond + s.editor.padding_top - view_h).max(0.0);

        // ---------------------------------------------------------------- мышь
        let edit_rect = Rect::from_min_max(rect.min, pos2(rect.right() - mm_w, rect.bottom()));
        let resp = ui.interact(edit_rect, Id::unique(("editor", self.docs[idx].id)), Sense::click_and_drag());
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let mods = ctx.input(|i| i.modifiers);
        let pos_at = |doc: &crate::doc::Document, p: Pos2| -> usize {
            let rope = doc.ed.rope();
            let line = (((p.y - top + doc.view.disp_y) / line_h).floor().max(0.0) as usize).min(rope.len_lines().saturating_sub(1));
            let col = ((p.x - x0 + doc.view.disp_x) / char_w).round().max(0.0) as usize;
            ctext::pos_for_col(rope, line, col, tab)
        };
        if resp.hovered() && pointer.is_some_and(|p| p.x > gutter_right) {
            ctx.set_cursor_icon(CursorIcon::Text);
        }
        let pressed = resp.hovered() && ctx.input(|i| i.pointer.primary_pressed());
        if pressed && let Some(pp) = pointer {
            self.focus = Focus::Editor;
            let doc = &mut self.docs[idx];
            let pos = pos_at(doc, pp);
            if mods.alt {
                doc.ed.toggle_cursor(pos);
            } else if mods.shift {
                doc.ed.extend_to(pos);
            } else {
                doc.ed.set_cursor(pos);
            }
            self.dragging = !mods.alt;
        }
        if self.dragging {
            if ctx.input(|i| i.pointer.primary_down()) {
                if let Some(pp) = pointer {
                    let doc = &mut self.docs[idx];
                    let pos = pos_at(doc, pp);
                    doc.ed.extend_to(pos);
                    // автопрокрутка при выделении за краем
                    if pp.y < rect.top() + 4.0 {
                        doc.view.scroll_y = (doc.view.scroll_y - line_h).max(0.0);
                    } else if pp.y > rect.bottom() - 4.0 {
                        doc.view.scroll_y = (doc.view.scroll_y + line_h).min(max_y);
                    }
                }
            } else {
                self.dragging = false;
            }
        }
        if resp.double_clicked()
            && let Some(pp) = pointer
        {
            let doc = &mut self.docs[idx];
            let pos = pos_at(doc, pp);
            doc.ed.select_word_at(pos);
            self.dragging = false;
        }
        if resp.triple_clicked() {
            self.docs[idx].ed.select_line();
            self.dragging = false;
        }
        if ui.rect_contains_pointer(rect) {
            let d = super::wheel_delta(&ctx);
            if d != egui::Vec2::ZERO {
                if mods.ctrl || mods.command {
                    self.exec(if d.y > 0.0 { crate::commands::Cmd::ViewZoomIn } else { crate::commands::Cmd::ViewZoomOut });
                } else {
                    let doc = &mut self.docs[idx];
                    let k = s.editor.scroll_sensitivity.clamp(0.1, 10.0);
                    let (dx, dy) = if mods.shift && d.x == 0.0 { (d.y, 0.0) } else { (d.x, d.y) };
                    doc.view.scroll_y = (doc.view.scroll_y - dy * k * 1.4).clamp(0.0, max_y);
                    doc.view.scroll_x = (doc.view.scroll_x - dx * k * 1.4).max(0.0);
                    doc.view.scrolled_at = now;
                }
            }
        }

        // ---------------------------------------------------------------- клавиатура
        if focused {
            let doc = &mut self.docs[idx];
            let ed = &mut doc.ed;
            let mut changed = false;
            for ev in events {
                match ev {
                    Event::Text(t) => {
                        if !t.is_empty() && !t.chars().all(char::is_control) {
                            ed.insert_text(t);
                            changed = true;
                        }
                    }
                    Event::Paste(t) => {
                        ed.paste(t);
                        changed = true;
                    }
                    Event::Copy => ctx.copy_text(ed.copy()),
                    Event::Cut => {
                        ctx.copy_text(ed.cut());
                        changed = true;
                    }
                    Event::Key { key, pressed: true, modifiers, .. } => {
                        let word = modifiers.ctrl || modifiers.command;
                        let ext = modifiers.shift;
                        changed = true;
                        match key {
                            Key::ArrowLeft => ed.move_cursors(if word { Move::WordLeft } else { Move::Left }, ext),
                            Key::ArrowRight => ed.move_cursors(if word { Move::WordRight } else { Move::Right }, ext),
                            Key::ArrowUp if !modifiers.alt => ed.move_cursors(Move::Up, ext),
                            Key::ArrowDown if !modifiers.alt => ed.move_cursors(Move::Down, ext),
                            Key::Home => ed.move_cursors(if word { Move::DocStart } else { Move::Home }, ext),
                            Key::End => ed.move_cursors(if word { Move::DocEnd } else { Move::End }, ext),
                            Key::PageUp => {
                                ed.move_cursors(Move::PageUp(view_lines.saturating_sub(1)), ext);
                                doc.view.scroll_y = (doc.view.scroll_y - view_h + line_h).max(0.0);
                            }
                            Key::PageDown => {
                                ed.move_cursors(Move::PageDown(view_lines.saturating_sub(1)), ext);
                                doc.view.scroll_y = (doc.view.scroll_y + view_h - line_h).min(max_y);
                            }
                            Key::Backspace => {
                                if word {
                                    ed.delete_word(false)
                                } else {
                                    ed.backspace()
                                }
                            }
                            Key::Delete => {
                                if word {
                                    ed.delete_word(true)
                                } else {
                                    ed.delete_forward()
                                }
                            }
                            Key::Enter if !word => ed.newline(),
                            Key::Tab if !word => {
                                if ext {
                                    ed.indent_lines(true)
                                } else {
                                    ed.tab()
                                }
                            }
                            Key::Escape => {
                                if self.find.open {
                                    self.find.open = false;
                                } else {
                                    ed.collapse();
                                }
                            }
                            _ => changed = false,
                        }
                    }
                    _ => {}
                }
            }
            if changed {
                doc.view.reveal = true;
                doc.view.activity = now;
            }
        }

        // ---------------------------------------------------------------- прокрутка к курсору и анимации
        let doc = &mut self.docs[idx];
        let rope = doc.ed.rope().clone();
        let sel = doc.ed.sel.clone();
        if doc.view.reveal {
            let head = sel.primary().head.min(rope.len_chars());
            let l = rope.char_to_line(head);
            let col = ctext::display_col(&rope, head, tab);
            let y = l as f32 * line_h;
            let margin = (line_h * 3.0).min(view_h / 3.0);
            if doc.view.center {
                doc.view.scroll_y = y - view_h / 2.0;
            } else if y < doc.view.scroll_y + margin {
                doc.view.scroll_y = y - margin;
            } else if y + line_h > doc.view.scroll_y + view_h - margin {
                doc.view.scroll_y = y + line_h - view_h + margin;
            }
            doc.view.scroll_y = doc.view.scroll_y.clamp(0.0, max_y);
            let x = col as f32 * char_w;
            let vis_w = (text_rect.right() - x0 - 24.0).max(40.0);
            if x < doc.view.scroll_x + 20.0 {
                doc.view.scroll_x = (x - 60.0).max(0.0);
            } else if x > doc.view.scroll_x + vis_w {
                doc.view.scroll_x = x - vis_w + 60.0;
            }
            doc.view.reveal = false;
            doc.view.center = false;
        }
        doc.view.scroll_y = doc.view.scroll_y.clamp(0.0, max_y);
        let tau =
            if s.animations.smooth_scroll && anim_on { s.animations.scroll_smoothing_ms as f32 / 1000.0 / 3.0 / s.animations.speed.max(0.05) } else { 0.0 };
        doc.view.disp_y = smooth_towards(doc.view.disp_y, doc.view.scroll_y, dt, tau);
        doc.view.disp_x = smooth_towards(doc.view.disp_x, doc.view.scroll_x, dt, tau);
        if doc.view.disp_y != doc.view.scroll_y || doc.view.disp_x != doc.view.scroll_x {
            self.tw.active = true;
        }

        // курсоры: цели в координатах содержимого
        let targets: Vec<Pos2> = sel
            .list
            .iter()
            .map(|sl| {
                let h = sl.head.min(rope.len_chars());
                let l = rope.char_to_line(h);
                pos2(ctext::display_col(&rope, h, tab) as f32 * char_w, l as f32 * line_h)
            })
            .collect();
        if doc.view.carets.len() != targets.len() {
            doc.view.carets = targets.iter().map(|t| CaretAnim { from: *t, to: *t, start: now, trail_from: *t, trail_start: -10.0 }).collect();
            doc.view.activity = now;
        } else {
            for (a, t) in doc.view.carets.iter_mut().zip(&targets) {
                if a.to != *t {
                    let cur = caret_pos(a, now, caret_dur);
                    if (cur - *t).length() > char_w * 1.5 {
                        a.trail_from = cur;
                        a.trail_start = now;
                    }
                    a.from = cur;
                    a.to = *t;
                    a.start = now;
                    doc.view.activity = now;
                }
            }
        }
        if doc.view.last_sel.as_ref() != Some(&sel) {
            doc.view.sel_changed = now;
            doc.view.last_sel = Some(sel.clone());
            doc.view.activity = now;
        }

        // эффекты ввода
        let serial = doc.ed.last.serial;
        let mut spawn: Vec<Pos2> = Vec::new();
        if serial != doc.view.last_serial {
            doc.view.last_serial = serial;
            doc.view.activity = now;
            if doc.ed.last.kind == Some(EditKind::Insert) && s.animations.typing.effect != TypingEffect::None && anim_on {
                for &p in doc.ed.last.positions.iter().take(8) {
                    let p = p.min(rope.len_chars());
                    let l = rope.char_to_line(p);
                    spawn.push(pos2(ctext::display_col(&rope, p, tab) as f32 * char_w, l as f32 * line_h));
                }
            }
        }
        let disp_y = doc.view.disp_y;
        let disp_x = doc.view.disp_x;
        let carets = doc.view.carets.clone();
        let activity = doc.view.activity;
        let sel_changed = doc.view.sel_changed;
        let large = doc.large;
        let doc_id = doc.id;

        let shake_off = {
            let age = (now - self.shake.0) as f32;
            if age < 0.12 && s.animations.typing.shake {
                self.tw.active = true;
                let mut seed = (now * 1e6) as u64 | 1;
                let k = (1.0 - age / 0.12) * 2.0 * s.animations.typing.intensity;
                vec2((rng(&mut seed) - 0.5) * k, (rng(&mut seed) - 0.5) * k)
            } else {
                egui::Vec2::ZERO
            }
        };
        let ox = x0 - disp_x + shake_off.x;
        let oy = top - disp_y + shake_off.y;
        if !spawn.is_empty() {
            let mut seed = (now * 1e6) as u64 | 7;
            let n = (6.0 * s.animations.typing.intensity).round().clamp(1.0, 40.0) as usize;
            for cp in &spawn {
                let at = pos2(ox + cp.x, oy + cp.y + line_h * 0.5);
                match s.animations.typing.effect {
                    TypingEffect::Sparks => {
                        for _ in 0..n {
                            let a = rng(&mut seed) * std::f32::consts::PI + std::f32::consts::PI;
                            let v = 60.0 + rng(&mut seed) * 120.0;
                            let col = if rng(&mut seed) > 0.4 { c.accent } else { mix(c.accent, Color32::WHITE, 0.5) };
                            self.particles.push(Particle {
                                pos: at,
                                vel: vec2(a.cos() * v, a.sin() * v),
                                born: now,
                                life: 0.35 + rng(&mut seed) * 0.3,
                                color: col,
                                size: 1.2 + rng(&mut seed) * 1.6,
                                kind: 0,
                            });
                        }
                    }
                    TypingEffect::Pop => {
                        self.particles.push(Particle { pos: at, vel: egui::Vec2::ZERO, born: now, life: 0.32, color: c.accent, size: line_h * 0.9, kind: 1 })
                    }
                    TypingEffect::Glow => {
                        self.particles.push(Particle { pos: at, vel: egui::Vec2::ZERO, born: now, life: 0.45, color: c.accent, size: line_h * 1.6, kind: 2 })
                    }
                    TypingEffect::None => {}
                }
            }
            if s.animations.typing.shake {
                self.shake = (now, 1.0);
            }
            if self.particles.len() > 600 {
                let cut = self.particles.len() - 600;
                self.particles.drain(..cut);
            }
        }

        // ---------------------------------------------------------------- отрисовка
        let p = ui.painter().with_clip_rect(rect);
        let tp = ui.painter().with_clip_rect(Rect::from_min_max(pos2(gutter_right + 4.0, rect.top()), pos2(rect.right() - mm_w, rect.bottom())));
        let first = ((disp_y - s.editor.padding_top) / line_h).floor().max(0.0) as usize;
        let last = (first + view_lines + 2).min(total);
        let line_y = |l: usize| oy + l as f32 * line_h;
        let doc = &self.docs[idx];
        let spans = &doc.spans;
        let brackets = &doc.brackets;

        // текущая строка
        let cursor_lines: Vec<usize> = {
            let mut v: Vec<usize> = sel.list.iter().map(|sl| rope.char_to_line(sl.head.min(rope.len_chars()))).collect();
            v.dedup();
            v.truncate(64);
            v
        };
        if s.editor.current_line != CurrentLine::None && (focused || cursor_lines.len() == 1) {
            let single = cursor_lines.len() == 1;
            for &l in &cursor_lines {
                let yc = if single { self.tw.value(("curline", doc_id), l as f32 * line_h, cl_spec) } else { l as f32 * line_h };
                let y = oy + yc;
                let full_row = Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right() - mm_w, y + line_h));
                match s.editor.current_line {
                    CurrentLine::Fill => {
                        p.rect_filled(full_row, egui::CornerRadius::ZERO, c.current_line);
                    }
                    CurrentLine::Border => {
                        p.rect_stroke(full_row.shrink2(vec2(2.0, 0.5)), cr(3.0), Stroke::new(1.0, c.border_strong), StrokeKind::Inside);
                    }
                    CurrentLine::Gutter => {
                        p.rect_filled(Rect::from_min_max(full_row.min, pos2(gutter_right + 6.0, full_row.max.y)), egui::CornerRadius::ZERO, c.current_line);
                    }
                    CurrentLine::None => {}
                }
            }
        }

        // направляющие отступов
        if s.editor.indent_guides || s.editor.rainbow_indent {
            let lvl = |l: usize| -> Option<usize> {
                let t = ctext::line_text(&rope, l);
                if t.trim().is_empty() {
                    None
                } else {
                    let mut col = 0;
                    for ch in t.chars() {
                        match ch {
                            ' ' => col += 1,
                            '\t' => col += tab - col % tab,
                            _ => break,
                        }
                    }
                    Some(col / tab)
                }
            };
            let mut levels: Vec<usize> = Vec::with_capacity(last - first);
            let mut prev = (first.saturating_sub(40)..first).rev().find_map(&lvl).unwrap_or(0);
            for l in first..last {
                let v = match lvl(l) {
                    Some(v) => v,
                    None => {
                        let next = (l + 1..(l + 40).min(total)).find_map(&lvl).unwrap_or(0);
                        prev.min(next.max(prev.min(next)))
                    }
                };
                if let Some(real) = lvl(l) {
                    prev = real;
                }
                levels.push(v);
            }
            let cur_line = cursor_lines.first().copied().unwrap_or(0);
            let active_level = if cur_line >= first && cur_line < last { levels[cur_line - first] } else { 0 };
            let (mut a0, mut a1) = (cur_line, cur_line);
            if active_level > 0 {
                while a0 > first && levels[a0 - 1 - first] >= active_level {
                    a0 -= 1;
                }
                while a1 + 1 < last && levels[a1 + 1 - first] >= active_level {
                    a1 += 1;
                }
            }
            for (k, l) in (first..last).enumerate() {
                let y = line_y(l);
                for g in 0..levels[k] {
                    let x = ox + (g * tab) as f32 * char_w;
                    if s.editor.rainbow_indent && !c.rainbow.is_empty() {
                        tp.rect_filled(
                            Rect::from_min_size(pos2(x, y), vec2(tab as f32 * char_w, line_h)),
                            egui::CornerRadius::ZERO,
                            c.rainbow[g % c.rainbow.len()],
                        );
                    }
                    if s.editor.indent_guides {
                        let active = g + 1 == active_level && l >= a0 && l <= a1;
                        tp.line_segment(
                            [pos2(x.round() + 0.5, y), pos2(x.round() + 0.5, y + line_h)],
                            Stroke::new(1.0, if active { c.indent_guide_active } else { c.indent_guide }),
                        );
                    }
                }
            }
        }

        // результаты поиска
        let first_char = rope.line_to_char(first);
        let last_char = if last >= total { rope.len_chars() } else { rope.line_to_char(last) };
        let seg_rect = |a: usize, b: usize, extend_nl: bool| -> Vec<Rect> {
            let mut out = Vec::new();
            let a = a.min(rope.len_chars());
            let b = b.min(rope.len_chars());
            let la = rope.char_to_line(a);
            let lb = rope.char_to_line(b);
            for l in la.max(first)..=lb.min(last.saturating_sub(1)) {
                let ls = rope.line_to_char(l);
                let ca = if l == la { ctext::display_col(&rope, a, tab) } else { 0 };
                let cb = if l == lb {
                    ctext::display_col(&rope, b, tab) as f32
                } else {
                    ctext::display_col(&rope, ls + ctext::line_len(&rope, l), tab) as f32 + if extend_nl { 0.6 } else { 0.0 }
                };
                let y = line_y(l);
                out.push(Rect::from_min_max(pos2(ox + ca as f32 * char_w, y + 1.0), pos2(ox + cb * char_w, y + line_h - 1.0)));
            }
            out
        };
        if !doc.view.find_matches.is_empty() {
            let m = &doc.view.find_matches;
            let start = m.partition_point(|r| r.1 < first_char);
            let prim = sel.primary();
            for &(a, b) in m[start..].iter().take_while(|r| r.0 <= last_char).take(2000) {
                let active = prim.start() == a && prim.end() == b;
                for r in seg_rect(a, b, false) {
                    tp.rect_filled(r, cr(3.0), if active { c.find_match_active } else { c.find_match });
                }
            }
        }

        // вхождения выделенного слова
        let prim = sel.primary();
        if s.editor.highlight_selection_matches && !prim.is_empty() && prim.len() <= 100 && rope.char_to_line(prim.start()) == rope.char_to_line(prim.end()) {
            let needle = rope.slice(prim.start()..prim.end()).to_string();
            if needle.trim().len() >= 2 {
                for l in first..last {
                    let lt = ctext::line_text(&rope, l);
                    let ls = rope.line_to_char(l);
                    let mut from = 0;
                    while let Some(bi) = lt[from..].find(&needle) {
                        let byte = from + bi;
                        let ca = ls + lt[..byte].chars().count();
                        if ca != prim.start() {
                            for r in seg_rect(ca, ca + needle.chars().count(), false) {
                                tp.rect_filled(r, cr(3.0), with_alpha(c.selection, 0.45));
                                tp.rect_stroke(r, cr(3.0), Stroke::new(1.0, with_alpha(c.accent, 0.35)), StrokeKind::Inside);
                            }
                        }
                        from = byte + needle.len().max(1);
                        if from >= lt.len() {
                            break;
                        }
                    }
                }
            }
        }

        // выделения
        let sel_k = self.tw.since(sel_changed, sel_spec);
        let sel_col = with_alpha(c.selection, 0.55 + 0.45 * sel_k);
        let sel_col = if focused { sel_col } else { with_alpha(sel_col, 0.6) };
        for sl in &sel.list {
            if sl.is_empty() || sl.end() < first_char || sl.start() > last_char {
                continue;
            }
            for r in seg_rect(sl.start(), sl.end(), true) {
                tp.rect_filled(r.expand2(vec2(0.0, 1.0)), cr(3.0), sel_col);
            }
        }

        // парная скобка
        if s.editor.highlight_matching_brackets && sel.len() == 1 && !brackets.is_empty() {
            let head = prim.head.min(rope.len_chars());
            let hb = rope.char_to_byte(head);
            let find = |b: usize| brackets.binary_search_by_key(&b, |x| x.pos).ok();
            let hit = find(hb).or_else(|| if hb > 0 { find(hb - 1) } else { None });
            if let Some(i) = hit
                && let Some(j) = brackets[i].partner
            {
                for k in [i, j as usize] {
                    let bp = rope.byte_to_char(brackets[k].pos);
                    if bp >= first_char && bp <= last_char {
                        for r in seg_rect(bp, bp + 1, false) {
                            tp.rect_filled(r, cr(3.0), c.bracket_match);
                            tp.rect_stroke(r, cr(3.0), Stroke::new(1.0, with_alpha(c.text_faint, 0.6)), StrokeKind::Inside);
                        }
                    }
                }
            }
        }

        // текст
        let text_w = text_rect.right() - x0;
        let col_lo = (disp_x / char_w).floor().max(0.0) as usize;
        let col_lo = col_lo.saturating_sub(2);
        let col_hi = col_lo + (text_w / char_w).ceil() as usize + 6;
        let ws_mode = s.editor.render_whitespace;
        let comment_class = anvil_syntax::CLASS_NAMES.iter().position(|n| *n == "comment").unwrap_or(0) as u8;
        let mut tag_cache: std::collections::HashMap<usize, Option<Color32>> = std::collections::HashMap::new();
        let tag_for = |si: usize, cache: &mut std::collections::HashMap<usize, Option<Color32>>| -> Option<Color32> {
            if !s.editor.comment_tags {
                return None;
            }
            *cache.entry(si).or_insert_with(|| {
                let sp = spans[si];
                let a = rope.byte_to_char(sp.start.min(rope.len_bytes()));
                let snippet: String = rope.chars_at(a).take(48).collect();
                // после маркера комментария: «// TODO …», «# FIXME», «// ! важно»; «//!» — док-комментарий, а не метка
                let after = snippet.trim_start_matches(['/', '#', '*', '-', ';', '<', '%']);
                let spaced = after.starts_with([' ', '\t']);
                let body = after.trim_start();
                c.tags
                    .iter()
                    .find(|(tag, _)| {
                        let word = tag.chars().all(char::is_alphanumeric);
                        body.starts_with(tag.as_str()) && (word || (spaced && body[tag.len()..].starts_with([' ', '\t'])))
                    })
                    .map(|(_, col)| *col)
            })
        };
        for l in first..last {
            let y = line_y(l);
            let ls_byte = rope.line_to_byte(l);
            let mut si = spans.partition_point(|sp| sp.end <= ls_byte);
            let mut bi = brackets.partition_point(|b| b.pos < ls_byte);
            let line = rope.line(l);
            let line_len = ctext::line_len(&rope, l);
            let (lead_end, trail_start) = if ws_mode == Whitespace::Boundary {
                let t: String = line.chars().take(line_len).collect();
                let lead = t.chars().take_while(|c| c.is_whitespace()).count();
                let trail = t.trim_end().chars().count();
                (lead, trail)
            } else {
                (0, usize::MAX)
            };
            let mut job = LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            let mut run = String::new();
            let mut run_style: Option<(Color32, bool, bool)> = None;
            let mut first_col: Option<usize> = None;
            let mut dc = 0usize;
            let mut byte = ls_byte;
            let flush = |job: &mut LayoutJob, run: &mut String, st: Option<(Color32, bool, bool)>| {
                if let Some((col, it, un)) = st
                    && !run.is_empty()
                {
                    job.append(
                        run,
                        0.0,
                        TextFormat {
                            font_id: font.clone(),
                            color: col,
                            italics: it,
                            underline: if un { Stroke::new(1.0, col) } else { Stroke::NONE },
                            extra_letter_spacing: s.editor.letter_spacing,
                            ..Default::default()
                        },
                    );
                }
                run.clear();
            };
            for (ci, ch) in line.chars().enumerate() {
                if ch == '\n' {
                    break;
                }
                let blen = ch.len_utf8();
                while si < spans.len() && spans[si].end <= byte {
                    si += 1;
                }
                while bi < brackets.len() && brackets[bi].pos < byte {
                    bi += 1;
                }
                let w = if ch == '\t' { tab - dc % tab } else { 1 };
                if dc + w > col_lo && dc <= col_hi {
                    let class = if si < spans.len() && spans[si].start <= byte { spans[si].class } else { 0 };
                    let st = c.syn(class);
                    let mut col = st.color;
                    if class == comment_class
                        && si < spans.len()
                        && spans[si].start <= byte
                        && let Some(tc) = tag_for(si, &mut tag_cache)
                    {
                        col = tc;
                    }
                    if s.editor.bracket_colorization && !c.brackets.is_empty() && bi < brackets.len() && brackets[bi].pos == byte {
                        col = c.brackets[brackets[bi].depth as usize % c.brackets.len()];
                    }
                    let show_ws = match ws_mode {
                        Whitespace::None => false,
                        Whitespace::All => true,
                        Whitespace::Boundary => ci < lead_end || ci >= trail_start,
                    };
                    let (piece, style): (String, (Color32, bool, bool)) = match ch {
                        '\t' if show_ws => (format!("→{}", " ".repeat(w - 1)), (with_alpha(c.text_faint, 0.6), false, false)),
                        '\t' => (" ".repeat(w), (col, false, false)),
                        ' ' if show_ws => ("·".into(), (with_alpha(c.text_faint, 0.6), false, false)),
                        _ => (ch.to_string(), (col, st.italic, st.underline)),
                    };
                    if first_col.is_none() {
                        first_col = Some(dc);
                    }
                    if run_style != Some(style) {
                        flush(&mut job, &mut run, run_style);
                        run_style = Some(style);
                    }
                    run.push_str(&piece);
                } else if dc > col_hi {
                    break;
                }
                dc += w;
                byte += blen;
            }
            flush(&mut job, &mut run, run_style);
            if let Some(fc) = first_col {
                let galley = ctx.fonts_mut(|f| f.layout_job(job));
                tp.galley(pos2(ox + fc as f32 * char_w, y + (line_h - row_h) / 2.0), galley, c.text);
            }
        }

        // курсоры
        let period = s.cursor.blink_interval_ms.max(200) as f32 / 1000.0;
        let (blink_a, blink_h) = if focused && self.window_focused {
            blink_value(s.cursor.blink, (now - activity) as f32, period, s.cursor.blink_timeout_s as f32)
        } else {
            (0.45, 1.0)
        };
        for a in &carets {
            let cp = caret_pos(a, now, caret_dur);
            if (now - a.start) < caret_dur as f64 {
                self.tw.active = true;
            }
            let hx = ox + cp.x;
            let hy = oy + cp.y;
            if hy + line_h < rect.top() || hy > rect.bottom() {
                continue;
            }
            // шлейф
            if trail_dur > 0.0 {
                let k = ((now - a.trail_start) as f32 / trail_dur).clamp(0.0, 1.0);
                if k < 1.0 {
                    self.tw.active = true;
                    let tail = a.trail_from + (cp - a.trail_from) * ease(anvil_config::Easing::OutCubic, k);
                    let (tx, ty) = (ox + tail.x, oy + tail.y);
                    let alpha = (1.0 - k) * 0.45;
                    let mut mesh = Mesh::default();
                    let (inset_t, inset_h) = (line_h * 0.22, line_h * 0.78);
                    mesh.colored_vertex(pos2(tx, ty + inset_t + 3.0), with_alpha(c.caret, 0.0));
                    mesh.colored_vertex(pos2(tx, ty + inset_h - 3.0), with_alpha(c.caret, 0.0));
                    mesh.colored_vertex(pos2(hx, hy + inset_t), with_alpha(c.caret, alpha));
                    mesh.colored_vertex(pos2(hx, hy + inset_h), with_alpha(c.caret, alpha));
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(1, 2, 3);
                    tp.add(Shape::mesh(mesh));
                }
            }
            let cw = s.cursor.width.clamp(1.0, 8.0);
            let h = (line_h - 6.0) * blink_h;
            let cy = hy + (line_h - h) / 2.0;
            let r = match s.cursor.style {
                CursorStyle::Line => Rect::from_min_size(pos2(hx - cw / 2.0, cy), vec2(cw, h)),
                CursorStyle::Block | CursorStyle::Hollow => Rect::from_min_size(pos2(hx, cy), vec2(char_w, h)),
                CursorStyle::Underline => Rect::from_min_size(pos2(hx, hy + line_h - 4.0), vec2(char_w, 2.0)),
            };
            if s.cursor.glow && blink_a > 0.05 {
                super::shadow(&tp, r, 2.0, with_alpha(c.caret, 0.45 * blink_a), s.cursor.glow_radius.clamp(0.0, 40.0) as u8);
            }
            match s.cursor.style {
                CursorStyle::Hollow => {
                    tp.rect_stroke(r, cr(2.0), Stroke::new(1.5, with_alpha(c.caret, blink_a)), StrokeKind::Inside);
                }
                CursorStyle::Block => {
                    tp.rect_filled(r, cr(2.0), with_alpha(c.caret, blink_a * 0.6));
                }
                _ => {
                    tp.rect_filled(r, cr(1.0), with_alpha(c.caret, blink_a));
                }
            }
        }

        // частицы эффектов ввода
        if !self.particles.is_empty() {
            self.tw.active = true;
            for pt in &mut self.particles {
                pt.pos += pt.vel * dt;
                pt.vel.y += 320.0 * dt;
                pt.vel *= 0.96;
            }
            self.particles.retain(|pt| ((now - pt.born) as f32) < pt.life);
            for pt in &self.particles {
                let k = ((now - pt.born) as f32 / pt.life).clamp(0.0, 1.0);
                match pt.kind {
                    0 => {
                        tp.circle_filled(pt.pos, pt.size * (1.0 - k * 0.5), with_alpha(pt.color, 1.0 - k));
                    }
                    1 => {
                        tp.circle_stroke(
                            pt.pos,
                            pt.size * (0.3 + ease(anvil_config::Easing::OutCubic, k)),
                            Stroke::new(1.5, with_alpha(pt.color, (1.0 - k) * 0.8)),
                        );
                    }
                    _ => {
                        super::shadow(
                            &tp,
                            Rect::from_center_size(pt.pos, vec2(4.0, 4.0)),
                            2.0,
                            with_alpha(pt.color, (1.0 - k) * 0.6),
                            (pt.size * (0.5 + k)) as u8,
                        );
                    }
                }
            }
        }

        // номера строк
        if s.editor.line_numbers != LineNumbers::Off {
            let cur = cursor_lines.first().copied().unwrap_or(0);
            for l in first..last {
                let is_cur = cursor_lines.contains(&l);
                let n = match s.editor.line_numbers {
                    LineNumbers::Relative if !is_cur => l.abs_diff(cur),
                    _ => l + 1,
                };
                let col = if is_cur { c.line_number_active } else { c.line_number };
                text(&p, pos2(gutter_right + shake_off.x, line_y(l) + line_h / 2.0), Align2::RIGHT_CENTER, &n.to_string(), small.clone(), col);
            }
        }

        // полоса прокрутки
        let content_h = total as f32 * line_h + beyond;
        if content_h > view_h + 1.0 {
            let hovered = ui.rect_contains_pointer(rect);
            let recently = (now - doc.view.scrolled_at) < 1.0;
            let vis = self.tw.value(("sb-vis", doc_id), if hovered || recently { 1.0 } else { 0.0 }, self.spec(&s.animations.hover));
            if recently {
                self.tw.active = true;
            }
            if vis > 0.01 {
                let track_h = view_h - 4.0;
                let th = (view_h / content_h * track_h).max(24.0);
                let ty = rect.top() + 2.0 + (disp_y / max_y.max(1.0)) * (track_h - th);
                let sx = rect.right() - mm_w - 9.0;
                p.rect_filled(Rect::from_min_size(pos2(sx, ty), vec2(6.0, th)), cr(3.0), with_alpha(c.text, 0.13 * vis));
            }
        }

        // миникарта
        if mm_w > 0.0 {
            let mm = Rect::from_min_max(pos2(rect.right() - mm_w, rect.top()), rect.max);
            p.line_segment([mm.left_top(), mm.left_bottom()], Stroke::new(1.0, with_alpha(c.border, 0.6)));
            let lp = s.editor.minimap.line_px.clamp(1.0, 8.0);
            let mm_lines = (mm.height() / lp).floor() as usize;
            let start = if total <= mm_lines { 0 } else { ((disp_y / max_y.max(1.0)) * (total - mm_lines) as f32).round() as usize };
            let cwm = ((mm_w - 14.0) / 110.0).clamp(0.5, 1.6);
            let op = s.editor.minimap.opacity.clamp(0.05, 1.0);
            let mmp = p.with_clip_rect(mm);
            for l in start..(start + mm_lines).min(total) {
                let y = mm.top() + (l - start) as f32 * lp;
                let ls_byte = rope.line_to_byte(l);
                let mut si = spans.partition_point(|sp| sp.end <= ls_byte);
                let mut byte = ls_byte;
                let mut dc = 0usize;
                let mut run_start: Option<(usize, u8)> = None;
                let draw = |from: usize, to: usize, class: u8| {
                    if to > from {
                        let col = if large { c.text } else { c.syn(class).color };
                        mmp.rect_filled(
                            Rect::from_min_size(pos2(mm.left() + 8.0 + from as f32 * cwm, y), vec2((to - from) as f32 * cwm, (lp * 0.7).max(1.0))),
                            egui::CornerRadius::ZERO,
                            with_alpha(col, 0.6 * op),
                        );
                    }
                };
                for ch in rope.line(l).chars().take(160) {
                    if ch == '\n' {
                        break;
                    }
                    while si < spans.len() && spans[si].end <= byte {
                        si += 1;
                    }
                    let class = if si < spans.len() && spans[si].start <= byte { spans[si].class } else { 0 };
                    let w = if ch == '\t' { tab - dc % tab } else { 1 };
                    if ch.is_whitespace() {
                        if let Some((st, cl)) = run_start.take() {
                            draw(st, dc, cl);
                        }
                    } else {
                        match run_start {
                            Some((_, cl)) if cl == class => {}
                            Some((st, cl)) => {
                                draw(st, dc, cl);
                                run_start = Some((dc, class));
                            }
                            None => run_start = Some((dc, class)),
                        }
                    }
                    dc += w;
                    byte += ch.len_utf8();
                }
                if let Some((st, cl)) = run_start {
                    draw(st, dc, cl);
                }
            }
            let mm_resp = ui.interact(mm, Id::unique(("minimap", doc_id)), Sense::click_and_drag());
            let show_slider = match s.editor.minimap.slider {
                Visibility::Always => true,
                Visibility::Hover => mm_resp.hovered() || mm_resp.dragged(),
                Visibility::Never => false,
            };
            let vis_first = disp_y / line_h;
            if show_slider {
                let sy = mm.top() + (vis_first - start as f32) * lp;
                let sh = view_lines as f32 * lp;
                let a = if mm_resp.hovered() || mm_resp.dragged() { 1.6 } else { 1.0 };
                mmp.rect_filled(Rect::from_min_size(pos2(mm.left(), sy), vec2(mm_w, sh)), egui::CornerRadius::ZERO, with_alpha(c.minimap_slider, a));
            }
            if mm_resp.is_pointer_button_down_on()
                && let Some(pp) = pointer
            {
                let line = start as f32 + (pp.y - mm.top()) / lp - view_lines as f32 / 2.0;
                let doc = &mut self.docs[idx];
                doc.view.scroll_y = (line * line_h).clamp(0.0, max_y);
                doc.view.scrolled_at = now;
            }
            if mm_resp.hovered() {
                ctx.set_cursor_icon(CursorIcon::Default);
            }
        }
    }
}
