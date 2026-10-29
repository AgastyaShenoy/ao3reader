//! A small immediate-mode widget kit on top of `Gfx`.
//!
//! Frames only run in response to input, timers or background results (never continuously),
//! so an idle window costs no CPU. Each input event runs one *input frame* (layout and
//! hit-testing, nothing drawn); WM_PAINT then runs a *paint frame* with the same code.

use super::edit::EditBox;
use super::gfx::*;
use crate::reader::theme::ThemePalette;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::HFONT;

pub type Id = u64;

pub fn id(s: &str) -> Id {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

pub fn id2(base: Id, salt: impl Hash) -> Id {
    let mut h = DefaultHasher::new();
    base.hash(&mut h);
    salt.hash(&mut h);
    h.finish()
}

#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub vk: u16,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Default)]
pub struct Input {
    pub mouse: (f32, f32),
    pub mouse_in: bool,
    pub down: bool,
    pub pressed: bool,
    pub released: bool,
    /// Wheel movement in notches (positive = away from the user); fractional for touchpads.
    pub wheel: f32,
    pub keys: Vec<Key>,
}

impl Input {
    pub fn clear_transient(&mut self) {
        self.pressed = false;
        self.released = false;
        self.wheel = 0.0;
        self.keys.clear();
    }
}

/// Theme colours resolved for drawing.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub bg: Color,
    pub surface: Color,
    pub card: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub border: Color,
    pub quote_bg: Color,
    pub notes_bg: Color,
    pub on_accent: Color,
    pub input_bg: Color,
    pub danger: Color,
    pub success: Color,
    pub is_dark: bool,
}

impl Colors {
    pub fn from_palette(p: &ThemePalette) -> Self {
        let text = Color::rgb(p.text);
        let card = Color::rgb(p.card);
        Self {
            bg: Color::rgb(p.bg),
            surface: Color::rgb(p.surface),
            card,
            text,
            muted: Color::rgb(p.text_muted),
            accent: Color::rgb(p.accent),
            border: Color::rgb(p.border),
            quote_bg: Color::rgb(p.quote_bg),
            notes_bg: Color::rgb(p.notes_bg),
            on_accent: Color::from_u8(255, 255, 255),
            input_bg: if p.is_dark { Color::rgb(p.bg) } else { Color::from_u8(255, 255, 255) },
            danger: Color::from_u8(220, 60, 60),
            success: Color::from_u8(34, 160, 84),
            is_dark: p.is_dark,
        }
    }
    pub fn hover(&self) -> Color {
        self.text.alpha(0.07)
    }
    pub fn pressed(&self) -> Color {
        self.text.alpha(0.12)
    }
}

#[derive(Default)]
pub struct ScrollState {
    pub offset: f32,
    pub content_h: f32,
    drag_grab: Option<f32>,
}

/// State that persists between frames.
pub struct Memory {
    pub active: Option<Id>,
    pub hot: Option<Id>,
    pub scroll: HashMap<Id, ScrollState>,
    pub edits: HashMap<Id, EditBox>,
    pub edit_ids: HashMap<i32, Id>,
    next_ctrl_id: i32,
    pub edit_font: HFONT,
    pub started: Instant,
}

impl Memory {
    pub fn new(edit_font: HFONT) -> Self {
        Self {
            active: None,
            hot: None,
            scroll: HashMap::new(),
            edits: HashMap::new(),
            edit_ids: HashMap::new(),
            next_ctrl_id: 100,
            edit_font,
            started: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Resp {
    pub hovered: bool,
    pub held: bool,
    pub clicked: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InputResp {
    pub changed: bool,
    pub submitted: bool,
    pub escaped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

pub struct FrameOut {
    pub repaint: bool,
    pub wake_ms: Option<u32>,
    pub hand_cursor: bool,
}

pub struct Ui<'a> {
    pub g: &'a mut Gfx,
    pub input: &'a Input,
    pub mem: &'a mut Memory,
    pub c: Colors,
    pub hwnd: HWND,
    pub size: (f32, f32),
    hot_now: Option<Id>,
    /// Set once a scroll area has consumed this frame's wheel input (innermost first).
    wheel_used: bool,
    /// Regions covered by overlays; content underneath gets no hover or clicks.
    blockers: Vec<Rect>,
    in_overlay: bool,
    /// Layout-only pass: nothing is drawn and widgets do not react.
    dry: bool,
    repaint: bool,
    wake_ms: Option<u32>,
    hand: bool,
}

pub const RADIUS: f32 = 6.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.0;
pub const CONTROL_H: f32 = 32.0;

impl<'a> Ui<'a> {
    pub fn new(g: &'a mut Gfx, input: &'a Input, mem: &'a mut Memory, c: Colors, hwnd: HWND, size: (f32, f32)) -> Self {
        for e in mem.edits.values_mut() {
            e.seen = false;
        }
        Self { g, input, mem, c, hwnd, size, hot_now: None, wheel_used: false, blockers: Vec::new(), in_overlay: false, dry: false, repaint: false, wake_ms: None, hand: false }
    }

    /// Finishes the frame: retires edit controls that were not shown, tracks hover changes.
    pub fn finish(self) -> FrameOut {
        let gone: Vec<Id> = self.mem.edits.iter().filter(|(_, e)| !e.seen).map(|(k, _)| *k).collect();
        for k in gone {
            if let Some(e) = self.mem.edits.remove(&k) {
                self.mem.edit_ids.remove(&e.ctrl_id);
                if unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() } == e.hwnd {
                    unsafe {
                        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(self.hwnd));
                    }
                }
                e.destroy();
            }
        }
        let mut repaint = self.repaint;
        if self.mem.hot != self.hot_now {
            self.mem.hot = self.hot_now;
            repaint = true;
        }
        if self.input.released {
            self.mem.active = None;
            repaint = true;
        }
        self.g.end_frame();
        FrameOut { repaint, wake_ms: self.wake_ms, hand_cursor: self.hand }
    }

    pub fn request_repaint(&mut self) {
        self.repaint = true;
    }

    /// Asks for another frame after `ms` milliseconds (animations, timeouts).
    pub fn wake_after(&mut self, ms: u32) {
        self.wake_ms = Some(self.wake_ms.map_or(ms, |w| w.min(ms)));
    }

    pub fn seconds(&self) -> f32 {
        self.mem.started.elapsed().as_secs_f32()
    }

    pub fn hit(&self, r: Rect) -> bool {
        let m = self.input.mouse;
        self.input.mouse_in
            && !self.dry
            && r.contains(m)
            && self.g.clip().is_none_or(|c| c.contains(m))
            && (self.in_overlay || !self.blockers.iter().any(|b| b.contains(m)))
    }

    /// Marks a region as covered by an overlay drawn later in the frame.
    pub fn block(&mut self, r: Rect) {
        self.blockers.push(r);
    }

    /// Runs `f` as overlay content, which is not blocked by `block`ed regions.
    pub fn overlay<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let prev = std::mem::replace(&mut self.in_overlay, true);
        let r = f(self);
        self.in_overlay = prev;
        r
    }

    /// Runs `f` for its layout only: nothing is drawn and no widget reacts.
    pub fn measure_only<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let drawing = std::mem::replace(&mut self.g.drawing, false);
        let dry = std::mem::replace(&mut self.dry, true);
        let r = f(self);
        self.g.drawing = drawing;
        self.dry = dry;
        r
    }

    /// A key pressed this frame with exactly these modifiers.
    pub fn key_combo(&self, vk: u16, ctrl: bool, shift: bool) -> bool {
        self.input.keys.iter().any(|k| k.vk == vk && k.ctrl == ctrl && k.shift == shift && !k.alt)
    }

    pub fn interact(&mut self, id: Id, r: Rect) -> Resp {
        let hovered = self.hit(r) && self.mem.active.is_none_or(|a| a == id);
        if hovered {
            self.hot_now = Some(id);
            self.hand = true;
        }
        if hovered && self.input.pressed && self.mem.active.is_none() {
            self.mem.active = Some(id);
        }
        let held = self.mem.active == Some(id);
        let clicked = held && self.input.released && hovered;
        if clicked {
            self.repaint = true;
        }
        Resp { hovered, held: held && self.input.down, clicked }
    }

    pub fn key_pressed(&self, vk: u16) -> bool {
        self.input.keys.iter().any(|k| k.vk == vk && !k.ctrl && !k.alt)
    }

    // ---- text -----------------------------------------------------------------------

    pub fn text(&mut self, s: &str, style: &TextStyle, x: f32, y: f32, color: Color) -> Text {
        self.g.text(s, style, TextOpts::default(), x, y, color)
    }

    pub fn text_opts(&mut self, s: &str, style: &TextStyle, opts: TextOpts, x: f32, y: f32, color: Color) -> Text {
        self.g.text(s, style, opts, x, y, color)
    }

    pub fn measure(&mut self, s: &str, style: &TextStyle, opts: TextOpts) -> Text {
        self.g.layout(s, style, opts)
    }

    /// Text vertically centred in `r`.
    pub fn text_in(&mut self, s: &str, style: &TextStyle, r: Rect, align: Align, color: Color) -> Text {
        let t = self.g.layout(s, style, TextOpts::ellipsize(r.w));
        let x = match align {
            Align::Center => r.x + (r.w - t.w) / 2.0,
            Align::Right => r.right() - t.w,
            _ => r.x,
        };
        self.g.draw_text(t, x, r.y + (r.h - t.h) / 2.0, color);
        t
    }

    // ---- containers -------------------------------------------------------------------

    pub fn card(&mut self, r: Rect) {
        let c = self.c;
        self.g.fill_round(r, 8.0, c.card);
        self.g.stroke_round(r, 8.0, 1.0, c.border);
    }

    pub fn separator(&mut self, x: f32, y: f32, w: f32) {
        let c = self.c.border;
        self.g.line(x, y + 0.5, x + w, y + 0.5, 1.0, c);
    }

    /// Vertical scroll container. `content` draws at the given top y and returns its height.
    /// Nested areas get the wheel first; it reaches the outer one only if they cannot scroll.
    pub fn scroll_area(&mut self, id: Id, r: Rect, content: impl FnOnce(&mut Ui, Rect, f32) -> f32) {
        let hovered = self.hit(r);
        let (mut offset, content_h) = {
            let s = self.mem.scroll.entry(id).or_default();
            (s.offset, s.content_h)
        };
        let max = (content_h - r.h).max(0.0);

        // Scrollbar thumb drag.
        let bar = Rect::new(r.right() - 10.0, r.y, 10.0, r.h);
        let thumb_h = if content_h > 0.0 { (r.h * r.h / content_h).max(32.0) } else { r.h };
        let thumb_y = if max > 0.0 { r.y + (r.h - thumb_h) * (offset / max) } else { r.y };
        let thumb = Rect::new(bar.x + 2.0, thumb_y, 6.0, thumb_h);
        let bar_id = id2(id, "bar");
        let resp = if max > 0.0 { self.interact(bar_id, bar) } else { Resp::default() };
        if resp.held {
            let grab = {
                let s = self.mem.scroll.entry(id).or_default();
                *s.drag_grab.get_or_insert(if thumb.contains(self.input.mouse) { self.input.mouse.1 - thumb_y } else { thumb_h / 2.0 })
            };
            let t = (self.input.mouse.1 - grab - r.y) / (r.h - thumb_h).max(1.0);
            offset = t * max;
            self.repaint = true;
        } else if let Some(s) = self.mem.scroll.get_mut(&id) {
            s.drag_grab = None;
        }
        offset = offset.clamp(0.0, max);

        self.g.push_clip(r);
        let h = content(self, r, r.y - offset);
        self.g.pop_clip();

        let max = (h - r.h).max(0.0);
        if hovered && self.input.wheel != 0.0 && !self.wheel_used && max > 0.0 {
            // Takes effect in the paint frame that follows this input frame.
            offset -= self.input.wheel * 48.0;
            self.wheel_used = true;
            self.repaint = true;
        }
        let offset = offset.clamp(0.0, max);
        if max > 0.0 {
            let thumb_h = (r.h * r.h / h).max(32.0);
            let thumb_y = r.y + (r.h - thumb_h) * (offset / max);
            let alpha = if resp.held || resp.hovered { 0.45 } else { 0.22 };
            let c = self.c.text.alpha(alpha);
            self.g.fill_round(Rect::new(bar.x + 3.0, thumb_y + 2.0, 4.0, thumb_h - 4.0), 2.0, c);
        }
        let s = self.mem.scroll.entry(id).or_default();
        if s.content_h != h {
            self.repaint = true;
        }
        s.offset = offset;
        s.content_h = h;
    }

    pub fn scroll_to_top(&mut self, id: Id) {
        if let Some(s) = self.mem.scroll.get_mut(&id) {
            s.offset = 0.0;
        }
    }

    // ---- widgets ----------------------------------------------------------------------

    pub fn button_width(&mut self, label: &str, icon: Option<&str>) -> f32 {
        let tw = if label.is_empty() { 0.0 } else { self.measure(label, &TextStyle::ui(BODY), TextOpts::default()).w };
        let iw = if icon.is_some() { 16.0 + if label.is_empty() { 0.0 } else { 8.0 } } else { 0.0 };
        tw + iw + 28.0
    }

    /// Square icon-only ghost button.
    pub fn icon_button(&mut self, id: Id, r: Rect, icon: &str, active: bool) -> bool {
        if active {
            let c = self.c.accent.alpha(0.18);
            self.g.fill_round(r, RADIUS, c);
        }
        self.button_ex(id, r, "", Some(icon), ButtonKind::Ghost, true)
    }

    pub fn button(&mut self, id: Id, r: Rect, label: &str) -> bool {
        self.button_ex(id, r, label, None, ButtonKind::Secondary, true)
    }

    pub fn button_ex(&mut self, id: Id, r: Rect, label: &str, icon: Option<&str>, kind: ButtonKind, enabled: bool) -> bool {
        let resp = if enabled { self.interact(id, r) } else { Resp::default() };
        let c = self.c;
        let (fill, fg, border) = match kind {
            ButtonKind::Primary => (c.accent, c.on_accent, None),
            ButtonKind::Danger => (c.danger, c.on_accent, None),
            ButtonKind::Secondary => (c.card, c.text, Some(c.border)),
            ButtonKind::Ghost => (Color { a: 0.0, ..c.card }, c.text, None),
        };
        self.g.fill_round(r, RADIUS, fill);
        if let Some(b) = border {
            self.g.stroke_round(r, RADIUS, 1.0, b);
        }
        if resp.held {
            self.g.fill_round(r, RADIUS, c.pressed());
        } else if resp.hovered {
            self.g.fill_round(r, RADIUS, c.hover());
        }
        let fg = if enabled { fg } else { fg.alpha(0.4) };
        let label_w = if label.is_empty() { 0.0 } else { self.measure(label, &TextStyle::ui(BODY), TextOpts::default()).w };
        let icon_w = if icon.is_some() { 16.0 } else { 0.0 };
        let gap = if icon.is_some() && !label.is_empty() { 8.0 } else { 0.0 };
        let total = label_w + icon_w + gap;
        let mut x = r.x + ((r.w - total) / 2.0).max(10.0);
        if let Some(ic) = icon {
            let t = self.measure(ic, &TextStyle::icons(15.0), TextOpts::default());
            self.g.draw_text(t, x + (16.0 - t.w) / 2.0, r.y + (r.h - t.h) / 2.0, fg);
            x += icon_w + gap;
        }
        if !label.is_empty() {
            let style = if kind == ButtonKind::Primary { TextStyle::ui(BODY).semibold() } else { TextStyle::ui(BODY) };
            let t = self.g.layout(label, &style, TextOpts::ellipsize((r.right() - x - 8.0).max(1.0)));
            self.g.draw_text(t, x, r.y + (r.h - t.h) / 2.0, fg);
        }
        resp.clicked
    }

    /// Windows 11 style on/off switch (40×20) at (x, y).
    pub fn toggle(&mut self, id: Id, x: f32, y: f32, value: &mut bool) -> bool {
        let r = Rect::new(x, y, 40.0, 20.0);
        let resp = self.interact(id, r.shrink(-4.0));
        let mut changed = false;
        if resp.clicked {
            *value = !*value;
            changed = true;
        }
        let c = self.c;
        if *value {
            self.g.fill_round(r, 10.0, c.accent);
            self.g.fill_circle(r.right() - 10.0, r.y + 10.0, 6.0, c.on_accent);
        } else {
            self.g.stroke_round(r, 10.0, 1.0, c.muted);
            self.g.fill_circle(r.x + 10.0, r.y + 10.0, 5.0, c.muted);
        }
        if resp.hovered {
            self.g.fill_round(r, 10.0, c.hover());
        }
        changed
    }

    /// Horizontal slider over `min..=max`, snapped to `step`.
    pub fn slider(&mut self, id: Id, r: Rect, value: &mut f32, min: f32, max: f32, step: f32) -> bool {
        let resp = self.interact(id, r);
        let mut changed = false;
        if resp.held {
            let t = ((self.input.mouse.0 - r.x - 8.0) / (r.w - 16.0).max(1.0)).clamp(0.0, 1.0);
            let mut v = min + t * (max - min);
            if step > 0.0 {
                v = (v / step).round() * step;
            }
            v = v.clamp(min, max);
            if (v - *value).abs() > f32::EPSILON {
                *value = v;
                changed = true;
                self.repaint = true;
            }
        }
        let c = self.c;
        let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
        let cy = r.y + r.h / 2.0;
        let x0 = r.x + 8.0;
        let x1 = r.right() - 8.0;
        let xv = x0 + (x1 - x0) * t;
        self.g.fill_round(Rect::new(x0, cy - 2.0, x1 - x0, 4.0), 2.0, c.border);
        self.g.fill_round(Rect::new(x0, cy - 2.0, xv - x0, 4.0), 2.0, c.accent);
        self.g.fill_circle(xv, cy, 9.0, c.card);
        self.g.fill_circle(xv, cy, if resp.held || resp.hovered { 6.0 } else { 5.0 }, c.accent);
        changed
    }

    /// Row of mutually exclusive options.
    pub fn segmented(&mut self, id: Id, r: Rect, options: &[&str], selected: &mut usize) -> bool {
        let c = self.c;
        self.g.fill_round(r, RADIUS, c.bg);
        self.g.stroke_round(r, RADIUS, 1.0, c.border);
        let n = options.len().max(1) as f32;
        let seg_w = r.w / n;
        let mut changed = false;
        for (i, label) in options.iter().enumerate() {
            let sr = Rect::new(r.x + seg_w * i as f32, r.y, seg_w, r.h).shrink(3.0);
            let resp = self.interact(id2(id, i), sr);
            if resp.clicked && *selected != i {
                *selected = i;
                changed = true;
            }
            let is_sel = *selected == i;
            if is_sel {
                self.g.fill_round(sr, RADIUS - 2.0, c.accent);
            } else if resp.hovered {
                self.g.fill_round(sr, RADIUS - 2.0, c.hover());
            }
            let style = if is_sel { TextStyle::ui(13.0).semibold() } else { TextStyle::ui(13.0) };
            self.text_in(label, &style, sr.shrink2(6.0, 0.0), Align::Center, if is_sel { c.on_accent } else { c.text });
        }
        changed
    }

    /// Pill-shaped tab strip with icons; wraps onto more rows when narrow. Returns its height.
    pub fn tab_strip(&mut self, id: Id, x: f32, y: f32, w: f32, items: &[(&str, &str)], selected: &mut usize) -> (f32, bool) {
        let c = self.c;
        let mut cx = x;
        let mut cy = y;
        let h = 34.0;
        let mut changed = false;
        for (i, (icon, label)) in items.iter().enumerate() {
            let bw = self.button_width(label, Some(icon));
            if cx > x && cx + bw > x + w {
                cx = x;
                cy += h + 6.0;
            }
            let r = Rect::new(cx, cy, bw, h);
            let resp = self.interact(id2(id, i), r);
            if resp.clicked && *selected != i {
                *selected = i;
                changed = true;
            }
            let is_sel = *selected == i;
            if is_sel {
                self.g.fill_round(r, h / 2.0, c.accent);
            } else if resp.hovered {
                self.g.fill_round(r, h / 2.0, c.hover());
            }
            let fg = if is_sel { c.on_accent } else { c.text };
            let it = self.measure(icon, &TextStyle::icons(14.0), TextOpts::default());
            self.g.draw_text(it, r.x + 14.0, r.y + (h - it.h) / 2.0, fg);
            let style = if is_sel { TextStyle::ui(13.5).semibold() } else { TextStyle::ui(13.5) };
            let t = self.measure(label, &style, TextOpts::default());
            self.g.draw_text(t, r.x + 38.0, r.y + (h - t.h) / 2.0, fg);
            cx += bw + 6.0;
        }
        (cy + h - y, changed)
    }

    /// Single-line text input backed by a native EDIT control.
    pub fn text_input(&mut self, id: Id, r: Rect, value: &mut String, placeholder: &str, password: bool) -> InputResp {
        let c = self.c;
        let focused = self.mem.edits.get(&id).is_some_and(|e| unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() } == e.hwnd);
        self.g.fill_round(r, RADIUS, c.input_bg);
        self.g.stroke_round(r, RADIUS, if focused { 2.0 } else { 1.0 }, if focused { c.accent } else { c.border });

        if !self.mem.edits.contains_key(&id) {
            let ctrl_id = self.mem.next_ctrl_id;
            self.mem.next_ctrl_id += 1;
            if let Some(e) = EditBox::create(self.hwnd, ctrl_id, password, placeholder, self.mem.edit_font) {
                self.mem.edit_ids.insert(ctrl_id, id);
                self.mem.edits.insert(id, e);
            }
        }
        let scale = self.g.dpi / 96.0;
        let visible = self.g.clip().is_none_or(|clip| clip.contains_rect(&r));
        let mut out = InputResp::default();
        let Some(e) = self.mem.edits.get_mut(&id) else { return out };
        e.seen = true;

        if e.user_changed {
            e.user_changed = false;
            let now = e.read();
            if now != *value {
                *value = now.clone();
                out.changed = true;
            }
            e.text = now;
        } else if e.text != *value {
            e.write(value);
        }
        out.submitted = std::mem::take(&mut e.submitted);
        out.escaped = std::mem::take(&mut e.escaped);

        if visible {
            let text_h = BODY * 1.45;
            let inner = Rect::new(r.x + 10.0, r.y + (r.h - text_h) / 2.0, r.w - 20.0, text_h);
            e.place(
                (inner.x * scale).round() as i32,
                (inner.y * scale).round() as i32,
                (inner.w * scale).round() as i32,
                (inner.h * scale).round() as i32,
            );
        } else {
            e.hide();
        }
        if out.changed || out.submitted || out.escaped {
            self.repaint = true;
        }
        out
    }

    pub fn focus_input(&mut self, id: Id) {
        if let Some(e) = self.mem.edits.get(&id) {
            e.focus();
        }
    }

    /// Eight dots chasing round a circle; keeps frames coming while visible.
    pub fn spinner(&mut self, cx: f32, cy: f32, radius: f32) {
        let t = self.seconds();
        let head = (t * 10.0) as usize % 8;
        let c = self.c.accent;
        for i in 0..8 {
            let a = i as f32 / 8.0 * std::f32::consts::TAU;
            let age = (head + 8 - i) % 8;
            let alpha = 1.0 - age as f32 / 8.0;
            self.g.fill_circle(cx + a.cos() * radius, cy + a.sin() * radius, radius * 0.22, c.alpha(alpha.max(0.15)));
        }
        self.wake_after(100);
    }
}

/// Simple vertical layout cursor.
pub struct Col {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

impl Col {
    pub fn new(x: f32, y: f32, w: f32) -> Self {
        Self { x, y, w }
    }
    pub fn take(&mut self, h: f32) -> Rect {
        let r = Rect::new(self.x, self.y, self.w, h);
        self.y += h;
        r
    }
    pub fn gap(&mut self, h: f32) {
        self.y += h;
    }
}
