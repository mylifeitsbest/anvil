//! Боковая панель: проводник и тулчейны/паки.

use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Rect, Sense, Stroke, pos2, vec2};

use super::{Icon, cr, icon, icon_button, text, text_width};
use crate::anim::smooth_towards;
use crate::app::{AnvilApp, SidebarView, ToastKind};
use crate::commands::Cmd;
use crate::theme::{mix, with_alpha};

impl AnvilApp {
    pub(crate) fn draw_sidebar(&mut self, ui: &mut egui::Ui, rect: Rect, k: f32, right: bool) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        p.rect_filled(rect, egui::CornerRadius::ZERO, c.sidebar);
        let edge = if right { [rect.left_top(), rect.left_bottom()] } else { [rect.right_top(), rect.right_bottom()] };
        p.line_segment(edge, Stroke::new(1.0, c.border));
        if k < 0.98 {
            self.tw.active = true;
        }
        let dens = self.settings.ui.density.factor();

        // вкладки панели
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), 38.0));
        p.line_segment([head.left_bottom(), head.right_bottom()], Stroke::new(1.0, c.border));
        let tabs = [(SidebarView::Explorer, Icon::Files, "sb-files"), (SidebarView::Tools, Icon::Blocks, "sb-tools")];
        let mut x = head.left() + 8.0;
        for (view, ic, key) in tabs {
            let r = Rect::from_min_size(pos2(x, head.top() + 6.0), vec2(32.0, 26.0));
            let ht = self.hover_t(key, ui.rect_contains_pointer(r));
            if icon_button(ui, r, Id::unique(key), ic, &c, ht, self.sidebar_view == view).clicked() {
                self.sidebar_view = view;
                if view == SidebarView::Tools && !self.tools.scanned {
                    self.rescan_tools();
                }
            }
            x += 34.0;
        }
        let sr = Rect::from_min_size(pos2(x, head.top() + 6.0), vec2(32.0, 26.0));
        let ht = self.hover_t("sb-search", ui.rect_contains_pointer(sr));
        if icon_button(ui, sr, Id::unique("sb-search"), Icon::Search, &c, ht, false).clicked() {
            self.exec(Cmd::PaletteFiles);
        }
        let gr = Rect::from_min_size(pos2(head.right() - 40.0, head.top() + 6.0), vec2(32.0, 26.0));
        let ht = self.hover_t("sb-gear", ui.rect_contains_pointer(gr));
        if icon_button(ui, gr, Id::unique("sb-gear"), Icon::Gear, &c, ht, false).clicked() {
            self.exec(Cmd::PaletteCommands);
        }

        let title = Rect::from_min_size(pos2(rect.left(), head.bottom()), vec2(rect.width(), 30.0 * dens));
        let foot_h = 30.0;
        let body = Rect::from_min_max(pos2(rect.left(), title.bottom()), pos2(rect.right(), rect.bottom() - foot_h));
        let foot = Rect::from_min_max(pos2(rect.left(), body.bottom()), rect.max);
        p.line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, c.border));

        match self.sidebar_view {
            SidebarView::Explorer => {
                text(&p, pos2(title.left() + 14.0, title.center().y), Align2::LEFT_CENTER, "ПРОВОДНИК", self.bold_font(10.0), c.text_faint);
                if !self.ws.name.is_empty() {
                    text(
                        &p,
                        pos2(title.right() - 12.0, title.center().y),
                        Align2::RIGHT_CENTER,
                        &self.ws.name.to_uppercase(),
                        self.bold_font(10.0),
                        c.text_faint,
                    );
                }
                self.draw_tree(ui, body);
                let n = self.ws.index.lock().map(|g| g.len()).unwrap_or(0);
                let indexing = self.ws.indexing.load(std::sync::atomic::Ordering::Relaxed);
                let (dot, label) = if self.ws.root.is_none() {
                    (c.text_faint, "папка не открыта".to_string())
                } else if indexing {
                    (c.warn, "индексация…".to_string())
                } else {
                    (c.ok, format!("{n} файлов · локально"))
                };
                p.circle_filled(pos2(foot.left() + 16.0, foot.center().y), 3.0, dot);
                text(&p, pos2(foot.left() + 26.0, foot.center().y), Align2::LEFT_CENTER, &label, self.ui_font(11.0), c.text_faint);
            }
            SidebarView::Tools => {
                text(&p, pos2(title.left() + 14.0, title.center().y), Align2::LEFT_CENTER, "ТУЛЧЕЙНЫ И ПАКИ", self.bold_font(10.0), c.text_faint);
                let rr = Rect::from_min_size(pos2(title.right() - 34.0, title.top() + 3.0), vec2(26.0, 24.0));
                let ht = self.hover_t("tools-refresh", ui.rect_contains_pointer(rr));
                if icon_button(ui, rr, Id::unique("tools-refresh"), Icon::Refresh, &c, ht, false).clicked() {
                    self.exec(Cmd::ToolsRescan);
                }
                self.draw_tools(ui, body);
                let found = self.tools.found.values().filter(|f| f.is_some()).count();
                let label = if self.tools.scanning {
                    "поиск компиляторов…".to_string()
                } else {
                    format!("найдено {found} из {} · {} языков", self.registry.packs.len(), self.registry.language_count())
                };
                p.circle_filled(pos2(foot.left() + 16.0, foot.center().y), 3.0, if self.tools.scanning { c.warn } else { c.ok });
                text(&p, pos2(foot.left() + 26.0, foot.center().y), Align2::LEFT_CENTER, &label, self.ui_font(11.0), c.text_faint);
            }
        }
    }

    fn draw_tree(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        if self.ws.root.is_none() {
            let b = Rect::from_min_size(pos2(rect.left() + 14.0, rect.top() + 12.0), vec2(rect.width() - 28.0, 32.0));
            let resp = ui.interact(b, Id::unique("tree-open"), Sense::click());
            let ht = self.hover_t("tree-open", resp.hovered());
            p.rect_filled(b, cr(self.settings.theme.radius), mix(c.accent, mix(c.accent, Color32::WHITE, 0.15), ht));
            text(&p, b.center(), Align2::CENTER_CENTER, "Открыть папку", self.bold_font(12.0), c.on_accent);
            if resp.clicked() {
                self.exec(Cmd::FileOpenFolder);
            }
            let hint = self.keymap.label(Cmd::FileOpenFolder).map(|k| format!("или {k}")).unwrap_or_default();
            text(&p, pos2(rect.left() + 14.0, b.bottom() + 20.0), Align2::LEFT_CENTER, &hint, self.ui_font(11.0), c.text_faint);
            return;
        }
        let rows = self.ws.rows();
        let row_h = (23.0 * self.settings.ui.density.factor()).round();
        let content_h = rows.len() as f32 * row_h + 8.0;
        let max_scroll = (content_h - rect.height()).max(0.0);
        if ui.rect_contains_pointer(rect) {
            let d = super::wheel_delta(ui.ctx()).y;
            self.tree_scroll = (self.tree_scroll - d * 1.3).clamp(0.0, max_scroll);
        }
        self.tree_scroll = self.tree_scroll.clamp(0.0, max_scroll);
        let tau = if self.settings.animations.smooth_scroll && self.settings.animations.enabled { 0.035 } else { 0.0 };
        self.tree_disp = smooth_towards(self.tree_disp, self.tree_scroll, self.dt, tau);
        if self.tree_disp != self.tree_scroll {
            self.tw.active = true;
        }
        let active_path = self.doc().and_then(|d| d.path.clone());
        let first = (self.tree_disp / row_h).floor().max(0.0) as usize;
        let last = (first + (rect.height() / row_h) as usize + 2).min(rows.len());
        let spec = self.spec(&self.settings.animations.tree.clone());
        let stagger = self.settings.animations.tree_stagger_ms as f64 / 1000.0;
        let font = self.ui_font(12.5);
        let mut toggle = None;
        let mut open = None;
        for (i, row) in rows.iter().enumerate().take(last).skip(first) {
            let y = rect.top() + 4.0 + i as f32 * row_h - self.tree_disp;
            let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row_h));
            let start = row.parent_opened + (row.index_in_parent.min(30)) as f64 * stagger;
            let kin = self.tw.since(start, spec);
            let dx = (1.0 - kin) * -10.0;
            let resp = ui.interact(r.intersect(rect), Id::unique(("row", &row.entry.path)), Sense::click());
            let ht = self.hover_t(("row-h", &row.entry.path), resp.hovered());
            let active = active_path.as_ref() == Some(&row.entry.path);
            if active {
                p.rect_filled(r, egui::CornerRadius::ZERO, with_alpha(c.accent, 0.13));
                p.rect_filled(Rect::from_min_size(pos2(r.left(), r.top() + 4.0), vec2(2.0, row_h - 8.0)), cr(1.0), c.accent);
            } else if ht > 0.01 {
                p.rect_filled(r, egui::CornerRadius::ZERO, with_alpha(c.hover, ht * 1.4));
            }
            let x = r.left() + 10.0 + row.depth as f32 * 12.0 + dx;
            let cy = r.center().y;
            if row.entry.is_dir {
                let ot = self.tw.value(("chev", &row.entry.path), if row.open { 1.0 } else { 0.0 }, spec);
                let cr_ = Rect::from_min_size(pos2(x, cy - 6.0), vec2(12.0, 12.0));
                icon(&p, cr_, Icon::ChevronRight, with_alpha(c.text_faint, (1.0 - ot) * kin), 1.4);
                icon(&p, cr_, Icon::ChevronDown, with_alpha(c.text_faint, ot * kin), 1.4);
                text(&p, pos2(x + 17.0, cy), Align2::LEFT_CENTER, &row.entry.name, font.clone(), with_alpha(c.text, kin));
            } else {
                let bw = self.file_badge(&p, x + 1.0, cy, &row.entry.name);
                let col = if active { c.text } else { mix(c.text_muted, c.text, ht) };
                text(&p, pos2(x + 4.0 + bw, cy), Align2::LEFT_CENTER, &row.entry.name, font.clone(), with_alpha(col, kin));
                let dirty = self.docs.iter().any(|d| d.path.as_ref() == Some(&row.entry.path) && d.dirty());
                if dirty {
                    p.circle_filled(pos2(r.right() - 14.0, cy), 3.0, c.text_muted);
                }
            }
            if resp.clicked() {
                if row.entry.is_dir {
                    toggle = Some(row.entry.path.clone());
                } else {
                    open = Some(row.entry.path.clone());
                }
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
        }
        if let Some(d) = toggle {
            self.ws.toggle(&d, self.now);
        }
        if let Some(f) = open {
            self.open_path(&f);
        }
    }

    fn draw_tools(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let c = self.colors.clone();
        let p = ui.painter().with_clip_rect(rect);
        let font = self.ui_font(12.5);
        let small = self.ui_font(11.0);

        // перенос из VS Code и папки — короткие действия сверху
        let actions = [
            ("Настройки VS Code", Cmd::ImportVscodeSettings),
            ("Тема VS Code", Cmd::PalettePick(crate::commands::Pick::ImportVscodeTheme)),
            ("Папка паков", Cmd::PacksFolder),
            ("Папка тем", Cmd::ThemesFolder),
        ];
        let packs = {
            let mut v = self.registry.packs.clone();
            v.sort_by_key(|pk| (self.tools.found.get(&pk.id).and_then(|f| f.as_ref()).is_none(), pk.name.to_lowercase()));
            v
        };
        let row_h = 46.0;
        let actions_h = 2.0 * 30.0 + 16.0;
        let content_h = actions_h + packs.len() as f32 * row_h + 12.0;
        let max_scroll = (content_h - rect.height()).max(0.0);
        if ui.rect_contains_pointer(rect) {
            let d = super::wheel_delta(ui.ctx()).y;
            self.tools_scroll = (self.tools_scroll - d * 1.3).clamp(0.0, max_scroll);
        }
        let tau = if self.settings.animations.smooth_scroll && self.settings.animations.enabled { 0.035 } else { 0.0 };
        self.tools_disp = smooth_towards(self.tools_disp, self.tools_scroll, self.dt, tau);
        if self.tools_disp != self.tools_scroll {
            self.tw.active = true;
        }
        let top = rect.top() + 8.0 - self.tools_disp;
        let bw = (rect.width() - 34.0) / 2.0;
        for (i, (label, cmd)) in actions.iter().enumerate() {
            let col = (i % 2) as f32;
            let rowi = (i / 2) as f32;
            let r = Rect::from_min_size(pos2(rect.left() + 12.0 + col * (bw + 10.0), top + rowi * 30.0), vec2(bw, 26.0));
            let resp = ui.interact(r.intersect(rect), Id::unique(("tool-act", i)), Sense::click());
            let ht = self.hover_t(("tool-act", i), resp.hovered());
            p.rect_filled(r, cr(self.settings.theme.radius * 0.7), with_alpha(c.hover, 1.0 + ht));
            p.rect_stroke(r, cr(self.settings.theme.radius * 0.7), Stroke::new(1.0, c.border), egui::StrokeKind::Inside);
            text(&p, r.center(), Align2::CENTER_CENTER, label, self.ui_font(10.5), mix(c.text_muted, c.text, ht));
            if resp.clicked() {
                self.exec(*cmd);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
        }
        let mut y = top + actions_h;
        let mut install = None;
        let mut site = None;
        for pk in &packs {
            let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row_h));
            y += row_h;
            if r.bottom() < rect.top() || r.top() > rect.bottom() {
                continue;
            }
            let found = self.tools.found.get(&pk.id).cloned().flatten();
            let resp = ui.interact(r.intersect(rect), Id::unique(("pack", &pk.id)), Sense::hover());
            let ht = self.hover_t(("pack-h", &pk.id), resp.hovered());
            if ht > 0.01 {
                p.rect_filled(r, egui::CornerRadius::ZERO, with_alpha(c.hover, ht * 1.3));
            }
            let dot = match (&found, self.tools.found.contains_key(&pk.id)) {
                (Some(_), _) => c.ok,
                (None, true) => c.text_faint,
                (None, false) => with_alpha(c.text_faint, 0.4),
            };
            p.circle_filled(pos2(r.left() + 16.0, r.top() + 16.0), 3.5, dot);
            text(&p, pos2(r.left() + 28.0, r.top() + 16.0), Align2::LEFT_CENTER, &pk.name, font.clone(), if found.is_some() { c.text } else { c.text_muted });
            let sub = match (&found, self.tools.versions.get(&pk.id)) {
                (Some(_), Some(v)) => v.clone(),
                (Some(f), None) => f.path.to_string_lossy().to_string(),
                (None, _) => format!("не найден · {}", pk.languages.join(", ")),
            };
            let max_w = rect.width() - 40.0 - if found.is_none() { 96.0 } else { 0.0 };
            let mut sub_s = sub.clone();
            while text_width(ui.ctx(), &sub_s, &small) > max_w && sub_s.chars().count() > 4 {
                sub_s = sub_s.chars().take(sub_s.chars().count() - 2).collect::<String>();
                sub_s.push('…');
                sub_s = sub_s.replace("……", "…");
            }
            text(&p, pos2(r.left() + 28.0, r.top() + 33.0), Align2::LEFT_CENTER, &sub_s, small.clone(), c.text_faint);
            if found.is_none() && self.tools.found.contains_key(&pk.id) {
                let has_cmd = pk.install.current().is_some();
                let armed = self.tools.armed.as_ref().is_some_and(|(id, t)| *id == pk.id && self.now - t < 4.0);
                let label = if !has_cmd {
                    "Сайт"
                } else if armed {
                    "Точно?"
                } else {
                    "Установить"
                };
                let w = text_width(ui.ctx(), label, &small) + 18.0;
                let b = Rect::from_min_size(pos2(r.right() - 12.0 - w, r.top() + 11.0), vec2(w, 24.0));
                let bresp = ui.interact(b.intersect(rect), Id::unique(("install", &pk.id)), Sense::click());
                let bh = self.hover_t(("install-h", &pk.id), bresp.hovered());
                let fill = if armed { c.warn } else { mix(c.accent, mix(c.accent, Color32::WHITE, 0.15), bh) };
                p.rect_filled(b, cr(self.settings.theme.radius * 0.7), if has_cmd { fill } else { with_alpha(c.hover, 1.5 + bh) });
                text(&p, b.center(), Align2::CENTER_CENTER, label, small.clone(), if has_cmd { c.on_accent } else { c.text_muted });
                if bresp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                if bresp.clicked() {
                    if !has_cmd {
                        site = Some(pk.homepage.clone());
                    } else if armed {
                        install = Some(pk.id.clone());
                        self.tools.armed = None;
                    } else {
                        self.tools.armed = Some((pk.id.clone(), self.now));
                        let cmd = pk.install.current().unwrap_or_default().to_string();
                        self.toast(format!("Будет выполнено: {cmd}. Нажмите «Точно?» для подтверждения"), ToastKind::Info);
                    }
                }
            } else if found.is_some() && resp.hovered() {
                let b = Rect::from_min_size(pos2(r.right() - 34.0, r.top() + 11.0), vec2(24.0, 24.0));
                let bresp = ui.interact(b, Id::unique(("site", &pk.id)), Sense::click());
                icon(&p, b.shrink(4.0), Icon::Link, if bresp.hovered() { c.text } else { c.text_faint }, 1.2);
                if bresp.clicked() {
                    site = Some(pk.homepage.clone());
                }
            }
        }
        if let Some(id) = install {
            self.install_pack(&id);
        }
        if let Some(url) = site {
            self.open_url(&url);
        }
    }
}
