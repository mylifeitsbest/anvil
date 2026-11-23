//! Отрисовка интерфейса. Всё рисуется вручную через `Painter` — так проще
//! добиться минималистичного вида и полного контроля над анимациями.

pub mod chrome;
pub mod editor;
pub mod lsp_ui;
pub mod overlay;
pub mod panel;
pub mod sidebar;

use eframe::egui::{self, Align2, Color32, CornerRadius, Event, FontId, Key, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, epaint, pos2, vec2};

use crate::theme::{Colors, with_alpha};

pub fn text(p: &Painter, pos: Pos2, anchor: Align2, s: &str, font: FontId, color: Color32) -> Rect {
    p.text(pos, anchor, s, font, color)
}

/// Прокрутка колесом за кадр (в точках), без встроенного сглаживания egui — сглаживаем сами.
pub fn wheel_delta(ctx: &egui::Context) -> egui::Vec2 {
    ctx.input(|i| {
        let mut d = egui::Vec2::ZERO;
        for ev in &i.raw.events {
            if let Event::MouseWheel { unit, delta, .. } = ev {
                d += match unit {
                    egui::MouseWheelUnit::Point => *delta,
                    egui::MouseWheelUnit::Line => *delta * 48.0,
                    egui::MouseWheelUnit::Page => *delta * i.content_rect().height(),
                };
            }
        }
        d
    })
}

pub fn text_width(ctx: &egui::Context, s: &str, font: &FontId) -> f32 {
    ctx.fonts_mut(|f| f.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)).size().x
}

/// Мягкая тень под всплывающими элементами.
pub fn shadow(p: &Painter, rect: Rect, radius: f32, color: Color32, blur: u8) {
    let s = epaint::Shadow { offset: [0, 8], blur, spread: 0, color };
    p.add(s.as_shape(rect, CornerRadius::same(radius.clamp(0.0, 120.0) as u8)));
}

pub fn cr(r: f32) -> CornerRadius {
    CornerRadius::same(r.clamp(0.0, 120.0).round() as u8)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Files,
    Search,
    Blocks,
    Gear,
    Play,
    Stop,
    Close,
    Min,
    Max,
    ChevronRight,
    ChevronDown,
    Branch,
    Check,
    Warn,
    Error,
    Info,
    Refresh,
    Download,
    Link,
    Folder,
    Plus,
    Up,
    Down,
}

/// Векторные иконки 16×16, растягиваются в `rect`.
pub fn icon(p: &Painter, rect: Rect, ic: Icon, color: Color32, w: f32) {
    let s = rect.width().min(rect.height()) / 16.0;
    let o = rect.center() - vec2(8.0 * s, 8.0 * s);
    let pt = |x: f32, y: f32| o + vec2(x * s, y * s);
    let st = Stroke::new(w, color);
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], st);
    };
    let poly = |pts: &[(f32, f32)], closed: bool| {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| pt(*x, *y)).collect();
        if closed {
            p.add(Shape::closed_line(v, st));
        } else {
            p.add(Shape::line(v, st));
        }
    };
    match ic {
        Icon::Files => {
            poly(&[(4.0, 2.0), (9.5, 2.0), (12.5, 5.0), (12.5, 14.0), (4.0, 14.0)], true);
            poly(&[(9.5, 2.0), (9.5, 5.0), (12.5, 5.0)], false);
        }
        Icon::Search => {
            p.circle_stroke(pt(7.0, 7.0), 4.2 * s, st);
            line((10.2, 10.2), (13.5, 13.5));
        }
        Icon::Blocks => {
            for (x, y) in [(2.5, 2.5), (9.0, 2.5), (2.5, 9.0)] {
                p.rect_stroke(Rect::from_min_size(pt(x, y), vec2(4.5 * s, 4.5 * s)), cr(1.2 * s), st, StrokeKind::Middle);
            }
            line((11.25, 9.0), (11.25, 13.5));
            line((9.0, 11.25), (13.5, 11.25));
        }
        Icon::Gear => {
            p.circle_stroke(pt(8.0, 8.0), 2.2 * s, st);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (sn, cs) = a.sin_cos();
                line((8.0 + cs * 4.4, 8.0 + sn * 4.4), (8.0 + cs * 6.0, 8.0 + sn * 6.0));
            }
            p.circle_stroke(pt(8.0, 8.0), 4.4 * s, st);
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![pt(5.0, 3.5), pt(12.5, 8.0), pt(5.0, 12.5)], color, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(pt(4.5, 4.5), pt(11.5, 11.5)), cr(1.5 * s), color);
        }
        Icon::Close => {
            line((4.5, 4.5), (11.5, 11.5));
            line((11.5, 4.5), (4.5, 11.5));
        }
        Icon::Min => line((4.0, 8.5), (12.0, 8.5)),
        Icon::Max => {
            p.rect_stroke(Rect::from_min_max(pt(4.5, 4.5), pt(11.5, 11.5)), cr(1.0 * s), st, StrokeKind::Middle);
        }
        Icon::ChevronRight => poly(&[(6.0, 4.0), (10.0, 8.0), (6.0, 12.0)], false),
        Icon::ChevronDown => poly(&[(4.0, 6.0), (8.0, 10.0), (12.0, 6.0)], false),
        Icon::Branch => {
            p.circle_stroke(pt(5.0, 3.5), 1.6 * s, st);
            p.circle_stroke(pt(5.0, 12.5), 1.6 * s, st);
            p.circle_stroke(pt(11.5, 5.0), 1.6 * s, st);
            line((5.0, 5.1), (5.0, 10.9));
            poly(&[(11.5, 6.6), (11.5, 8.0), (6.2, 10.6)], false);
        }
        Icon::Check => poly(&[(3.5, 8.5), (6.8, 11.5), (12.5, 4.5)], false),
        Icon::Warn => {
            poly(&[(8.0, 2.5), (14.0, 13.0), (2.0, 13.0)], true);
            line((8.0, 6.5), (8.0, 9.5));
            p.circle_filled(pt(8.0, 11.3), 0.7 * s, color);
        }
        Icon::Error => {
            p.circle_stroke(pt(8.0, 8.0), 5.8 * s, st);
            line((5.8, 5.8), (10.2, 10.2));
            line((10.2, 5.8), (5.8, 10.2));
        }
        Icon::Info => {
            p.circle_stroke(pt(8.0, 8.0), 5.8 * s, st);
            line((8.0, 7.2), (8.0, 11.0));
            p.circle_filled(pt(8.0, 5.0), 0.7 * s, color);
        }
        Icon::Refresh => {
            let n = 14;
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let a = -0.6 + i as f32 / n as f32 * 4.6;
                    let (sn, cs) = a.sin_cos();
                    pt(8.0 + cs * 5.0, 8.0 + sn * 5.0)
                })
                .collect();
            p.add(Shape::line(pts, st));
            poly(&[(12.6, 2.6), (12.9, 5.4), (10.2, 5.6)], false);
        }
        Icon::Download => {
            line((8.0, 2.5), (8.0, 10.5));
            poly(&[(4.8, 7.5), (8.0, 10.7), (11.2, 7.5)], false);
            line((3.5, 13.5), (12.5, 13.5));
        }
        Icon::Link => {
            p.circle_stroke(pt(8.0, 8.0), 5.8 * s, st);
            line((2.2, 8.0), (13.8, 8.0));
            p.add(Shape::ellipse_stroke(pt(8.0, 8.0), vec2(2.6 * s, 5.8 * s), st));
        }
        Icon::Folder => {
            poly(&[(2.0, 4.0), (6.0, 4.0), (7.5, 5.5), (14.0, 5.5), (14.0, 12.5), (2.0, 12.5)], true);
        }
        Icon::Plus => {
            line((8.0, 3.5), (8.0, 12.5));
            line((3.5, 8.0), (12.5, 8.0));
        }
        Icon::Up => poly(&[(4.0, 10.0), (8.0, 6.0), (12.0, 10.0)], false),
        Icon::Down => poly(&[(4.0, 6.0), (8.0, 10.0), (12.0, 6.0)], false),
    }
}

/// Логотип — буква «A»: каретка ^ и черта под ней (как assets/icon/anvil-small.svg), цвет — акцент темы.
pub fn logo(p: &Painter, rect: Rect, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - vec2(12.0 * s, 12.0 * s);
    let pt = |x: f32, y: f32| o + vec2(x * s, y * s);
    // Отрезки с круглыми концами: egui рисует торцы прямыми, поэтому скругляем кругами.
    let round = |ends: &[Pos2], w: f32| {
        for e in ends.windows(2) {
            p.line_segment([e[0], e[1]], Stroke::new(w * s, color));
        }
        for &e in ends {
            p.circle_filled(e, w * s / 2.0, color);
        }
    };
    round(&[pt(5.2, 18.95), pt(12.0, 4.55), pt(18.8, 18.95)], 4.1);
    round(&[pt(9.95, 19.95), pt(14.05, 19.95)], 3.1);
}

/// Кнопка-иконка с анимированной подсветкой при наведении. Возвращает ответ egui.
pub fn icon_button(ui: &mut egui::Ui, rect: Rect, id: egui::Id, ic: Icon, colors: &Colors, hover_t: f32, active: bool) -> egui::Response {
    let resp = ui.interact(rect, id, egui::Sense::click());
    let p = ui.painter();
    if hover_t > 0.01 || active {
        p.rect_filled(rect.shrink(1.0), cr(6.0), with_alpha(colors.hover, if active { 1.6 } else { hover_t * 1.6 }));
    }
    let c = if active || resp.hovered() { colors.text } else { colors.text_muted };
    icon(p, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), ic, c, 1.4);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

// ---------------------------------------------------------------------------- однострочное поле

#[derive(Default, Clone, Debug)]
pub struct LineInput {
    pub text: String,
    /// Позиция курсора в символах.
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub edited_at: f64,
}

#[derive(Default, Debug, Clone)]
pub struct InputOut {
    pub changed: bool,
    pub enter: bool,
    pub shift_enter: bool,
    pub escape: bool,
    pub up: bool,
    pub down: bool,
    pub page_up: bool,
    pub page_down: bool,
    pub tab: bool,
    pub copy: Option<String>,
}

impl LineInput {
    pub fn set(&mut self, s: &str) {
        self.text = s.to_string();
        self.cursor = self.text.chars().count();
        self.anchor = None;
    }

    fn byte(&self, c: usize) -> usize {
        self.text.char_indices().nth(c).map(|(b, _)| b).unwrap_or(self.text.len())
    }

    fn sel(&self) -> Option<(usize, usize)> {
        self.anchor.filter(|a| *a != self.cursor).map(|a| (a.min(self.cursor), a.max(self.cursor)))
    }

    fn delete_sel(&mut self) -> bool {
        if let Some((a, b)) = self.sel() {
            let (ba, bb) = (self.byte(a), self.byte(b));
            self.text.replace_range(ba..bb, "");
            self.cursor = a;
            self.anchor = None;
            true
        } else {
            false
        }
    }

    fn insert(&mut self, s: &str) {
        self.delete_sel();
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        let b = self.byte(self.cursor);
        self.text.insert_str(b, &s);
        self.cursor += s.chars().count();
    }

    pub fn handle(&mut self, events: &[Event], now: f64) -> InputOut {
        let mut out = InputOut::default();
        for ev in events {
            match ev {
                Event::Text(t) => {
                    self.insert(t);
                    out.changed = true;
                }
                Event::Paste(t) => {
                    self.insert(t.lines().next().unwrap_or_default());
                    out.changed = true;
                }
                Event::Copy | Event::Cut => {
                    let s = match self.sel() {
                        Some((a, b)) => self.text.chars().skip(a).take(b - a).collect(),
                        None => self.text.clone(),
                    };
                    out.copy = Some(s);
                    if matches!(ev, Event::Cut) && self.delete_sel() {
                        out.changed = true;
                    }
                }
                Event::Key { key, pressed: true, modifiers, .. } => {
                    let len = self.text.chars().count();
                    let word = modifiers.ctrl || modifiers.command;
                    let ext = modifiers.shift;
                    let mv = |me: &mut Self, to: usize| {
                        if ext {
                            if me.anchor.is_none() {
                                me.anchor = Some(me.cursor);
                            }
                        } else {
                            me.anchor = None;
                        }
                        me.cursor = to.min(len);
                    };
                    match key {
                        Key::Backspace => {
                            if !self.delete_sel() && self.cursor > 0 {
                                let from = if word { self.prev_word() } else { self.cursor - 1 };
                                let (ba, bb) = (self.byte(from), self.byte(self.cursor));
                                self.text.replace_range(ba..bb, "");
                                self.cursor = from;
                            }
                            out.changed = true;
                        }
                        Key::Delete => {
                            if !self.delete_sel() && self.cursor < len {
                                let (ba, bb) = (self.byte(self.cursor), self.byte(self.cursor + 1));
                                self.text.replace_range(ba..bb, "");
                            }
                            out.changed = true;
                        }
                        Key::ArrowLeft => {
                            let to = if word { self.prev_word() } else { self.cursor.saturating_sub(1) };
                            mv(self, to);
                        }
                        Key::ArrowRight => {
                            let to = if word { self.next_word() } else { self.cursor + 1 };
                            mv(self, to);
                        }
                        Key::Home => mv(self, 0),
                        Key::End => mv(self, len),
                        Key::A if word => {
                            self.anchor = Some(0);
                            self.cursor = len;
                        }
                        Key::Enter => {
                            if modifiers.shift {
                                out.shift_enter = true;
                            } else {
                                out.enter = true;
                            }
                        }
                        Key::Escape => out.escape = true,
                        Key::ArrowUp => out.up = true,
                        Key::ArrowDown => out.down = true,
                        Key::PageUp => out.page_up = true,
                        Key::PageDown => out.page_down = true,
                        Key::Tab => out.tab = true,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if out.changed {
            self.edited_at = now;
        }
        out
    }

    fn prev_word(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor.min(chars.len());
        while i > 0 && !chars[i - 1].is_alphanumeric() {
            i -= 1;
        }
        while i > 0 && chars[i - 1].is_alphanumeric() {
            i -= 1;
        }
        i
    }

    fn next_word(&self) -> usize {
        let chars: Vec<char> = self.text.chars().collect();
        let mut i = self.cursor.min(chars.len());
        while i < chars.len() && !chars[i].is_alphanumeric() {
            i += 1;
        }
        while i < chars.len() && chars[i].is_alphanumeric() {
            i += 1;
        }
        i
    }

    /// Рисует текст поля с курсором.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(&self, ctx: &egui::Context, p: &Painter, rect: Rect, font: &FontId, colors: &Colors, focused: bool, placeholder: &str, caret_alpha: f32) {
        let y = rect.center().y;
        let before: String = self.text.chars().take(self.cursor).collect();
        let cx = rect.left() + text_width(ctx, &before, font);
        if let Some((a, b)) = self.sel() {
            let xa = rect.left() + text_width(ctx, &self.text.chars().take(a).collect::<String>(), font);
            let xb = rect.left() + text_width(ctx, &self.text.chars().take(b).collect::<String>(), font);
            p.rect_filled(Rect::from_min_max(pos2(xa, y - font.size * 0.7), pos2(xb, y + font.size * 0.7)), cr(3.0), colors.selection);
        }
        if self.text.is_empty() {
            text(p, pos2(rect.left(), y), Align2::LEFT_CENTER, placeholder, font.clone(), colors.text_faint);
        } else {
            text(p, pos2(rect.left(), y), Align2::LEFT_CENTER, &self.text, font.clone(), colors.text);
        }
        if focused {
            p.rect_filled(Rect::from_min_size(pos2(cx, y - font.size * 0.62), vec2(1.6, font.size * 1.24)), cr(1.0), with_alpha(colors.caret, caret_alpha));
        }
    }
}

/// Нечёткий поиск: подпоследовательность с бонусами за начало слов и подряд идущие буквы.
pub fn fuzzy(q: &str, s: &str) -> Option<(i32, Vec<usize>)> {
    if q.is_empty() {
        return Some((0, Vec::new()));
    }
    let ql: Vec<char> = q.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    let chars: Vec<char> = s.chars().collect();
    let sl: Vec<char> = s.to_lowercase().chars().collect();
    if sl.len() != chars.len() {
        return None;
    }
    let mut idx = Vec::with_capacity(ql.len());
    let mut j = 0;
    let mut score = 0i32;
    let mut prev: Option<usize> = None;
    for (i, c) in sl.iter().enumerate() {
        if j < ql.len() && *c == ql[j] {
            let boundary =
                i == 0 || matches!(chars[i - 1], ' ' | ':' | '_' | '-' | '.' | '/' | '\\' | '(') || (chars[i].is_uppercase() && chars[i - 1].is_lowercase());
            score += 1 + if prev == Some(i.wrapping_sub(1)) { 5 } else { 0 } + if boundary { 4 } else { 0 };
            idx.push(i);
            prev = Some(i);
            j += 1;
        }
    }
    if j < ql.len() {
        return None;
    }
    score -= (chars.len() as i32) / 12;
    Some((score, idx))
}

/// Рисует строку, подсвечивая символы с индексами `hl`.
#[allow(clippy::too_many_arguments)]
pub fn paint_highlighted(ctx: &egui::Context, p: &Painter, pos: Pos2, s: &str, hl: &[usize], font: &FontId, color: Color32, hl_color: Color32) -> f32 {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let mut run = String::new();
    let mut run_hl = false;
    let flush = |job: &mut LayoutJob, run: &mut String, h: bool| {
        if !run.is_empty() {
            job.append(run, 0.0, TextFormat { font_id: font.clone(), color: if h { hl_color } else { color }, ..Default::default() });
            run.clear();
        }
    };
    let mut k = 0;
    for (i, c) in s.chars().enumerate() {
        let h = k < hl.len() && hl[k] == i;
        if h {
            k += 1;
        }
        if h != run_hl {
            flush(&mut job, &mut run, run_hl);
            run_hl = h;
        }
        run.push(c);
    }
    flush(&mut job, &mut run, run_hl);
    let g = ctx.fonts_mut(|f| f.layout_job(job));
    let w = g.size().x;
    p.galley(pos2(pos.x, pos.y - g.size().y / 2.0), g, color);
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_prefers_word_starts() {
        let (a, _) = fuzzy("tc", "Тема: цвет").map(|x| (x.0, ())).unwrap_or((0, ()));
        let _ = a;
        let x = fuzzy("os", "Открыть settings").map(|v| v.1);
        assert!(x.is_none(), "русская «о» не латинская o");
        let a = fuzzy("fs", "file.save").map(|v| v.0).unwrap_or(0);
        let b = fuzzy("fs", "offset").map(|v| v.0).unwrap_or(0);
        assert!(a > b);
    }

    #[test]
    fn line_input_editing() {
        let mut li = LineInput::default();
        li.handle(&[Event::Text("привет мир".into())], 0.0);
        assert_eq!(li.cursor, 10);
        li.handle(&[Event::Key { key: Key::Backspace, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::CTRL }], 0.0);
        assert_eq!(li.text, "привет ");
    }
}
