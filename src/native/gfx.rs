//! Drawing backend: Direct2D with a *software* DC render target plus DirectWrite text.
//!
//! The software target rasterises on the CPU (WARP), so no GPU vendor driver is ever loaded
//! into the process. Everything here works in DIPs (1/96 inch); the render target's DPI does
//! the scaling, so callers never deal with physical pixels.

use crate::reader::rich_text::{InlineStyle, Span};
use crate::reader::theme::Rgb;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use windows::core::{Interface, Result, BOOL, HSTRING};
use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, RECT};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::HDC;
use windows_numerics::Vector2;

/// Layouts not used for this many frames are dropped from the cache.
const LAYOUT_TTL_FRAMES: u64 = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.right() && p.1 >= self.y && p.1 < self.bottom()
    }
    pub fn shrink(&self, d: f32) -> Rect {
        self.shrink2(d, d)
    }
    pub fn shrink2(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        Rect::new(x, y, (self.right().min(o.right()) - x).max(0.0), (self.bottom().min(o.bottom()) - y).max(0.0))
    }
    pub fn contains_rect(&self, o: &Rect) -> bool {
        o.x >= self.x - 0.5 && o.y >= self.y - 0.5 && o.right() <= self.right() + 0.5 && o.bottom() <= self.bottom() + 0.5
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.right(), bottom: self.bottom() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub fn rgb(c: Rgb) -> Self {
        Self { r: c.0 as f32 / 255.0, g: c.1 as f32 / 255.0, b: c.2 as f32 / 255.0, a: 1.0 }
    }
    pub const fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
    }
    pub fn alpha(self, a: f32) -> Self {
        Self { a: self.a * a, ..self }
    }
    /// Linear blend towards `o` by `t` (0 = self, 1 = o).
    pub fn mix(self, o: Color, t: f32) -> Self {
        Self {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }
    fn d2d(&self) -> D2D1_COLOR_F {
        D2D1_COLOR_F { r: self.r, g: self.g, b: self.b, a: self.a }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Family {
    /// The system UI face (Segoe UI Variable / Segoe UI).
    Ui,
    /// Segoe Fluent Icons / Segoe MDL2 Assets glyph font.
    Icons,
    Mono,
    /// A named family, optionally loaded from a font file outside the system collection.
    Named { name: String, file: Option<PathBuf> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub family: Family,
    pub size: f32,
    pub bold: bool,
    pub semibold: bool,
    pub italic: bool,
}

impl TextStyle {
    pub fn ui(size: f32) -> Self {
        Self { family: Family::Ui, size, bold: false, semibold: false, italic: false }
    }
    pub fn icons(size: f32) -> Self {
        Self { family: Family::Icons, ..Self::ui(size) }
    }
    pub fn semibold(mut self) -> Self {
        self.semibold = true;
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    fn weight(&self) -> DWRITE_FONT_WEIGHT {
        if self.bold {
            DWRITE_FONT_WEIGHT_BOLD
        } else if self.semibold {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextOpts {
    /// Wrap width in DIPs; `None` lays the text out on one line.
    pub wrap: Option<f32>,
    pub align: Align,
    /// Line height as a multiple of the font size; `None` uses the font's natural spacing.
    pub line_height: Option<f32>,
    /// Truncate with an ellipsis after this many lines (requires `wrap` for > 1 line).
    pub max_lines: Option<u32>,
    /// Single-line width limit with ellipsis (ignored when `wrap` is set).
    pub ellipsize: Option<f32>,
}

impl Default for TextOpts {
    fn default() -> Self {
        Self { wrap: None, align: Align::Left, line_height: None, max_lines: None, ellipsize: None }
    }
}

impl TextOpts {
    pub fn wrap(w: f32) -> Self {
        Self { wrap: Some(w.max(1.0)), ..Default::default() }
    }
    pub fn ellipsize(w: f32) -> Self {
        Self { ellipsize: Some(w.max(1.0)), ..Default::default() }
    }
    pub fn align(mut self, a: Align) -> Self {
        self.align = a;
        self
    }
    pub fn line_height(mut self, m: f32) -> Self {
        self.line_height = Some(m);
        self
    }
    pub fn max_lines(mut self, n: u32) -> Self {
        self.max_lines = Some(n);
        self
    }
}

/// A laid-out piece of text, valid until the end of the next frame.
#[derive(Debug, Clone, Copy)]
pub struct Text {
    key: u64,
    pub w: f32,
    pub h: f32,
}

struct CachedLayout {
    layout: IDWriteTextLayout,
    w: f32,
    h: f32,
    last_used: u64,
}

pub struct Gfx {
    dw: IDWriteFactory,
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    d2d: ID2D1Factory,
    formats: HashMap<u64, IDWriteTextFormat>,
    layouts: HashMap<u64, CachedLayout>,
    collections: HashMap<PathBuf, IDWriteFontCollection>,
    ui_family: &'static str,
    icon_family: &'static str,
    mono_family: &'static str,
    frame: u64,
    clips: Vec<Rect>,
    /// False during input-only frames: layout and hit-testing run, nothing is drawn.
    pub drawing: bool,
    pub dpi: f32,
}

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl Gfx {
    pub fn new(dpi: f32) -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let (rt, brush) = Self::create_target(&d2d)?;
            let mut g = Self {
                dw,
                rt,
                brush,
                d2d,
                formats: HashMap::new(),
                layouts: HashMap::new(),
                collections: HashMap::new(),
                ui_family: "Segoe UI",
                icon_family: "Segoe MDL2 Assets",
                mono_family: "Consolas",
                frame: 0,
                clips: Vec::new(),
                drawing: false,
                dpi,
            };
            if g.has_system_family("Segoe UI Variable Text") {
                g.ui_family = "Segoe UI Variable Text";
            }
            if g.has_system_family("Segoe Fluent Icons") {
                g.icon_family = "Segoe Fluent Icons";
            }
            if g.has_system_family("Cascadia Mono") {
                g.mono_family = "Cascadia Mono";
            }
            Ok(g)
        }
    }

    unsafe fn create_target(d2d: &ID2D1Factory) -> Result<(ID2D1DCRenderTarget, ID2D1SolidColorBrush)> {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
            dpiX: 0.0,
            dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let rt = d2d.CreateDCRenderTarget(&props)?;
        rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);
        let brush = rt.CreateSolidColorBrush(&Color::from_u8(0, 0, 0).d2d(), None)?;
        Ok((rt, brush))
    }

    pub fn has_system_family(&self, name: &str) -> bool {
        unsafe {
            let mut coll = None;
            if self.dw.GetSystemFontCollection(&mut coll, false).is_err() {
                return false;
            }
            let Some(coll) = coll else { return false };
            let mut idx = 0u32;
            let mut exists = BOOL(0);
            coll.FindFamilyName(&HSTRING::from(name), &mut idx, &mut exists).is_ok() && exists.as_bool()
        }
    }

    /// English names of every installed font family, sorted case-insensitively.
    pub fn system_font_families(&self) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let mut coll = None;
            if self.dw.GetSystemFontCollection(&mut coll, false).is_err() {
                return out;
            }
            let Some(coll) = coll else { return out };
            for i in 0..coll.GetFontFamilyCount() {
                if let Some(name) = coll.GetFontFamily(i).ok().and_then(|f| f.GetFamilyNames().ok()).and_then(|n| localized_name(&n)) {
                    out.push(name);
                }
            }
        }
        out.sort_by_key(|n| n.to_lowercase());
        out.dedup();
        out
    }

    /// Loads a .ttf/.otf file into a private collection and returns its family name.
    pub fn load_font_file(&mut self, path: &Path) -> Option<String> {
        let coll = self.collection_for(path)?;
        unsafe { coll.GetFontFamily(0).ok().and_then(|f| f.GetFamilyNames().ok()).and_then(|n| localized_name(&n)) }
    }

    fn collection_for(&mut self, path: &Path) -> Option<IDWriteFontCollection> {
        if let Some(c) = self.collections.get(path) {
            return Some(c.clone());
        }
        let coll = unsafe {
            let f5: IDWriteFactory5 = self.dw.cast().ok()?;
            let builder = f5.CreateFontSetBuilder().ok()?;
            let file = f5.CreateFontFileReference(&HSTRING::from(path.as_os_str()), None).ok()?;
            builder.AddFontFile(&file).ok()?;
            let set = builder.CreateFontSet().ok()?;
            let coll1 = f5.CreateFontCollectionFromFontSet(&set).ok()?;
            if coll1.GetFontFamilyCount() == 0 {
                return None;
            }
            coll1.cast::<IDWriteFontCollection>().ok()?
        };
        self.collections.insert(path.to_path_buf(), coll.clone());
        Some(coll)
    }

    fn format(&mut self, style: &TextStyle) -> Option<IDWriteTextFormat> {
        let key = hash_of(&(&style.family, style.size.to_bits(), style.bold, style.semibold, style.italic));
        if let Some(f) = self.formats.get(&key) {
            return Some(f.clone());
        }
        let (family, coll) = match &style.family {
            Family::Ui => (self.ui_family.to_string(), None),
            Family::Icons => (self.icon_family.to_string(), None),
            Family::Mono => (self.mono_family.to_string(), None),
            Family::Named { name, file } => (name.clone(), file.as_deref().and_then(|p| self.collection_for(p))),
        };
        let fmt = unsafe {
            self.dw
                .CreateTextFormat(
                    &HSTRING::from(family),
                    coll.as_ref(),
                    style.weight(),
                    if style.italic { DWRITE_FONT_STYLE_ITALIC } else { DWRITE_FONT_STYLE_NORMAL },
                    DWRITE_FONT_STRETCH_NORMAL,
                    style.size,
                    windows::core::w!("en-us"),
                )
                .ok()?
        };
        self.formats.insert(key, fmt.clone());
        Some(fmt)
    }

    /// Lays out `text` (cached across frames) and returns its size.
    pub fn layout(&mut self, text: &str, style: &TextStyle, opts: TextOpts) -> Text {
        self.layout_rich(text, &[], style, opts)
    }

    /// Like `layout`, with italic/bold/strikethrough spans (byte ranges into `text`).
    pub fn layout_rich(&mut self, text: &str, spans: &[Span], style: &TextStyle, opts: TextOpts) -> Text {
        let key = hash_of(&(
            text,
            spans,
            &style.family,
            style.size.to_bits(),
            style.bold,
            style.semibold,
            style.italic,
            opts.wrap.map(f32::to_bits),
            opts.align,
            opts.line_height.map(f32::to_bits),
            opts.max_lines,
            opts.ellipsize.map(f32::to_bits),
        ));
        if let Some(c) = self.layouts.get_mut(&key) {
            c.last_used = self.frame;
            return Text { key, w: c.w, h: c.h };
        }
        let Some(fmt) = self.format(style) else {
            return Text { key: 0, w: 0.0, h: style.size };
        };
        let built = unsafe { self.build_layout(&fmt, text, spans, style, &opts) };
        match built {
            Ok((layout, w, h)) => {
                self.layouts.insert(key, CachedLayout { layout, w, h, last_used: self.frame });
                Text { key, w, h }
            }
            Err(_) => Text { key: 0, w: 0.0, h: style.size },
        }
    }

    /// Measures without caching: (height, bottom of each line). Used to paginate whole
    /// chapters without filling the layout cache with off-screen paragraphs.
    pub fn measure_lines(&mut self, text: &str, spans: &[Span], style: &TextStyle, opts: TextOpts) -> (f32, Vec<f32>) {
        let Some(fmt) = self.format(style) else { return (style.size, vec![style.size]) };
        match unsafe { self.build_layout(&fmt, text, spans, style, &opts) } {
            Ok((layout, _, h)) => (h, unsafe { line_bottoms(&layout) }),
            Err(_) => (style.size, vec![style.size]),
        }
    }

    /// Rectangles (relative to the layout origin) covering a byte range of the laid-out text.
    pub fn range_rects(&self, t: Text, text: &str, start: usize, end: usize) -> Vec<Rect> {
        let Some(c) = self.layouts.get(&t.key) else { return Vec::new() };
        let start = start.min(text.len());
        let end = end.clamp(start, text.len());
        let pos = utf16_len(&text[..start]);
        let len = utf16_len(&text[start..end]);
        unsafe {
            let mut count = 0u32;
            let _ = c.layout.HitTestTextRange(pos, len, 0.0, 0.0, None, &mut count);
            if count == 0 {
                return Vec::new();
            }
            let mut buf = vec![DWRITE_HIT_TEST_METRICS::default(); count as usize];
            if c.layout.HitTestTextRange(pos, len, 0.0, 0.0, Some(&mut buf), &mut count).is_err() {
                return Vec::new();
            }
            buf.iter().take(count as usize).map(|m| Rect::new(m.left, m.top, m.width, m.height)).collect()
        }
    }

    unsafe fn build_layout(
        &self,
        fmt: &IDWriteTextFormat,
        text: &str,
        spans: &[Span],
        style: &TextStyle,
        opts: &TextOpts,
    ) -> Result<(IDWriteTextLayout, f32, f32)> {
        let w16 = wide(text);
        let line_px = opts.line_height.map(|m| style.size * m);
        let max_h = match (opts.max_lines, line_px) {
            (Some(n), Some(lh)) => lh * n as f32 + 1.0,
            (Some(n), None) => style.size * 1.33 * n as f32 + 1.0,
            _ => f32::MAX,
        };
        let max_w = opts.wrap.or(opts.ellipsize).unwrap_or(f32::MAX);
        let layout = self.dw.CreateTextLayout(&w16, fmt, max_w, max_h)?;
        if opts.wrap.is_none() {
            layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        }
        if let Some(lh) = line_px {
            layout.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, lh, lh * 0.8)?;
        }
        if opts.wrap.is_some() {
            layout.SetTextAlignment(match opts.align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                Align::Justify => DWRITE_TEXT_ALIGNMENT_JUSTIFIED,
            })?;
        }
        if opts.ellipsize.is_some() || opts.max_lines.is_some() {
            let sign = self.dw.CreateEllipsisTrimmingSign(fmt)?;
            let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
            layout.SetTrimming(&trimming, &sign)?;
        }
        for span in spans {
            let start = span.start.min(text.len());
            let end = span.end.clamp(start, text.len());
            let range = DWRITE_TEXT_RANGE { startPosition: utf16_len(&text[..start]), length: utf16_len(&text[start..end]) };
            match span.style {
                InlineStyle::Italic => layout.SetFontStyle(DWRITE_FONT_STYLE_ITALIC, range)?,
                InlineStyle::Bold => layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, range)?,
                InlineStyle::Strike => layout.SetStrikethrough(true, range)?,
            }
        }
        let mut m = DWRITE_TEXT_METRICS::default();
        layout.GetMetrics(&mut m)?;
        let w = if opts.wrap.is_some() { m.width } else { m.widthIncludingTrailingWhitespace.min(max_w) };
        let h = m.height.min(max_h);
        Ok((layout, w, h))
    }

    // ---- frame lifecycle -------------------------------------------------------------

    pub fn begin_draw(&mut self, hdc: HDC, px: &RECT) -> Result<()> {
        unsafe {
            self.rt.BindDC(hdc, px)?;
            self.rt.SetDpi(self.dpi, self.dpi);
            self.rt.BeginDraw();
        }
        self.drawing = true;
        Ok(())
    }

    pub fn end_draw(&mut self) {
        self.drawing = false;
        while self.clips.pop().is_some() {
            unsafe { self.rt.PopAxisAlignedClip() };
        }
        let res = unsafe { self.rt.EndDraw(None, None) };
        if let Err(e) = res {
            if e.code() == D2DERR_RECREATE_TARGET {
                if let Ok((rt, brush)) = unsafe { Self::create_target(&self.d2d) } {
                    self.rt = rt;
                    self.brush = brush;
                }
            }
        }
    }

    /// Ends a frame (drawn or not) and evicts text layouts that went out of use.
    pub fn end_frame(&mut self) {
        let frame = self.frame;
        self.layouts.retain(|_, c| c.last_used + LAYOUT_TTL_FRAMES >= frame);
        self.frame += 1;
    }

    pub fn set_dpi(&mut self, dpi: f32) {
        self.dpi = dpi;
    }

    // ---- drawing primitives (no-ops while not drawing) ---------------------------------

    fn set_color(&self, c: Color) {
        unsafe { self.brush.SetColor(&c.d2d()) };
    }

    pub fn clear(&mut self, c: Color) {
        if self.drawing {
            unsafe { self.rt.Clear(Some(&c.d2d())) };
        }
    }

    pub fn fill_rect(&mut self, r: Rect, c: Color) {
        if self.drawing {
            self.set_color(c);
            unsafe { self.rt.FillRectangle(&r.d2d(), &self.brush) };
        }
    }

    pub fn fill_round(&mut self, r: Rect, radius: f32, c: Color) {
        if self.drawing {
            self.set_color(c);
            let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
            unsafe { self.rt.FillRoundedRectangle(&rr, &self.brush) };
        }
    }

    pub fn stroke_round(&mut self, r: Rect, radius: f32, width: f32, c: Color) {
        if self.drawing {
            self.set_color(c);
            // Inset by half the stroke so the border stays inside `r`.
            let r = r.shrink(width / 2.0);
            let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: radius, radiusY: radius };
            unsafe { self.rt.DrawRoundedRectangle(&rr, &self.brush, width, None) };
        }
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, c: Color) {
        if self.drawing {
            self.set_color(c);
            unsafe { self.rt.DrawLine(Vector2 { X: x0, Y: y0 }, Vector2 { X: x1, Y: y1 }, &self.brush, width, None) };
        }
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, c: Color) {
        if self.drawing {
            self.set_color(c);
            let e = D2D1_ELLIPSE { point: Vector2 { X: cx, Y: cy }, radiusX: r, radiusY: r };
            unsafe { self.rt.FillEllipse(&e, &self.brush) };
        }
    }

    pub fn draw_text(&mut self, t: Text, x: f32, y: f32, c: Color) {
        if !self.drawing {
            return;
        }
        if let Some(cached) = self.layouts.get(&t.key) {
            self.set_color(c);
            unsafe {
                self.rt.DrawTextLayout(
                    Vector2 { X: x, Y: y },
                    &cached.layout,
                    &self.brush,
                    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                )
            };
        }
    }

    /// Lays out and draws in one go; returns the text size.
    pub fn text(&mut self, s: &str, style: &TextStyle, opts: TextOpts, x: f32, y: f32, c: Color) -> Text {
        let t = self.layout(s, style, opts);
        self.draw_text(t, x, y, c);
        t
    }

    pub fn push_clip(&mut self, r: Rect) {
        let r = match self.clips.last() {
            Some(top) => top.intersect(&r),
            None => r,
        };
        self.clips.push(r);
        if self.drawing {
            unsafe { self.rt.PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_ALIASED) };
        }
    }

    pub fn pop_clip(&mut self) {
        if self.clips.pop().is_some() && self.drawing {
            unsafe { self.rt.PopAxisAlignedClip() };
        }
    }

    pub fn clip(&self) -> Option<Rect> {
        self.clips.last().copied()
    }
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

/// Cumulative bottom edge of every line in a layout.
unsafe fn line_bottoms(layout: &IDWriteTextLayout) -> Vec<f32> {
    let mut count = 0u32;
    let _ = layout.GetLineMetrics(None, &mut count);
    let mut buf = vec![DWRITE_LINE_METRICS::default(); count as usize];
    if count == 0 || layout.GetLineMetrics(Some(&mut buf), &mut count).is_err() {
        return Vec::new();
    }
    let mut y = 0.0;
    buf.iter()
        .take(count as usize)
        .map(|l| {
            y += l.height;
            y
        })
        .collect()
}

unsafe fn localized_name(names: &IDWriteLocalizedStrings) -> Option<String> {
    let mut idx = 0u32;
    let mut exists = BOOL(0);
    let _ = names.FindLocaleName(windows::core::w!("en-us"), &mut idx, &mut exists);
    if !exists.as_bool() {
        idx = 0;
    }
    let len = names.GetStringLength(idx).ok()? as usize;
    let mut buf = vec![0u16; len + 1];
    names.GetString(idx, &mut buf).ok()?;
    Some(String::from_utf16_lossy(&buf[..len]))
}
