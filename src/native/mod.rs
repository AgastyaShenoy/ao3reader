//! Native Win32 interface: Direct2D software rendering + DirectWrite, no GPU driver.
//!
//! The window host below owns all state on the UI thread. Every input event runs an input
//! frame (no drawing), then invalidates; WM_PAINT runs the paint frame. Nothing animates or
//! polls unless a widget asks for a wake-up, so an idle window uses no CPU.

#![allow(unsafe_op_in_unsafe_fn)]

pub mod app;
mod edit;
mod gfx;
mod icons;
mod settings;
mod ui;
mod worker;

use app::{App, Deferred};
use edit::{create_edit_font, WM_APP_EDIT_KEY};
use gfx::Gfx;
use std::cell::RefCell;
use ui::{Colors, FrameOut, Input, Key, Memory, Ui};
use windows::core::{w, Result, HSTRING};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use worker::WM_APP_RESULT;

use crate::storage::config::AppConfig;

const TIMER_WAKE: usize = 1;
const DEFAULT_SIZE: (f32, f32) = (1120.0, 800.0);
const MIN_SIZE: (f32, f32) = (680.0, 520.0);
const EDIT_FONT_DIP: f32 = 13.5;

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

struct Host {
    hwnd: HWND,
    gfx: Gfx,
    input: Input,
    mem: Memory,
    app: App,
    hand_cursor: bool,
    edit_brush: HBRUSH,
    edit_brush_color: u32,
}

fn colorref(c: gfx::Color) -> COLORREF {
    let to = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    COLORREF(to(c.r) | (to(c.g) << 8) | (to(c.b) << 16))
}

fn loword(v: usize) -> u16 {
    (v & 0xFFFF) as u16
}

fn hiword(v: usize) -> u16 {
    ((v >> 16) & 0xFFFF) as u16
}

fn lparam_point(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

impl Host {
    fn scale(&self) -> f32 {
        self.gfx.dpi / 96.0
    }

    fn client_dips(&self) -> (f32, f32) {
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let s = self.scale();
        ((rc.right - rc.left) as f32 / s, (rc.bottom - rc.top) as f32 / s)
    }

    fn colors(&self) -> Colors {
        Colors::from_palette(&self.app.config.theme.palette())
    }

    fn frame(&mut self) -> FrameOut {
        let colors = self.colors();
        let size = self.client_dips();
        let out = {
            let mut ui = Ui::new(&mut self.gfx, &self.input, &mut self.mem, colors, self.hwnd, size);
            self.app.frame(&mut ui);
            ui.finish()
        };
        self.input.clear_transient();
        self.hand_cursor = out.hand_cursor;
        if let Some(ms) = out.wake_ms {
            unsafe { SetTimer(Some(self.hwnd), TIMER_WAKE, ms.max(10), None) };
        }
        out
    }

    fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    /// Runs an input frame and schedules a repaint.
    fn input_frame(&mut self) {
        self.frame();
        self.gfx.end_frame();
        self.invalidate();
    }

    fn paint(&mut self) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(self.hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(self.hwnd, &mut rc);
            if rc.right > 0 && rc.bottom > 0 && self.gfx.begin_draw(hdc, &rc).is_ok() {
                let out = self.frame();
                self.gfx.end_draw();
                self.gfx.end_frame();
                if out.repaint {
                    self.invalidate();
                }
            }
            let _ = EndPaint(self.hwnd, &ps);
        }
        self.sync_edit_colors();
    }

    /// Keeps the native edit controls' background in step with the theme.
    fn sync_edit_colors(&mut self) {
        let want = colorref(self.colors().input_bg).0;
        if want != self.edit_brush_color {
            unsafe {
                let _ = DeleteObject(self.edit_brush.into());
                self.edit_brush = CreateSolidBrush(COLORREF(want));
                for e in self.mem.edits.values() {
                    let _ = InvalidateRect(Some(e.hwnd), None, true);
                }
            }
            self.edit_brush_color = want;
        }
    }

    fn mouse_from(&mut self, lp: LPARAM) {
        let (x, y) = lparam_point(lp);
        let s = self.scale();
        self.input.mouse = (x as f32 / s, y as f32 / s);
        if !self.input.mouse_in {
            self.input.mouse_in = true;
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
            }
        }
    }

    fn handle(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        unsafe {
            match msg {
                WM_PAINT => {
                    self.paint();
                    Some(LRESULT(0))
                }
                WM_SIZE => {
                    self.invalidate();
                    Some(LRESULT(0))
                }
                WM_MOUSEMOVE => {
                    self.mouse_from(lp);
                    let out = self.frame();
                    self.gfx.end_frame();
                    if out.repaint || self.mem.active.is_some() {
                        self.invalidate();
                    }
                    Some(LRESULT(0))
                }
                WM_MOUSELEAVE => {
                    self.input.mouse_in = false;
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                    // Clicking the canvas takes focus away from any text field.
                    let _ = SetFocus(Some(self.hwnd));
                    SetCapture(self.hwnd);
                    self.mouse_from(lp);
                    self.input.down = true;
                    self.input.pressed = true;
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_LBUTTONUP => {
                    let _ = ReleaseCapture();
                    self.mouse_from(lp);
                    self.input.down = false;
                    self.input.released = true;
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_MOUSEWHEEL => {
                    let delta = hiword(wp.0) as i16 as f32;
                    let (sx, sy) = lparam_point(lp);
                    let mut pt = POINT { x: sx, y: sy };
                    let _ = ScreenToClient(self.hwnd, &mut pt);
                    self.mouse_from(LPARAM(((pt.y as u16 as isize) << 16) | (pt.x as u16 as isize)));
                    self.input.wheel += delta / WHEEL_DELTA as f32;
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_KEYDOWN => {
                    let down = |vk: VIRTUAL_KEY| GetKeyState(vk.0 as i32) < 0;
                    self.input.keys.push(Key { vk: wp.0 as u16, ctrl: down(VK_CONTROL), shift: down(VK_SHIFT), alt: down(VK_MENU) });
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_SETCURSOR if loword(lp.0 as usize) as u32 == HTCLIENT => {
                    let cursor = if self.hand_cursor { IDC_HAND } else { IDC_ARROW };
                    if let Ok(h) = LoadCursorW(None, cursor) {
                        SetCursor(Some(h));
                    }
                    Some(LRESULT(1))
                }
                WM_TIMER if wp.0 == TIMER_WAKE => {
                    let _ = KillTimer(Some(self.hwnd), TIMER_WAKE);
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_APP_RESULT => {
                    let events: Vec<_> = self.app.worker().map(|w| w.results.try_iter().collect()).unwrap_or_default();
                    for ev in events {
                        self.app.on_event(ev);
                    }
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_COMMAND if hiword(wp.0) as u32 == EN_CHANGE => {
                    let ctrl = loword(wp.0) as i32;
                    if let Some(key) = self.mem.edit_ids.get(&ctrl).copied() {
                        if let Some(e) = self.mem.edits.get_mut(&key) {
                            e.user_changed = true;
                        }
                        self.input_frame();
                    }
                    Some(LRESULT(0))
                }
                WM_APP_EDIT_KEY => {
                    let ctrl = wp.0 as i32;
                    if let Some(key) = self.mem.edit_ids.get(&ctrl).copied() {
                        if let Some(e) = self.mem.edits.get_mut(&key) {
                            if lp.0 as u16 == VK_RETURN.0 {
                                e.submitted = true;
                            } else {
                                e.escaped = true;
                            }
                        }
                        self.input_frame();
                    }
                    Some(LRESULT(0))
                }
                WM_CTLCOLOREDIT => {
                    let hdc = HDC(wp.0 as *mut _);
                    let c = self.colors();
                    SetTextColor(hdc, colorref(c.text));
                    SetBkColor(hdc, colorref(c.input_bg));
                    Some(LRESULT(self.edit_brush.0 as isize))
                }
                WM_DPICHANGED => {
                    let dpi = loword(wp.0) as f32;
                    self.gfx.set_dpi(dpi);
                    let old = self.mem.edit_font;
                    self.mem.edit_font = create_edit_font(dpi, EDIT_FONT_DIP);
                    for e in self.mem.edits.values() {
                        e.set_font(self.mem.edit_font);
                    }
                    let _ = DeleteObject(old.into());
                    let r = &*(lp.0 as *const RECT);
                    let _ = SetWindowPos(self.hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                    self.invalidate();
                    Some(LRESULT(0))
                }
                WM_CLOSE => {
                    self.app.on_close();
                    let _ = DestroyWindow(self.hwnd);
                    Some(LRESULT(0))
                }
                _ => None,
            }
        }
    }
}

/// Window-frame size for a client area of `size` DIPs at `dpi`.
fn outer_size(size: (f32, f32), dpi: u32, style: WINDOW_STYLE) -> (i32, i32) {
    let s = dpi as f32 / 96.0;
    let mut rc = RECT { left: 0, top: 0, right: (size.0 * s) as i32, bottom: (size.1 * s) as i32 };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, WINDOW_EX_STYLE::default(), dpi);
    }
    (rc.right - rc.left, rc.bottom - rc.top)
}

const STYLE: WINDOW_STYLE = WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0 | WS_CLIPCHILDREN.0);

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_GETMINMAXINFO => {
                let (w, h) = outer_size(MIN_SIZE, GetDpiForWindow(hwnd).max(96), STYLE);
                let info = &mut *(lp.0 as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT { x: w, y: h };
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }

        // Re-entrant messages (sent while a frame is running, e.g. by edit controls) find the
        // host busy and fall through to the default handling.
        let handled = HOST.with(|h| h.try_borrow_mut().ok().and_then(|mut h| h.as_mut().and_then(|host| host.handle(msg, wp, lp))));
        let result = match handled {
            Some(r) => r,
            None => DefWindowProcW(hwnd, msg, wp, lp),
        };
        run_deferred();
        result
    }
}

/// Runs dialogs and shell calls requested during a frame, outside the host borrow.
fn run_deferred() {
    loop {
        let next = HOST.with(|h| h.try_borrow_mut().ok().and_then(|mut h| h.as_mut().and_then(|host| (!host.app.deferred.is_empty()).then(|| (host.hwnd, host.app.deferred.remove(0))))));
        let Some((hwnd, job)) = next else { break };
        match job {
            Deferred::Open(target) => unsafe {
                ShellExecuteW(Some(hwnd), w!("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL);
            },
            Deferred::PickFontFile => {
                let picked = rfd::FileDialog::new()
                    .set_title("Choose a font file")
                    .add_filter("Fonts (*.ttf, *.otf)", &["ttf", "otf"])
                    .pick_file();
                if let Some(path) = picked {
                    HOST.with(|h| {
                        if let Some(host) = h.borrow_mut().as_mut() {
                            let family = host.gfx.load_font_file(&path);
                            host.app.on_font_file(path, family);
                            host.input_frame();
                        }
                    });
                }
            }
        }
    }
}

pub fn run() -> Result<()> {
    let config = AppConfig::load();
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let class = w!("AO3ReaderNative");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let (w, h) = outer_size(DEFAULT_SIZE, GetDpiForSystem(), STYLE);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("AO3 Reader"),
            STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            w,
            h,
            None,
            None,
            Some(hinst.into()),
            None,
        )?;

        // The window may have opened on a monitor whose DPI differs from the system DPI.
        let dpi = GetDpiForWindow(hwnd).max(96);
        if dpi != GetDpiForSystem() {
            let (w, h) = outer_size(DEFAULT_SIZE, dpi, STYLE);
            let _ = SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }

        let mut app = App::new(config);
        app.attach(hwnd);
        let host = Host {
            hwnd,
            gfx: Gfx::new(dpi as f32)?,
            input: Input::default(),
            mem: Memory::new(create_edit_font(dpi as f32, EDIT_FONT_DIP)),
            app,
            hand_cursor: false,
            edit_brush: CreateSolidBrush(COLORREF(0)),
            edit_brush_color: u32::MAX,
        };
        HOST.with(|h| *h.borrow_mut() = Some(host));
        HOST.with(|h| {
            if let Some(host) = h.borrow_mut().as_mut() {
                host.sync_edit_colors();
            }
        });

        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        HOST.with(|h| h.borrow_mut().take());
    }
    Ok(())
}
