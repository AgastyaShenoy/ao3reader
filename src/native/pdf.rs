//! The PDF reader: pages as they are, in one scrolling column, rendered in tiles on background
//! threads by the engine from the pdfreader project (`crates/pdf-engine`, `crates/pdf-render`).

use super::gfx::*;
use super::reader::ReaderAction;
use super::ui::*;
use crate::ao3::models::{ReadingProgress, WorkMeta};
use crate::pdf::{Engine, PageSize};
use crate::reader::theme::ReaderThemeType;
use crate::storage::config::AppConfig;
use pdf_render::{compose, wanted_tiles, Frame, Layout, Look, RenderPool, Style, TileCache, Viewport};
use std::sync::Arc;

const TOOLBAR_H: f32 = 48.0;
/// Space around and between pages, in DIPs.
const GAP: f32 = 12.0;
/// Zoom steps, as a share of the page's printed size.
const ZOOMS: [f32; 13] = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0, 4.0];

const VK_SPACE: u16 = 0x20;
const VK_PRIOR: u16 = 0x21;
const VK_NEXT: u16 = 0x22;
const VK_END: u16 = 0x23;
const VK_HOME: u16 = 0x24;
const VK_LEFT: u16 = 0x25;
const VK_UP: u16 = 0x26;
const VK_RIGHT: u16 = 0x27;
const VK_DOWN: u16 = 0x28;
const VK_ESCAPE: u16 = 0x1B;
const VK_F11: u16 = 0x7A;
const VK_ADD: u16 = 0x6B;
const VK_SUBTRACT: u16 = 0x6D;
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_MINUS: u16 = 0xBD;
const VK_0: u16 = 0x30;

pub struct PdfScreen {
    meta: WorkMeta,
    pool: RenderPool,
    cache: TileCache,
    sizes: Vec<PageSize>,
    /// None fits the pages' width to the window; otherwise the zoom (1.0 = printed size).
    zoom: Option<f32>,
    layout: Option<Layout>,
    /// What the layout was made for: (pixels per point, window width in pixels).
    layout_for: (u32, u32),
    /// Scroll position, in layout pixels (sideways only when pages are wider than the window).
    x: i64,
    y: i64,
    /// Which way the last scroll went: the next screen that way is rendered first.
    ahead: i64,
    /// Where to go once laid out: page + fraction of it.
    restore: Option<f32>,
    anchor: f32,
    /// The visible pages, composed for drawing.
    buf: Vec<u32>,
    /// The scroll bar's thumb is being dragged: where it was grabbed.
    grab: Option<f32>,
    pub progress_dirty: bool,
    pub fullscreen: bool,
}

/// 0x00RRGGBB from a colour.
fn rgb(c: Color) -> u32 {
    let to = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (to(c.r) << 16) | (to(c.g) << 8) | to(c.b)
}

impl PdfScreen {
    /// `wake` is called from the render threads when pages are ready to show.
    pub fn new(meta: WorkMeta, engine: Arc<dyn Engine>, progress: Option<&ReadingProgress>, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let sizes = (0..engine.page_count()).map(|p| engine.page_size(p)).collect();
        let threads = std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(1, 4));
        Self {
            meta,
            pool: RenderPool::new(engine, threads, wake),
            cache: TileCache::default(),
            sizes,
            zoom: None,
            layout: None,
            layout_for: (0, 0),
            x: 0,
            y: 0,
            ahead: 1,
            restore: progress.map(|p| p.position),
            anchor: 0.0,
            buf: Vec::new(),
            grab: None,
            progress_dirty: false,
            fullscreen: false,
        }
    }

    pub fn work_id(&self) -> &str {
        &self.meta.id
    }

    fn page_count(&self) -> usize {
        self.sizes.len()
    }

    pub fn progress(&self) -> ReadingProgress {
        let n = self.page_count().max(1) as f32;
        let completed = self.anchor >= n - 0.05;
        ReadingProgress {
            current_chapter: 1,
            scroll_offset: self.anchor.floor(),
            position: self.anchor,
            percentage: if completed { 100.0 } else { (self.anchor / n * 100.0).clamp(0.0, 100.0) },
            last_read: chrono::Utc::now(),
            completed,
        }
    }

    /// Page and fraction at the top of the window.
    fn anchor_at(layout: &Layout, y: i64) -> f32 {
        if layout.pages.is_empty() {
            return 0.0;
        }
        let p = layout.page_at(y);
        let pos = layout.pages[p];
        p as f32 + ((y - pos.y) as f32 / pos.height.max(1) as f32).clamp(0.0, 0.999)
    }

    fn y_for(layout: &Layout, anchor: f32) -> i64 {
        let Some(last) = layout.pages.len().checked_sub(1) else { return 0 };
        let p = (anchor.max(0.0) as usize).min(last);
        let pos = layout.pages[p];
        pos.y + ((anchor - p as f32).clamp(0.0, 1.0) * pos.height as f32) as i64 - layout.gap as i64
    }

    fn set_zoom(&mut self, zoom: Option<f32>) {
        if let Some(l) = &self.layout {
            self.restore = Some(Self::anchor_at(l, self.y + l.gap as i64));
        }
        self.zoom = zoom.map(|z| z.clamp(ZOOMS[0], ZOOMS[ZOOMS.len() - 1]));
    }

    /// The zoom now, as a share of the printed size (also when fitting the width).
    fn zoom_now(&self, dpi: f32) -> f32 {
        self.zoom.unwrap_or_else(|| self.layout.as_ref().map_or(1.0, |l| l.scale * 72.0 / dpi))
    }

    fn step_zoom(&mut self, dpi: f32, up: bool) {
        let now = self.zoom_now(dpi);
        let next = if up { ZOOMS.iter().copied().find(|z| *z > now + 0.01) } else { ZOOMS.iter().rev().copied().find(|z| *z < now - 0.01) };
        if let Some(z) = next {
            self.set_zoom(Some(z));
        }
    }

    pub fn frame(&mut self, ui: &mut Ui, r: Rect, config: &mut AppConfig) -> Vec<ReaderAction> {
        let mut out = Vec::new();
        let c = ui.c;
        let chrome = !self.fullscreen;
        let top = if chrome { TOOLBAR_H } else { 0.0 };
        let area = Rect::new(r.x, r.y + top, r.w, (r.h - top).max(50.0));
        let px = ui.g.dpi / 96.0;
        let (vw, vh) = ((area.w * px).round().max(1.0) as u32, (area.h * px).round().max(1.0) as u32);

        // Lay the pages out for this zoom and window width.
        let gap = (GAP * px).round() as u32;
        let widest = self.sizes.iter().map(|s| s.width).fold(1.0f32, f32::max);
        let scale = match self.zoom {
            Some(z) => z * ui.g.dpi / 72.0,
            None => ((vw.saturating_sub(2 * gap)) as f32 / widest).clamp(0.05, 20.0),
        };
        if self.layout.is_none() || self.layout_for != (scale.to_bits(), vw) {
            if let (Some(l), None) = (&self.layout, self.restore) {
                self.restore = Some(Self::anchor_at(l, self.y + l.gap as i64));
            }
            self.layout = Some(Layout::new(&self.sizes, scale, gap, vw));
            self.layout_for = (scale.to_bits(), vw);
        }
        let layout = self.layout.as_ref().unwrap();
        if let Some(a) = self.restore.take() {
            self.y = Self::y_for(layout, a);
        }

        self.keys(ui, &mut out, vh as i64, px);
        if ui.hit(area) && ui.input.wheel != 0.0 {
            let dy = (-ui.input.wheel * 120.0 * px) as i64;
            self.y += dy;
            self.ahead = dy.signum();
            ui.request_repaint();
        }

        let layout = self.layout.as_ref().unwrap();
        let max_y = (layout.height - vh as i64).max(0);
        let max_x = (layout.width - vw as i64).max(0);
        self.y = self.y.clamp(0, max_y);
        self.x = if max_x == 0 { 0 } else { self.x.clamp(0, max_x) };
        let view = Viewport { x: self.x, y: self.y, width: vw, height: vh };

        // Pages: tiles that are ready, then ask for the ones still missing.
        if ui.g.drawing {
            for done in self.pool.take_finished() {
                self.cache.insert(done.key, done.width, done.height, done.pixels);
            }
            self.cache.set_budget(6 * vw as usize * vh as usize * 4);
            self.cache.begin_frame();
            let pal = config.palette();
            let look = if pal.is_dark {
                Look::Dark
            } else if config.theme == ReaderThemeType::WarmSepia {
                Look::Sepia
            } else {
                Look::Normal
            };
            self.buf.resize(vw as usize * vh as usize, 0);
            let style = Style { background: rgb(c.bg), border: Some(rgb(c.border)), look };
            let mut frame = Frame { pixels: &mut self.buf, width: vw, height: vh, stride: vw as usize };
            compose(&mut frame, layout, &view, &self.cache, &style);
            self.pool.set_jobs(wanted_tiles(layout, &view, self.ahead, look, &self.cache));
            self.cache.evict();
            ui.g.draw_pixels(&self.buf, vw, vh, area);
        }
        self.scrollbar(ui, area, vh as i64);

        // Where the reader is, for the library and the toolbar.
        let layout = self.layout.as_ref().unwrap();
        let anchor = if self.y >= max_y && max_y > 0 { self.page_count() as f32 } else { Self::anchor_at(layout, self.y + layout.gap as i64) };
        if (anchor - self.anchor).abs() > 0.001 {
            self.anchor = anchor;
            self.progress_dirty = true;
        }
        let current = layout.current_page(&view);

        if chrome {
            let mark = ui.node_mark();
            self.toolbar(ui, Rect::new(r.x, r.y, r.w, TOOLBAR_H), current, &mut out);
            ui.tab_first(mark);
        }
        out
    }

    fn keys(&mut self, ui: &mut Ui, out: &mut Vec<ReaderAction>, view_h: i64, px: f32) {
        let line = (48.0 * px) as i64;
        let screen = (view_h - line).max(line);
        let page_y = |s: &Self, p: usize| s.layout.as_ref().map_or(0, |l| l.pages[p.min(l.pages.len() - 1)].y - l.gap as i64);
        let current = self.layout.as_ref().map_or(0, |l| l.page_at(self.y + l.gap as i64 + 1));
        let mut dy = 0;
        if ui.key_combo(VK_DOWN, false, false) {
            dy = line;
        } else if ui.key_combo(VK_UP, false, false) {
            dy = -line;
        } else if ui.key_combo(VK_NEXT, false, false) || ui.key_combo(VK_SPACE, false, false) {
            dy = screen;
        } else if ui.key_combo(VK_PRIOR, false, false) || ui.key_combo(VK_SPACE, false, true) {
            dy = -screen;
        }
        if dy != 0 {
            self.y += dy;
            self.ahead = dy.signum();
        }
        if ui.key_combo(VK_RIGHT, false, false) {
            self.y = page_y(self, current + 1);
            self.ahead = 1;
        } else if ui.key_combo(VK_LEFT, false, false) {
            self.y = page_y(self, current.saturating_sub(1));
            self.ahead = -1;
        }
        if ui.key_combo(VK_HOME, false, false) {
            self.y = 0;
        }
        if ui.key_combo(VK_END, false, false) {
            self.y = i64::MAX / 4;
        }
        let dpi = ui.g.dpi;
        if ui.key_combo(VK_OEM_PLUS, true, false) || ui.key_combo(VK_ADD, true, false) {
            self.step_zoom(dpi, true);
        }
        if ui.key_combo(VK_OEM_MINUS, true, false) || ui.key_combo(VK_SUBTRACT, true, false) {
            self.step_zoom(dpi, false);
        }
        if ui.key_combo(VK_0, true, false) {
            self.set_zoom(None);
        }
        if ui.key_combo(VK_F11, false, false) || (self.fullscreen && ui.key_combo(VK_ESCAPE, false, false)) {
            out.push(ReaderAction::ToggleFullscreen);
        }
    }

    /// A thin scroll bar at the right edge; drag the thumb, or press the bar to jump there.
    fn scrollbar(&mut self, ui: &mut Ui, area: Rect, view_h: i64) {
        let Some(layout) = &self.layout else { return };
        let total = layout.height.max(1) as f32;
        let max = (layout.height - view_h).max(0) as f32;
        if max <= 0.0 {
            return;
        }
        let bar = Rect::new(area.right() - 12.0, area.y, 12.0, area.h);
        let thumb_h = (area.h * view_h as f32 / total).max(32.0);
        let thumb_y = area.y + (area.h - thumb_h) * (self.y as f32 / max);
        let resp = ui.interact(id("pdf-bar"), bar);
        if resp.held {
            let m = ui.input.mouse.1;
            let grab = *self.grab.get_or_insert(if (thumb_y..thumb_y + thumb_h).contains(&m) { m - thumb_y } else { thumb_h / 2.0 });
            let t = ((m - grab - area.y) / (area.h - thumb_h).max(1.0)).clamp(0.0, 1.0);
            self.y = (t * max) as i64;
            ui.request_repaint();
        } else {
            self.grab = None;
        }
        let alpha = if resp.held || resp.hovered { 0.5 } else { 0.28 };
        let c = ui.c.text.alpha(alpha);
        ui.g.fill_round(Rect::new(bar.x + 3.0, thumb_y + 2.0, 5.0, thumb_h - 4.0), 2.5, c);
    }

    fn toolbar(&mut self, ui: &mut Ui, r: Rect, current: usize, out: &mut Vec<ReaderAction>) {
        let c = ui.c;
        ui.g.fill_rect(r, c.surface);
        ui.separator(r.x, r.bottom() - 1.0, r.w);
        let b = |x: f32| Rect::new(x, r.y + 8.0, 34.0, 32.0);
        if ui.button_ex(id("pdf-back"), Rect::new(r.x + 8.0, r.y + 8.0, 96.0, 32.0), "Library", Some("\u{E72B}"), ButtonKind::Ghost, true) {
            out.push(ReaderAction::Close);
        }

        let mut x = r.right() - 8.0 - 34.0;
        let (fs_icon, fs_tip) = if self.fullscreen { ("\u{E73F}", "Leave full screen (F11)") } else { ("\u{E740}", "Full screen (F11)") };
        if ui.icon_button(id("pdf-full"), b(x), fs_icon, fs_tip, false) {
            out.push(ReaderAction::ToggleFullscreen);
        }
        x -= 38.0;
        let dpi = ui.g.dpi;
        if ui.icon_button(id("pdf-zoom-in"), b(x), "\u{E8A3}", "Zoom in (Ctrl+=)", false) {
            self.step_zoom(dpi, true);
        }
        let zr = Rect::new(x - 64.0, r.y + 8.0, 60.0, 32.0);
        let label = format!("{:.0}%", self.zoom_now(dpi) * 100.0);
        let fit = self.zoom.is_none();
        if ui.button_ex(id("pdf-fit"), zr, &label, None, if fit { ButtonKind::Tonal } else { ButtonKind::Ghost }, true) {
            self.set_zoom(if fit { Some(1.0) } else { None });
        }
        ui.tip(id("pdf-fit"), zr, if fit { "Fitting the width: click for the printed size (Ctrl+0 to fit again)" } else { "Fit the width (Ctrl+0)" });
        x -= 68.0 + 38.0;
        if ui.icon_button(id("pdf-zoom-out"), b(x), "\u{E71F}", "Zoom out (Ctrl+−)", false) {
            self.step_zoom(dpi, false);
        }

        // Title, and where the reader is.
        let page = format!("Page {} of {}", current + 1, self.page_count());
        let left = r.x + 150.0;
        let width = (x - 16.0 - left).max(0.0);
        let title = TextStyle::ui(14.0).semibold();
        let tt = ui.measure(&self.meta.title, &title, TextOpts::ellipsize((width - 130.0).max(40.0)));
        let pt = ui.measure(&page, &TextStyle::ui(13.0), TextOpts::default());
        let total = tt.w + 14.0 + pt.w;
        let tx = left + ((width - total) / 2.0).max(0.0);
        ui.g.draw_text(tt, tx, r.y + (r.h - tt.h) / 2.0, c.text);
        ui.g.draw_text(pt, tx + tt.w + 14.0, r.y + (r.h - pt.h) / 2.0, c.muted);
    }
}
