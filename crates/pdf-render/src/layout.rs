use engine::{PageSize, PixelRect};
use std::ops::Range;

/// Tiles are squares of this many pixels per zoom level, like a map viewer. Edge tiles are cut to the page.
pub const TILE: u32 = 512;

/// Where a page sits in the document, in pixels at the layout's scale.
#[derive(Clone, Copy, Debug)]
pub struct PagePos {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
}

/// A rectangle of the document in document pixels, usually the part shown in the window.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
}

impl Viewport {
    fn bottom(&self) -> i64 {
        self.y + i64::from(self.height)
    }
}

/// Pages in one continuous column at one scale, centered.
pub struct Layout {
    /// Pixels per point.
    pub scale: f32,
    /// Space around and between pages, in pixels.
    pub gap: u32,
    /// Width of the window the pages are centered in.
    pub view_width: u32,
    pub width: i64,
    pub height: i64,
    pub pages: Vec<PagePos>,
}

impl Layout {
    pub fn new(sizes: &[PageSize], scale: f32, gap: u32, view_width: u32) -> Self {
        let px = |pt: f32| ((pt * scale).round() as u32).max(1);
        let widest = sizes.iter().map(|s| px(s.width)).max().unwrap_or(0);
        let width = i64::from((widest + 2 * gap).max(view_width));
        let mut y = i64::from(gap);
        let pages = sizes
            .iter()
            .map(|s| {
                let (w, h) = (px(s.width), px(s.height));
                let pos = PagePos { x: (width - i64::from(w)) / 2, y, width: w, height: h };
                y += i64::from(h + gap);
                pos
            })
            .collect();
        Self { scale, gap, view_width, width, height: y, pages }
    }

    /// Pages that overlap the rows `top..bottom`.
    pub fn pages_between(&self, top: i64, bottom: i64) -> Range<usize> {
        let first = self.pages.partition_point(|p| p.y + i64::from(p.height) <= top);
        let end = self.pages.partition_point(|p| p.y < bottom);
        first..end.max(first)
    }

    /// The page at row `y`, or the next one when `y` falls between pages.
    pub fn page_at(&self, y: i64) -> usize {
        let i = self.pages.partition_point(|p| p.y + i64::from(p.height) <= y);
        i.min(self.pages.len().saturating_sub(1))
    }

    /// The page that takes up most of `view`.
    pub fn current_page(&self, view: &Viewport) -> usize {
        self.pages_between(view.y, view.bottom())
            .min_by_key(|&i| {
                let p = self.pages[i];
                -((p.y + i64::from(p.height)).min(view.bottom()) - p.y.max(view.y))
            })
            .unwrap_or_else(|| self.page_at(view.y))
    }

    /// The tiles of `page` that overlap `rect`: column, row, and the tile's area in page pixels.
    pub fn tiles(&self, page: usize, rect: &Viewport) -> impl Iterator<Item = (u32, u32, PixelRect)> + use<> {
        let p = self.pages[page];
        let (w, h) = (i64::from(p.width), i64::from(p.height));
        let (x0, x1) = ((rect.x - p.x).clamp(0, w), (rect.x + i64::from(rect.width) - p.x).clamp(0, w));
        let (y0, y1) = ((rect.y - p.y).clamp(0, h), (rect.bottom() - p.y).clamp(0, h));
        let t = i64::from(TILE);
        let span = |a: i64, b: i64| if a < b { (a / t) as u32..((b - 1) / t + 1) as u32 } else { 0..0 };
        let (cols, rows) = (span(x0, x1), span(y0, y1));
        rows.flat_map(move |row| {
            cols.clone().map(move |col| {
                let (x, y) = (col * TILE, row * TILE);
                (col, row, PixelRect { x, y, width: TILE.min(p.width - x), height: TILE.min(p.height - y) })
            })
        })
    }
}
