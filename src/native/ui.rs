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
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LEFT, VK_RETURN, VK_RIGHT, VK_SPACE, VK_TAB};

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
    /// The press was the second of a double-click.
    pub double: bool,
    pub released: bool,
    /// Wheel movement in notches (positive = away from the user); fractional for touchpads.
    pub wheel: f32,
    pub keys: Vec<Key>,
}

impl Input {
    pub fn clear_transient(&mut self) {
        self.pressed = false;
        self.double = false;
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
    /// The accent where it has to read as text or a thin mark (softened on dark themes).
    pub accent_text: Color,
    /// The accent for logo, progress and indicators: saturated, at least 3:1 on cards.
    pub accent_mark: Color,
    pub input_bg: Color,
    pub danger: Color,
    pub danger_text: Color,
    pub warning_text: Color,
    pub success: Color,
    /// Hearts and tint on romantic relationship tags.
    pub romance: Color,
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
            on_accent: Color::rgb(p.on_accent()),
            accent_text: Color::rgb(p.accent_text()),
            accent_mark: Color::rgb(p.accent_mark()),
            input_bg: if p.is_dark { Color::rgb(p.bg) } else { Color::from_u8(255, 255, 255) },
            danger: Color::from_u8(220, 60, 60),
            danger_text: Color::rgb(p.danger_text()),
            warning_text: Color::rgb(p.warning_text()),
            success: Color::from_u8(34, 160, 84),
            romance: Color::rgb(p.romance()),
            is_dark: p.is_dark,
        }
    }
    pub fn hover(&self) -> Color {
        self.text.alpha(0.07)
    }
    pub fn pressed(&self) -> Color {
        self.text.alpha(0.12)
    }
    /// Fill for the current choice in tabs, segments and chips: neutral, so the accent is left
    /// for the one main action on screen.
    pub fn selected(&self) -> Color {
        self.text.alpha(0.12)
    }
}

#[derive(Default)]
pub struct ScrollState {
    pub offset: f32,
    pub content_h: f32,
    drag_grab: Option<f32>,
}

/// What a control is, for keyboard focus and screen readers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    Button,
    /// An on/off switch, and whether it is on.
    Toggle(bool),
    /// A tab, and whether it is the current one.
    Tab(bool),
    /// One of several choices (a segment, a theme), and whether it is chosen.
    Choice(bool),
    Slider,
    MenuItem,
    /// Text to read, like the page in the reader: Tab reaches it, nothing happens on Enter.
    Text,
    /// A text field (a native edit control, which screen readers see by itself).
    Edit,
}

/// A control as the last frame showed it: the Tab order, the focus ring and what screen
/// readers see all come from these.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: Id,
    /// Where it is, in window DIPs (whole, even if partly scrolled away).
    pub rect: Rect,
    /// The clip it was drawn in, if any.
    pub clip: Option<Rect>,
    pub role: Role,
    pub name: String,
    /// A slider's value, as shown next to it.
    pub value: String,
    /// The scroll area it sits in (id, area), so keyboard focus can scroll it into view.
    pub scroll: Option<(Id, Rect)>,
}

impl Node {
    /// The part on screen (empty when scrolled away).
    pub fn visible(&self) -> Rect {
        let r = self.clip.map_or(self.rect, |c| self.rect.intersect(&c));
        if r.w > 0.0 && r.h > 0.0 { r } else { Rect::default() }
    }
}

/// An open dropdown menu, drawn above everything at the end of the frame.
#[derive(Clone)]
pub struct MenuState {
    pub id: Id,
    pub anchor: Rect,
    pub rect: Rect,
    /// (label, danger)
    pub items: Vec<(String, bool)>,
    pub selected: Option<usize>,
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
    pub menu: Option<MenuState>,
    /// Item picked from a menu, delivered to its button on the next frame.
    pub menu_pick: Option<(Id, usize)>,
    /// The control whose tooltip is pending, and since when it has been hovered.
    pub tip: Option<(Id, Instant)>,
    /// A control clicked while hovered: its tooltip stays hidden until the mouse leaves it.
    pub tip_off: Option<Id>,
    /// Keyboard focus, and whether its ring shows (only once the keyboard has been used).
    pub focus: Option<Id>,
    pub focus_ring: bool,
    /// The controls the last frame showed, in order.
    pub nodes: Vec<Node>,
    /// A control to click on the next frame (Enter or Space, or a screen reader's "invoke").
    pub activate: Option<Id>,
    /// A slider moved with the arrow keys: (slider, steps).
    pub nudge: Option<(Id, i32)>,
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
            menu: None,
            menu_pick: None,
            tip: None,
            tip_off: None,
            focus: None,
            focus_ring: false,
            nodes: Vec::new(),
            activate: None,
            nudge: None,
        }
    }

    /// Keyboard navigation, before a frame: Tab and Shift+Tab move the focus through the
    /// controls the last frame showed (scrolling them into view), Enter or Space clicks the
    /// focused one, and Left/Right move a focused slider. Those keys are taken out of `input`
    /// so nothing else acts on them as well. A click with the mouse hides the focus ring.
    /// Returns true when the focus moved.
    pub fn keyboard(&mut self, input: &mut Input) -> bool {
        if input.pressed {
            self.focus_ring = false;
        }
        let before = self.focus;
        let mut pos = self.focus.and_then(|f| self.nodes.iter().position(|n| n.id == f));
        let mut keep = Vec::new();
        for k in std::mem::take(&mut input.keys) {
            let plain = !k.ctrl && !k.alt;
            if k.vk == VK_TAB.0 && plain && !self.nodes.is_empty() {
                let n = self.nodes.len();
                let next = match (pos, k.shift) {
                    (Some(i), false) => (i + 1) % n,
                    (Some(i), true) => (i + n - 1) % n,
                    (None, false) => 0,
                    (None, true) => n - 1,
                };
                pos = Some(next);
                let id = self.nodes[next].id;
                self.focus = Some(id);
                self.focus_ring = true;
                self.reveal(next);
                // Typing goes to a text field once Tab reaches it.
                if let Some(e) = self.edits.get(&id) {
                    e.focus();
                }
                continue;
            }
            if let (Some(i), true, true) = (pos, self.focus_ring, plain && !k.shift) {
                let role = self.nodes[i].role;
                if (k.vk == VK_RETURN.0 || k.vk == VK_SPACE.0) && !matches!(role, Role::Text | Role::Slider | Role::Edit) {
                    self.activate = Some(self.nodes[i].id);
                    continue;
                }
                if role == Role::Slider && (k.vk == VK_LEFT.0 || k.vk == VK_RIGHT.0) {
                    self.nudge = Some((self.nodes[i].id, if k.vk == VK_RIGHT.0 { 1 } else { -1 }));
                    continue;
                }
            }
            keep.push(k);
        }
        input.keys = keep;
        self.focus != before
    }

    /// Scrolls node `i` into its scroll area's view.
    fn reveal(&mut self, i: usize) {
        let n = &self.nodes[i];
        let Some((sid, area)) = n.scroll else { return };
        let r = n.rect;
        if let Some(s) = self.scroll.get_mut(&sid) {
            if r.y < area.y {
                s.offset = (s.offset - (area.y - r.y) - 16.0).max(0.0);
            } else if r.bottom() > area.bottom() {
                s.offset += r.bottom() - area.bottom() + 16.0;
            }
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

/// What a count badge counts: works in a filter (quiet) or new works (accent tint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    Count,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// The one main action on a screen: accent fill.
    Primary,
    /// A card's main action, repeated down a list: quiet neutral fill.
    Tonal,
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
    menu_seen: bool,
    repaint: bool,
    wake_ms: Option<u32>,
    hand: bool,
    /// Tooltip to draw on top of everything this frame: (control, text).
    tip_now: Option<(Rect, String)>,
    /// Controls shown this frame (become `Memory::nodes`), and the scroll areas being drawn.
    nodes: Vec<Node>,
    scroll_stack: Vec<(Id, Rect)>,
    /// Name (and value) for the next control, from `name_next`.
    next_name: Option<(String, String)>,
}

pub const RADIUS: f32 = 6.0;
pub const BODY: f32 = 14.0;
pub const SMALL: f32 = 12.0;
pub const CONTROL_H: f32 = 32.0;
/// How long the mouse rests on a control before its tooltip shows.
const TIP_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

impl<'a> Ui<'a> {
    pub fn new(g: &'a mut Gfx, input: &'a Input, mem: &'a mut Memory, c: Colors, hwnd: HWND, size: (f32, f32)) -> Self {
        for e in mem.edits.values_mut() {
            e.seen = false;
        }
        let menu_rect = mem.menu.as_ref().map(|m| m.rect);
        let mut ui = Self {
            g,
            input,
            mem,
            c,
            hwnd,
            size,
            hot_now: None,
            wheel_used: false,
            blockers: Vec::new(),
            in_overlay: false,
            dry: false,
            menu_seen: false,
            repaint: false,
            wake_ms: None,
            hand: false,
            tip_now: None,
            nodes: Vec::new(),
            scroll_stack: Vec::new(),
            next_name: None,
        };
        ui.blockers.extend(menu_rect);
        ui
    }

    /// Finishes the frame: draws an open menu, retires edit controls that were not shown,
    /// tracks hover changes.
    pub fn finish(mut self) -> FrameOut {
        self.menu_overlay();
        // The keyboard focus ring, over everything but tooltips.
        if self.mem.focus_ring {
            if let Some(n) = self.nodes.iter().find(|n| Some(n.id) == self.mem.focus).cloned() {
                let c = self.c;
                if let Some(clip) = n.clip {
                    self.g.push_clip(clip.shrink(-3.0));
                }
                self.g.stroke_round(n.rect.shrink(-3.0), RADIUS + 2.0, 2.0, c.text);
                if n.clip.is_some() {
                    self.g.pop_clip();
                }
            }
        }
        self.mem.nodes = std::mem::take(&mut self.nodes);
        // Tooltips: forget a control once the mouse leaves it; never over an open menu.
        if self.mem.tip.is_some_and(|(t, _)| self.hot_now != Some(t)) {
            self.mem.tip = None;
        }
        if self.mem.tip_off.is_some_and(|t| self.hot_now != Some(t)) {
            self.mem.tip_off = None;
        }
        if let Some((r, text)) = self.tip_now.take() {
            if self.mem.menu.is_none() {
                self.draw_tip(r, &text);
            }
        }
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

    /// Names the next control for screen readers, with a value for sliders; controls with a
    /// label of their own use it when not named.
    pub fn name_next(&mut self, name: &str, value: &str) {
        self.next_name = Some((name.to_string(), value.to_string()));
    }

    /// A clickable control, known to screen readers as a button.
    pub fn interact(&mut self, id: Id, r: Rect) -> Resp {
        self.interact_as(id, r, Role::Button, "")
    }

    /// A clickable control known to screen readers as `role`, called `name` (unless named
    /// with `name_next`). It is also a stop for Tab, and Enter or Space clicks it.
    pub fn interact_as(&mut self, id: Id, r: Rect, role: Role, name: &str) -> Resp {
        self.register(id, r, role, name);
        self.pointer(id, r)
    }

    /// Where the next control will go in the Tab order (see `tab_first`).
    pub fn node_mark(&self) -> usize {
        self.nodes.len()
    }

    /// Moves the controls shown since `mark` to the front of the Tab order: for bars drawn
    /// after the content they sit above.
    pub fn tab_first(&mut self, mark: usize) {
        let bar = self.nodes.split_off(mark.min(self.nodes.len()));
        self.nodes.splice(0..0, bar);
    }

    /// Something to read rather than click (the reader's page): a stop for Tab, and what a
    /// screen reader reads there.
    pub fn text_node(&mut self, id: Id, r: Rect, text: &str) {
        self.register(id, r, Role::Text, text);
    }

    fn register(&mut self, id: Id, r: Rect, role: Role, name: &str) {
        let (name, value) = match self.next_name.take() {
            Some((n, v)) => (n, v),
            None => (name.to_string(), String::new()),
        };
        // Not while measuring, and not under a panel or menu drawn on top.
        let centre = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        if self.dry || (!self.in_overlay && self.blockers.iter().any(|b| b.contains(centre))) {
            return;
        }
        let clip = self.g.clip();
        let scroll = self.scroll_stack.last().copied();
        // Clipped away outside any scroll area (so focus couldn't bring it into view).
        if scroll.is_none() && clip.is_some_and(|c| c.intersect(&r).w <= 0.0) {
            return;
        }
        self.nodes.push(Node { id, rect: r, clip, role, name, value, scroll });
    }

    /// Mouse handling for a control (and keyboard or screen-reader clicks).
    fn pointer(&mut self, id: Id, r: Rect) -> Resp {
        let activated = !self.dry && self.mem.activate == Some(id);
        if activated {
            self.mem.activate = None;
        }
        let hovered = self.hit(r) && self.mem.active.is_none_or(|a| a == id);
        if hovered {
            self.hot_now = Some(id);
            self.hand = true;
        }
        if hovered && self.input.pressed && self.mem.active.is_none() {
            self.mem.active = Some(id);
            // Tab carries on from what was clicked.
            self.mem.focus = Some(id);
        }
        let held = self.mem.active == Some(id);
        let clicked = (held && self.input.released && hovered) || activated;
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

    /// Frame for a problem the user can't fix in place (AO3 down, a failed download): a plain
    /// card with an amber warning sign, noticeable without turning the screen red. The message
    /// goes at x + 40.
    pub fn notice(&mut self, r: Rect, title: &str) {
        let c = self.c;
        self.card(r);
        let ic = self.measure("\u{E7BA}", &TextStyle::icons(15.0), TextOpts::default());
        self.g.draw_text(ic, r.x + 16.0, r.y + 15.0, c.warning_text);
        self.text(title, &TextStyle::ui(13.5).semibold(), r.x + 40.0, r.y + 14.0, c.text);
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
        let resp = if max > 0.0 { self.pointer(bar_id, bar) } else { Resp::default() };
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
        self.scroll_stack.push((id, r));
        let h = content(self, r, r.y - offset);
        self.scroll_stack.pop();
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

    /// Button that opens a dropdown menu. Returns the item picked (one frame later).
    pub fn menu_button(&mut self, id: Id, r: Rect, label: &str, icon: Option<&str>, items: &[(&str, bool)], selected: Option<usize>) -> Option<usize> {
        let picked = match self.mem.menu_pick {
            Some((mid, i)) if mid == id => {
                self.mem.menu_pick = None;
                Some(i)
            }
            _ => None,
        };
        let open = self.mem.menu.as_ref().is_some_and(|m| m.id == id);
        if open {
            self.menu_seen = true;
            if let Some(m) = &mut self.mem.menu {
                m.anchor = r;
            }
        }
        if self.button_ex(id, r, label, icon, ButtonKind::Secondary, true) {
            if open {
                self.mem.menu = None;
            } else {
                let w = items
                    .iter()
                    // Room for the label plus the row's padding and check mark (see below).
                    .map(|(l, _)| self.g.layout(l, &TextStyle::ui(13.5), TextOpts::default()).w + 52.0)
                    .fold(r.w.max(180.0), f32::max);
                let h = items.len() as f32 * 34.0 + 8.0;
                let x = (r.right() - w).max(4.0).min(self.size.0 - w - 4.0);
                let y = if r.bottom() + 4.0 + h > self.size.1 - 4.0 { (r.y - 4.0 - h).max(4.0) } else { r.bottom() + 4.0 };
                self.mem.menu = Some(MenuState {
                    id,
                    anchor: r,
                    rect: Rect::new(x, y, w, h),
                    items: items.iter().map(|(l, d)| (l.to_string(), *d)).collect(),
                    selected,
                });
                self.menu_seen = true;
            }
            self.repaint = true;
        }
        picked
    }

    fn menu_overlay(&mut self) {
        let Some(m) = self.mem.menu.clone() else { return };
        if !self.menu_seen {
            // The button that owns the menu is gone (screen changed).
            self.mem.menu = None;
            return;
        }
        let c = self.c;
        self.in_overlay = true;
        self.g.fill_round(Rect::new(m.rect.x + 1.0, m.rect.y + 3.0, m.rect.w, m.rect.h), 8.0, Color::from_u8(0, 0, 0).alpha(0.18));
        self.g.fill_round(m.rect, 8.0, c.surface);
        self.g.stroke_round(m.rect, 8.0, 1.0, c.border);
        for (i, (label, danger)) in m.items.iter().enumerate() {
            let row = Rect::new(m.rect.x + 4.0, m.rect.y + 4.0 + i as f32 * 34.0, m.rect.w - 8.0, 34.0);
            let resp = self.interact_as(id2(m.id, ("menu-item", i)), row, Role::MenuItem, label);
            if resp.hovered {
                self.g.fill_round(row, 5.0, c.hover());
            }
            let color = if *danger { c.danger } else { c.text };
            self.text_in(label, &TextStyle::ui(13.5), Rect::new(row.x + 12.0, row.y, row.w - 40.0, row.h), Align::Left, color);
            if m.selected == Some(i) {
                self.text_in("\u{E73E}", &TextStyle::icons(12.0), Rect::new(row.right() - 28.0, row.y, 16.0, row.h), Align::Center, c.accent_text);
            }
            if resp.clicked {
                self.mem.menu_pick = Some((m.id, i));
                self.mem.menu = None;
                self.repaint = true;
            }
        }
        if self.input.pressed && !m.rect.contains(self.input.mouse) && !m.anchor.contains(self.input.mouse) {
            self.mem.menu = None;
            self.repaint = true;
        }
        self.in_overlay = false;
    }

    /// Square icon-only ghost button; `tip` names it when the mouse rests on it.
    pub fn icon_button(&mut self, id: Id, r: Rect, icon: &str, tip: &str, active: bool) -> bool {
        if active {
            let c = self.c.selected();
            self.g.fill_round(r, RADIUS, c);
        }
        let clicked = self.button_ex(id, r, "", Some(icon), ButtonKind::Ghost, true);
        self.tip(id, r, tip);
        clicked
    }

    /// Shows `text` near `r` once the mouse has rested on the control `id` for a moment.
    /// Call it right after the control itself.
    pub fn tip(&mut self, id: Id, r: Rect, text: &str) {
        // The tip names an icon-only control for screen readers too.
        if let Some(n) = self.nodes.iter_mut().rev().find(|n| n.id == id) {
            if n.name.is_empty() {
                n.name = text.to_string();
            }
        }
        if self.hot_now != Some(id) || self.dry || text.is_empty() {
            return;
        }
        if self.input.pressed || self.input.down {
            self.mem.tip_off = Some(id);
        }
        if self.mem.tip_off == Some(id) {
            return;
        }
        let since = match self.mem.tip {
            Some((t, s)) if t == id => s,
            _ => {
                let now = Instant::now();
                self.mem.tip = Some((id, now));
                now
            }
        };
        let waited = since.elapsed();
        if waited >= TIP_DELAY {
            self.tip_now = Some((r, text.to_string()));
        } else {
            self.wake_after((TIP_DELAY - waited).as_millis() as u32 + 10);
        }
    }

    /// A small label under (or, near the bottom edge, above) the control.
    fn draw_tip(&mut self, anchor: Rect, text: &str) {
        let c = self.c;
        let t = self.measure(text, &TextStyle::ui(12.5), TextOpts::wrap(300.0));
        let (w, h) = (t.w + 20.0, t.h + 12.0);
        let x = (anchor.x + (anchor.w - w) / 2.0).clamp(4.0, (self.size.0 - w - 4.0).max(4.0));
        let below = anchor.bottom() + 6.0;
        let y = if below + h > self.size.1 - 4.0 { anchor.y - 6.0 - h } else { below };
        let r = Rect::new(x, y, w, h);
        self.g.fill_round(Rect::new(r.x, r.y + 2.0, r.w, r.h), 6.0, Color::from_u8(0, 0, 0).alpha(0.18));
        self.g.fill_round(r, 6.0, c.surface);
        self.g.stroke_round(r, 6.0, 1.0, c.border);
        self.g.draw_text(t, r.x + 10.0, r.y + 6.0, c.text);
    }

    pub fn button(&mut self, id: Id, r: Rect, label: &str) -> bool {
        self.button_ex(id, r, label, None, ButtonKind::Secondary, true)
    }

    pub fn button_ex(&mut self, id: Id, r: Rect, label: &str, icon: Option<&str>, kind: ButtonKind, enabled: bool) -> bool {
        let resp = if enabled {
            self.interact_as(id, r, Role::Button, label)
        } else {
            self.next_name = None;
            Resp::default()
        };
        let c = self.c;
        let (fill, fg, border) = match kind {
            ButtonKind::Primary => (c.accent, c.on_accent, None),
            ButtonKind::Danger => (c.danger, c.on_accent, None),
            ButtonKind::Tonal => (c.text.alpha(0.09), c.text, None),
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
            let style = if matches!(kind, ButtonKind::Primary | ButtonKind::Tonal) { TextStyle::ui(BODY).semibold() } else { TextStyle::ui(BODY) };
            let t = self.g.layout(label, &style, TextOpts::ellipsize((r.right() - x - 8.0).max(1.0)));
            self.g.draw_text(t, x, r.y + (r.h - t.h) / 2.0, fg);
        }
        resp.clicked
    }

    /// Windows 11 style on/off switch (40×20) at (x, y).
    pub fn toggle(&mut self, id: Id, x: f32, y: f32, value: &mut bool) -> bool {
        let r = Rect::new(x, y, 40.0, 20.0);
        let resp = self.interact_as(id, r.shrink(-4.0), Role::Toggle(*value), "");
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
        if self.next_name.as_ref().is_some_and(|(_, v)| v.is_empty()) || self.next_name.is_none() {
            let name = self.next_name.take().map(|(n, _)| n).unwrap_or_default();
            self.next_name = Some((name, format!("{value:.2}")));
        }
        let resp = self.interact_as(id, r, Role::Slider, "");
        let mut changed = false;
        // Left and Right on a focused slider move it a step.
        if let Some((nid, steps)) = self.mem.nudge {
            if nid == id && !self.dry {
                self.mem.nudge = None;
                let st = if step > 0.0 { step } else { (max - min) / 20.0 };
                let v = (*value + st * steps as f32).clamp(min, max);
                if (v - *value).abs() > f32::EPSILON {
                    *value = v;
                    changed = true;
                    self.repaint = true;
                }
            }
        }
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
        self.g.fill_round(Rect::new(x0, cy - 2.0, xv - x0, 4.0), 2.0, c.accent_mark);
        self.g.fill_circle(xv, cy, 9.0, c.card);
        self.g.fill_circle(xv, cy, if resp.held || resp.hovered { 6.0 } else { 5.0 }, c.accent_mark);
        changed
    }

    /// Row of mutually exclusive options.
    pub fn segmented(&mut self, id: Id, r: Rect, options: &[&str], selected: &mut usize) -> bool {
        let items: Vec<(&str, usize)> = options.iter().map(|o| (*o, 0)).collect();
        self.segmented_counts(id, r, &items, selected, Badge::Count)
    }

    /// Small rounded number after a label: how many works a filter holds (`Badge::Count`,
    /// quiet) or how many are new (`Badge::New`, accent tint). Nothing is drawn for 0.
    /// `cy` is the vertical centre; returns the width used.
    pub fn badge(&mut self, x: f32, cy: f32, n: usize, kind: Badge, strong: bool) -> f32 {
        if n == 0 {
            return 0.0;
        }
        let c = self.c;
        let label = if n > 999 { "999+".to_string() } else { n.to_string() };
        let t = self.measure(&label, &TextStyle::ui(11.5).semibold(), TextOpts::default());
        let h = 18.0;
        let r = Rect::new(x, cy - h / 2.0, (t.w + 12.0).max(h), h);
        let (fill, fg) = match kind {
            Badge::Count => (c.text.alpha(if strong { 0.16 } else { 0.09 }), if strong { c.text } else { c.muted }),
            Badge::New => (c.accent_text.alpha(0.18), c.accent_text),
        };
        self.g.fill_round(r, h / 2.0, fill);
        self.g.draw_text(t, r.x + (r.w - t.w) / 2.0, r.y + (h - t.h) / 2.0, fg);
        r.w
    }

    /// Width `badge` will take (0 for 0).
    pub fn badge_width(&mut self, n: usize) -> f32 {
        if n == 0 {
            return 0.0;
        }
        let label = if n > 999 { "999+".to_string() } else { n.to_string() };
        (self.measure(&label, &TextStyle::ui(11.5).semibold(), TextOpts::default()).w + 12.0).max(18.0)
    }

    fn chip_style(selected: bool) -> TextStyle {
        if selected { TextStyle::ui(13.0).semibold() } else { TextStyle::ui(13.0) }
    }

    /// Width of a `chip`, for wrapping rows of them.
    pub fn chip_width(&mut self, label: &str, n: usize, selected: bool) -> f32 {
        let t = self.measure(label, &Self::chip_style(selected), TextOpts::default());
        t.w + 26.0 + if n > 0 { self.badge_width(n) + 6.0 } else { 0.0 }
    }

    /// A rounded, clickable chip 30 high, with a badge for `n` (none for 0). Returns its width
    /// and whether it was clicked.
    pub fn chip(&mut self, id: Id, x: f32, y: f32, label: &str, n: usize, kind: Badge, selected: bool) -> (f32, bool) {
        let c = self.c;
        let t = self.measure(label, &Self::chip_style(selected), TextOpts::default());
        let r = Rect::new(x, y, self.chip_width(label, n, selected), 30.0);
        let resp = self.interact_as(id, r, Role::Choice(selected), label);
        if selected {
            self.g.fill_round(r, 15.0, c.selected());
        } else {
            self.g.stroke_round(r, 15.0, 1.0, c.border);
            if resp.hovered {
                self.g.fill_round(r, 15.0, c.hover());
            }
        }
        self.g.draw_text(t, r.x + 13.0, r.y + (30.0 - t.h) / 2.0, c.text);
        self.badge(r.x + 19.0 + t.w, r.y + 15.0, n, kind, selected);
        (r.w, resp.clicked)
    }

    /// Segmented control whose options carry a count badge.
    pub fn segmented_counts(&mut self, id: Id, r: Rect, options: &[(&str, usize)], selected: &mut usize, kind: Badge) -> bool {
        let c = self.c;
        self.g.fill_round(r, RADIUS, c.bg);
        self.g.stroke_round(r, RADIUS, 1.0, c.border);
        let n = options.len().max(1) as f32;
        let seg_w = r.w / n;
        let mut changed = false;
        for (i, (label, count)) in options.iter().enumerate() {
            let sr = Rect::new(r.x + seg_w * i as f32, r.y, seg_w, r.h).shrink(3.0);
            let resp = self.interact_as(id2(id, i), sr, Role::Choice(*selected == i), label);
            if resp.clicked && *selected != i {
                *selected = i;
                changed = true;
            }
            let is_sel = *selected == i;
            if is_sel {
                self.g.fill_round(sr, RADIUS - 2.0, c.selected());
            } else if resp.hovered {
                self.g.fill_round(sr, RADIUS - 2.0, c.hover());
            }
            let style = if is_sel { TextStyle::ui(13.0).semibold() } else { TextStyle::ui(13.0) };
            let fg = if is_sel { c.text } else { c.muted };
            if *count == 0 {
                self.text_in(label, &style, sr.shrink2(6.0, 0.0), Align::Center, fg);
                continue;
            }
            let bw = self.badge_width(*count);
            let t = self.measure(label, &style, TextOpts::ellipsize((sr.w - bw - 18.0).max(1.0)));
            let x = sr.x + ((sr.w - t.w - 6.0 - bw) / 2.0).max(6.0);
            self.g.draw_text(t, x, sr.y + (sr.h - t.h) / 2.0, fg);
            self.badge(x + t.w + 6.0, sr.y + sr.h / 2.0, *count, kind, is_sel);
        }
        changed
    }

    /// Pill-shaped tab strip with icons; wraps onto more rows when narrow. Returns its height.
    /// `badges` holds a "new" count per tab (or is empty).
    pub fn tab_strip(&mut self, id: Id, x: f32, y: f32, w: f32, items: &[(&str, &str)], badges: &[usize], selected: &mut usize) -> (f32, bool) {
        let c = self.c;
        let mut cx = x;
        let mut cy = y;
        let h = 34.0;
        let mut changed = false;
        for (i, (icon, label)) in items.iter().enumerate() {
            let n = badges.get(i).copied().unwrap_or(0);
            let bw = self.button_width(label, Some(icon)) + if n > 0 { self.badge_width(n) + if label.is_empty() { 2.0 } else { 6.0 } } else { 0.0 };
            if cx > x && cx + bw > x + w {
                cx = x;
                cy += h + 6.0;
            }
            let r = Rect::new(cx, cy, bw, h);
            let resp = self.interact_as(id2(id, i), r, Role::Tab(*selected == i), label);
            if resp.clicked && *selected != i {
                *selected = i;
                changed = true;
            }
            let is_sel = *selected == i;
            if is_sel {
                self.g.fill_round(r, h / 2.0, c.selected());
                // A short accent mark under the current tab; the fill stays neutral.
                let cx = r.x + r.w / 2.0;
                self.g.fill_round(Rect::new(cx - 9.0, r.bottom() - 3.0, 18.0, 3.0), 1.5, c.accent_mark);
            } else if resp.hovered {
                self.g.fill_round(r, h / 2.0, c.hover());
            }
            let fg = if is_sel { c.text } else { c.muted };
            let it = self.measure(icon, &TextStyle::icons(14.0), TextOpts::default());
            self.g.draw_text(it, r.x + 14.0, r.y + (h - it.h) / 2.0, fg);
            let style = if is_sel { TextStyle::ui(13.5).semibold() } else { TextStyle::ui(13.5) };
            let t = self.measure(label, &style, TextOpts::default());
            self.g.draw_text(t, r.x + 38.0, r.y + (h - t.h) / 2.0, fg);
            if n > 0 {
                let bx = if label.is_empty() { r.x + 36.0 } else { r.x + 44.0 + t.w };
                self.badge(bx, r.y + h / 2.0, n, Badge::New, is_sel);
            }
            cx += bw + 6.0;
        }
        (cy + h - y, changed)
    }

    /// Single-line text input backed by a native EDIT control.
    pub fn text_input(&mut self, id: Id, r: Rect, value: &mut String, placeholder: &str, password: bool) -> InputResp {
        let c = self.c;
        let focused = self.mem.edits.get(&id).is_some_and(|e| unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() } == e.hwnd);
        self.register(id, r, Role::Edit, placeholder);
        if focused {
            self.mem.focus = Some(id);
        }
        self.g.fill_round(r, RADIUS, c.input_bg);
        self.g.stroke_round(r, RADIUS, if focused { 2.0 } else { 1.0 }, if focused { c.accent_mark } else { c.border });

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

    /// Focuses a text box with all its text selected, so typing replaces it.
    pub fn focus_input_selected(&mut self, id: Id) {
        if let Some(e) = self.mem.edits.get(&id) {
            e.focus();
            e.select_all();
        }
    }

    /// Eight dots chasing round a circle; keeps frames coming while visible.
    pub fn spinner(&mut self, cx: f32, cy: f32, radius: f32) {
        let t = self.seconds();
        let head = (t * 10.0) as usize % 8;
        let c = self.c.accent_mark;
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
