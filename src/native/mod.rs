//! Native Win32 interface: Direct2D software rendering + DirectWrite, no GPU driver.
//!
//! The window host below owns all state on the UI thread. Every input event runs an input
//! frame (no drawing), then invalidates; WM_PAINT runs the paint frame. Nothing animates or
//! polls unless a widget asks for a wake-up, so an idle window uses no CPU.

#![allow(unsafe_op_in_unsafe_fn)]

mod account;
mod book;
mod catalog;
mod browse;
pub mod app;
mod edit;
mod fonts;
mod gfx;
mod icons;
mod library;
mod mark;
mod pdf;
mod reader;
mod settings;
mod speech;
mod uia;
mod ui;
mod worker;

use app::{App, Deferred};
use edit::{create_edit_font, WM_APP_EDIT_KEY};
use gfx::Gfx;
use std::cell::RefCell;
use ui::{Colors, FrameOut, Input, Key, Memory, Ui};
use windows::core::{w, Result, HSTRING, PCWSTR};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
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
    /// What screen readers see; kept apart from the host so they can ask at any time.
    static A11Y: RefCell<Option<uia::Shared>> = const { RefCell::new(None) };
}

struct Host {
    hwnd: HWND,
    /// The monitor's DPI; the renderer runs at this times the interface size.
    win_dpi: f32,
    gfx: Gfx,
    input: Input,
    mem: Memory,
    app: App,
    hand_cursor: bool,
    edit_brush: HBRUSH,
    edit_brush_color: u32,
    /// The colours last given to the window's title bar.
    title_bar: Option<(u32, u32, u32)>,
    a11y: uia::Shared,
    /// What the focused control said last (the page in the reader changes under the focus).
    focus_said: String,
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
        Colors::from_palette(&self.app.config.palette())
    }

    /// Gives the title bar the theme's colours: the header's background, the text colour and the
    /// border (Windows 11), and dark window buttons on dark themes (Windows 10 too).
    fn theme_title_bar(&mut self, c: Colors) {
        use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE};
        let want = (colorref(c.surface).0, colorref(c.text).0, colorref(c.border).0);
        if self.title_bar == Some(want) {
            return;
        }
        self.title_bar = Some(want);
        let dark: i32 = self.app.config.palette().is_dark.into();
        unsafe {
            let set = |attr, v: &u32| DwmSetWindowAttribute(self.hwnd, attr, v as *const u32 as *const core::ffi::c_void, 4);
            // Older Windows doesn't know some of these; it just keeps its own colours.
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const i32 as *const core::ffi::c_void, 4);
            let _ = set(DWMWA_CAPTION_COLOR, &want.0);
            let _ = set(DWMWA_TEXT_COLOR, &want.1);
            let _ = set(DWMWA_BORDER_COLOR, &want.2);
        }
    }

    /// Reads the system's dark mode and high-contrast setting and passes them to the app.
    fn read_system_look(&mut self) {
        unsafe {
            use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
            use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
            let mut light: u32 = 1;
            let mut size = 4u32;
            let ok = RegGetValueW(
                HKEY_CURRENT_USER,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut light as *mut u32 as *mut core::ffi::c_void),
                Some(&mut size),
            )
            .is_ok();
            let mut hc = HIGHCONTRASTW { cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32, ..Default::default() };
            let hc_on = SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize, Some(&mut hc as *mut HIGHCONTRASTW as *mut core::ffi::c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok()
                && hc.dwFlags.contains(HCF_HIGHCONTRASTON);
            self.app.on_system_look(ok && light == 0, hc_on);
        }
    }

    fn frame(&mut self) -> FrameOut {
        if self.gfx.set_ui_family(self.app.config.typography.ui_font.as_deref()) {
            self.set_edit_font();
        }
        // The interface size the user chose (1.0 unless they changed it).
        let want = self.win_dpi * self.app.config.ui_scale.clamp(0.75, 2.0);
        if (self.gfx.dpi - want).abs() > 0.01 {
            self.gfx.set_dpi(want);
            self.set_edit_font();
        }
        let colors = self.colors();
        self.theme_title_bar(colors);
        let size = self.client_dips();
        let moved = self.mem.keyboard(&mut self.input);
        if moved && self.mem.focus.is_none_or(|f| !self.mem.edits.contains_key(&f)) && unsafe { GetFocus() } != self.hwnd {
            // Tab left a text field.
            unsafe {
                let _ = SetFocus(Some(self.hwnd));
            }
        }
        let out = {
            let mut ui = Ui::new(&mut self.gfx, &self.input, &mut self.mem, colors, self.hwnd, size);
            self.app.frame(&mut ui);
            ui.finish()
        };
        self.input.clear_transient();
        self.update_a11y(moved);
        self.hand_cursor = out.hand_cursor;
        if let Some(ms) = out.wake_ms {
            unsafe { SetTimer(Some(self.hwnd), TIMER_WAKE, ms.max(10), None) };
        }
        out
    }

    /// Hands this frame's controls to screen readers, and tells them when the keyboard focus
    /// moved (or the focused page changed under it), so they read it out.
    fn update_a11y(&mut self, moved: bool) {
        self.a11y.update(&self.mem.nodes, self.mem.focus.filter(|_| self.mem.focus_ring), self.scale());
        let Some(f) = self.mem.focus.filter(|_| self.mem.focus_ring) else { return };
        let Some(node) = self.mem.nodes.iter().find(|n| n.id == f) else { return };
        if moved || node.name != self.focus_said {
            self.focus_said = node.name.clone();
            self.a11y.focus_changed(f);
        }
    }

    /// Rebuilds the text boxes' font for the current DPI and interface face.
    fn set_edit_font(&mut self) {
        let old = self.mem.edit_font;
        self.mem.edit_font = create_edit_font(self.gfx.dpi, EDIT_FONT_DIP, self.gfx.ui_family());
        for e in self.mem.edits.values() {
            e.set_font(self.mem.edit_font);
        }
        unsafe {
            let _ = DeleteObject(old.into());
        }
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
                    self.input.double = msg == WM_LBUTTONDBLCLK;
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
                    let events = self.app.take_events();
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
                uia::WM_APP_A11Y => {
                    // A screen reader pressed or focused a control.
                    let id = lp.0 as u64;
                    if wp.0 == uia::ACT_CLICK {
                        self.mem.activate = Some(id);
                    }
                    self.mem.focus = Some(id);
                    self.mem.focus_ring = true;
                    self.input_frame();
                    Some(LRESULT(0))
                }
                WM_SETTINGCHANGE => {
                    // Light/dark mode or high contrast may have changed.
                    self.read_system_look();
                    self.invalidate();
                    None
                }
                WM_DPICHANGED => {
                    let dpi = loword(wp.0) as f32;
                    self.win_dpi = dpi;
                    self.gfx.set_dpi(dpi * self.app.config.ui_scale.clamp(0.75, 2.0));
                    self.set_edit_font();
                    let r = &*(lp.0 as *const RECT);
                    let _ = SetWindowPos(self.hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                    self.invalidate();
                    Some(LRESULT(0))
                }
                WM_CLOSE => {
                    // Where the window was, to open there next time.
                    let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
                    if GetWindowPlacement(self.hwnd, &mut wp).is_ok() {
                        let r = wp.rcNormalPosition;
                        let place = Some(([r.left, r.top, r.right, r.bottom], wp.showCmd == SW_SHOWMAXIMIZED.0 as u32));
                        if self.app.config.window != place {
                            self.app.config.window = place;
                            self.app.mark_config_dirty();
                        }
                    }
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
            // Screen readers asking for the window's controls.
            WM_GETOBJECT if lp.0 as i32 == windows::Win32::UI::Accessibility::UiaRootObjectId => {
                if let Some(root) = A11Y.with(|a| a.borrow().as_ref().map(|s| s.root())) {
                    return windows::Win32::UI::Accessibility::UiaReturnRawElementProvider(hwnd, wp, lp, &root);
                }
            }
            // Another start of the app ("Open with" while it's running) handing over a book.
            WM_COPYDATA => {
                let cds = &*(lp.0 as *const COPYDATASTRUCT);
                if cds.dwData == OPEN_FILE_DATA && !cds.lpData.is_null() {
                    let units = std::slice::from_raw_parts(cds.lpData as *const u16, cds.cbData as usize / 2);
                    let path = std::path::PathBuf::from(String::from_utf16_lossy(units));
                    HOST.with(|h| {
                        if let Ok(mut h) = h.try_borrow_mut() {
                            if let Some(host) = h.as_mut() {
                                host.app.open_file(path);
                                host.invalidate();
                            }
                        }
                    });
                }
                return LRESULT(1);
            }
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

/// Applies a dialog result to the app and runs a frame so it shows.
fn with_host(f: impl FnOnce(&mut Host)) {
    HOST.with(|h| {
        if let Some(host) = h.borrow_mut().as_mut() {
            f(host);
            host.input_frame();
        }
    });
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
            Deferred::SetTitle(title) => unsafe {
                let _ = SetWindowTextW(hwnd, &HSTRING::from(title));
            },
            Deferred::Fullscreen(on) => set_fullscreen(hwnd, on),
            Deferred::Copy(text) => unsafe {
                use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
                use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
                const CF_UNICODETEXT: u32 = 13;
                let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
                if OpenClipboard(Some(hwnd)).is_ok() {
                    let _ = EmptyClipboard();
                    if let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) {
                        let p = GlobalLock(mem) as *mut u16;
                        if !p.is_null() {
                            std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                            let _ = GlobalUnlock(mem);
                            let _ = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0)));
                        }
                    }
                    let _ = CloseClipboard();
                }
            },
            Deferred::PickColor { text, current } => unsafe {
                use windows::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
                // COLORREF is 0x00BBGGRR.
                let swap = |c: u32| ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF);
                let mut custom = [COLORREF(0x00FF_FFFF); 16];
                let mut cc = CHOOSECOLORW {
                    lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
                    hwndOwner: hwnd,
                    rgbResult: COLORREF(swap(current)),
                    lpCustColors: custom.as_mut_ptr(),
                    Flags: CC_FULLOPEN | CC_RGBINIT,
                    ..Default::default()
                };
                if ChooseColorW(&mut cc).as_bool() {
                    let rgb = swap(cc.rgbResult.0);
                    with_host(|host| host.app.on_color(text, rgb));
                }
            },
            Deferred::ImportFile => {
                let picked = rfd::FileDialog::new()
                    .set_title("Import a work")
                    .add_filter("EPUB, PDF, AO3 download or saved work (*.epub, *.pdf, *.html, *.json)", &["epub", "pdf", "html", "htm", "json"])
                    .pick_file();
                if let Some(path) = picked {
                    with_host(|host| host.app.import_file(path));
                }
            }
            Deferred::SaveFile { name, filter, ext, data } => {
                let picked = rfd::FileDialog::new().set_file_name(&name).add_filter(filter, &[ext]).save_file();
                if let Some(path) = picked {
                    let result = std::fs::write(&path, &data).map(|_| path).map_err(|e| e.to_string());
                    with_host(|host| host.app.on_saved(result));
                }
            }
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

thread_local! {
    /// Window placement saved while the reader is full screen.
    static SAVED_PLACEMENT: std::cell::Cell<Option<WINDOWPLACEMENT>> = const { std::cell::Cell::new(None) };
}

/// Borderless full screen on the window's current monitor, restoring the old placement after.
fn set_fullscreen(hwnd: HWND, on: bool) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        if on {
            let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if GetWindowPlacement(hwnd, &mut wp).is_ok() && GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
                SAVED_PLACEMENT.with(|p| p.set(Some(wp)));
                SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(WS_OVERLAPPEDWINDOW.0 as isize));
                let m = mi.rcMonitor;
                let _ = SetWindowPos(hwnd, Some(HWND_TOP), m.left, m.top, m.right - m.left, m.bottom - m.top, SWP_NOOWNERZORDER | SWP_FRAMECHANGED);
            }
        } else if let Some(wp) = SAVED_PLACEMENT.with(|p| p.take()) {
            SetWindowLongPtrW(hwnd, GWL_STYLE, style | WS_OVERLAPPEDWINDOW.0 as isize);
            let _ = SetWindowPlacement(hwnd, &wp);
            let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_FRAMECHANGED);
        }
    }
}

/// Title bar and taskbar icons: the flax flower in white on a flax-blue tile, drawn at the sizes
/// Windows asks for at this DPI (the exe itself carries no icon resource).
unsafe fn set_window_icons(hwnd: HWND, gfx: &mut Gfx, dpi: u32) {
    let tile = gfx::Color::from_u8(79, 111, 184);
    let flower = gfx::Color::from_u8(255, 255, 255);
    for (which, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
        let px = GetSystemMetricsForDpi(metric, dpi).max(16);
        if let Some(icon) = gfx.icon(mark::PATH, mark::SIZE, px, tile, flower) {
            SendMessageW(hwnd, WM_SETICON, Some(WPARAM(which as usize)), Some(LPARAM(icon.0 as isize)));
        }
    }
}

/// Marks a WM_COPYDATA that carries a book's path to open.
const OPEN_FILE_DATA: usize = 0x464C_4158; // "FLAX"

/// One copy of the app per library: a second start (opening a book with the app while it runs)
/// hands the book to the first and leaves, so two copies never save over each other's changes.
/// Returns true when another copy took over.
unsafe fn hand_to_running_copy(class: PCWSTR) -> bool {
    let key = library_key();
    let name = HSTRING::from(format!("Local\\FlaxReader.{key:08x}"));
    // Kept for the life of the process: the name is taken while it runs.
    let mutex = windows::Win32::System::Threading::CreateMutexW(None, false, &name);
    let taken = windows::Win32::Foundation::GetLastError() == windows::Win32::Foundation::ERROR_ALREADY_EXISTS;
    std::mem::forget(mutex);
    if !taken {
        return false;
    }
    // The running copy's window: the one marked with this library's key.
    let mut found = HWND::default();
    let mut w = FindWindowExW(None, None, class, PCWSTR::null()).unwrap_or_default();
    while !w.is_invalid() {
        if GetPropW(w, w!("FlaxReaderLibrary")).0 as usize as u32 == key {
            found = w;
            break;
        }
        w = FindWindowExW(None, Some(w), class, PCWSTR::null()).unwrap_or_default();
    }
    if found.is_invalid() {
        // Starting up or closing: carry on as a copy of its own.
        return false;
    }
    if let Some(path) = std::env::args_os().nth(1).filter(|p| std::path::Path::new(p).is_file()) {
        let units: Vec<u16> = path.to_string_lossy().encode_utf16().collect();
        let cds = COPYDATASTRUCT { dwData: OPEN_FILE_DATA, cbData: (units.len() * 2) as u32, lpData: units.as_ptr() as *mut _ };
        SendMessageW(found, WM_COPYDATA, None, Some(LPARAM(&cds as *const _ as isize)));
    }
    if IsIconic(found).as_bool() {
        let _ = ShowWindow(found, SW_RESTORE);
    }
    let _ = SetForegroundWindow(found);
    true
}

/// The library's key, put on the window so a second start can find this copy.
fn library_key() -> u32 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    AppConfig::get_app_dir().to_string_lossy().to_lowercase().hash(&mut h);
    h.finish() as u32
}

pub fn run() -> Result<()> {
    let config = AppConfig::load();
    unsafe {
        if hand_to_running_copy(w!("AO3ReaderNative")) {
            return Ok(());
        }
        // COM, for pictures (WIC), the reading voice and screen readers' names for text fields.
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
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
            w!("Flax Reader"),
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

        let _ = SetPropW(hwnd, w!("FlaxReaderLibrary"), Some(windows::Win32::Foundation::HANDLE(library_key() as usize as *mut _)));
        // The window may have opened on a monitor whose DPI differs from the system DPI.
        let dpi = GetDpiForWindow(hwnd).max(96);
        if dpi != GetDpiForSystem() {
            let (w, h) = outer_size(DEFAULT_SIZE, dpi, STYLE);
            let _ = SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        // Back where it was last time (Windows keeps it on a screen that's still there).
        let mut maximized = false;
        if let Some(([l, t, r, b], max)) = config.window {
            if r - l >= 200 && b - t >= 150 {
                let wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, showCmd: SW_HIDE.0 as u32, rcNormalPosition: RECT { left: l, top: t, right: r, bottom: b }, ..Default::default() };
                let _ = SetWindowPlacement(hwnd, &wp);
                maximized = max;
            }
        }

        let mut app = App::new(config);
        app.attach(hwnd);
        // "Open with": the book's path is the first argument.
        if let Some(path) = std::env::args_os().nth(1).map(std::path::PathBuf::from).filter(|p| p.is_file()) {
            app.open_file(path);
        }
        let mut gfx = Gfx::new(dpi as f32)?;
        set_window_icons(hwnd, &mut gfx, dpi);
        let edit_font = create_edit_font(dpi as f32, EDIT_FONT_DIP, gfx.ui_family());
        let host = Host {
            hwnd,
            win_dpi: dpi as f32,
            gfx,
            input: Input::default(),
            mem: Memory::new(edit_font),
            app,
            hand_cursor: false,
            edit_brush: CreateSolidBrush(COLORREF(0)),
            edit_brush_color: u32::MAX,
            title_bar: None,
            a11y: uia::Shared::new(hwnd),
            focus_said: String::new(),
        };
        A11Y.with(|a| *a.borrow_mut() = Some(host.a11y.clone()));
        HOST.with(|h| *h.borrow_mut() = Some(host));
        HOST.with(|h| {
            if let Some(host) = h.borrow_mut().as_mut() {
                host.read_system_look();
                host.sync_edit_colors();
            }
        });

        let _ = ShowWindow(hwnd, if maximized { SW_SHOWMAXIMIZED } else { SW_SHOWDEFAULT });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        HOST.with(|h| h.borrow_mut().take());
        A11Y.with(|a| a.borrow_mut().take());
    }
    Ok(())
}
