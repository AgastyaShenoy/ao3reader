//! Native single-line EDIT controls hosted as child windows.
//!
//! Using the real Windows control gives IME input, clipboard, undo, selection, password masking
//! and screen-reader support for free. The surrounding box is drawn by the widget kit; the
//! control itself only paints its text on the theme's input colour (see WM_CTLCOLOREDIT).

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::EM_SETCUEBANNER;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Posted to the parent when Enter or Escape is pressed in an edit: wParam = control id, lParam = VK.
pub const WM_APP_EDIT_KEY: u32 = WM_APP + 2;

pub struct EditBox {
    pub hwnd: HWND,
    pub ctrl_id: i32,
    /// Text as last synchronised with the owning `String`.
    pub text: String,
    rect: (i32, i32, i32, i32),
    visible: bool,
    pub seen: bool,
    pub user_changed: bool,
    pub submitted: bool,
    pub escaped: bool,
}

impl EditBox {
    pub fn create(parent: HWND, ctrl_id: i32, password: bool, placeholder: &str, font: HFONT) -> Option<Self> {
        unsafe {
            let mut style = WS_CHILD | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32 | ES_LEFT as u32);
            if password {
                style |= WINDOW_STYLE(ES_PASSWORD as u32);
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("EDIT"),
                w!(""),
                style,
                0,
                0,
                10,
                10,
                Some(parent),
                Some(HMENU(ctrl_id as usize as *mut _)),
                None,
                None,
            )
            .ok()?;
            SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(0)));
            if !placeholder.is_empty() {
                let cue = HSTRING::from(placeholder);
                SendMessageW(hwnd, EM_SETCUEBANNER, Some(WPARAM(0)), Some(LPARAM(cue.as_ptr() as isize)));
            }
            let _ = SetWindowSubclass(hwnd, Some(edit_subclass), 1, parent.0 as usize);
            Some(Self {
                hwnd,
                ctrl_id,
                text: String::new(),
                rect: (0, 0, 0, 0),
                visible: false,
                seen: true,
                user_changed: false,
                submitted: false,
                escaped: false,
            })
        }
    }

    pub fn place(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if self.rect != (x, y, w, h) {
            self.rect = (x, y, w, h);
            unsafe {
                let _ = SetWindowPos(self.hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
        if !self.visible {
            self.visible = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNA);
            }
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            self.visible = false;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    pub fn read(&self) -> String {
        unsafe {
            let len = GetWindowTextLengthW(self.hwnd).max(0) as usize;
            let mut buf = vec![0u16; len + 1];
            let n = GetWindowTextW(self.hwnd, &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..n])
        }
    }

    pub fn write(&mut self, s: &str) {
        self.text = s.to_string();
        unsafe {
            let _ = SetWindowTextW(self.hwnd, &HSTRING::from(s));
        }
    }

    pub fn set_font(&self, font: HFONT) {
        unsafe {
            SendMessageW(self.hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
    }

    /// Pastes the clipboard into the control, as Ctrl+V would.
    pub fn paste(&self) {
        unsafe {
            SendMessageW(self.hwnd, WM_PASTE, None, None);
        }
    }

    pub fn focus(&self) {
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
        }
    }

    pub fn destroy(self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// Turns Enter/Escape into notifications for the parent and stops the default beep.
unsafe extern "system" fn edit_subclass(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, parent: usize) -> LRESULT {
    unsafe {
        match msg {
            WM_KEYDOWN if wp.0 as u16 == VK_RETURN.0 || wp.0 as u16 == VK_ESCAPE.0 => {
                let ctrl_id = GetDlgCtrlID(hwnd);
                let _ = PostMessageW(Some(HWND(parent as *mut _)), WM_APP_EDIT_KEY, WPARAM(ctrl_id as usize), LPARAM(wp.0 as isize));
                LRESULT(0)
            }
            WM_CHAR if wp.0 == '\r' as usize || wp.0 == 0x1b => LRESULT(0),
            _ => DefSubclassProc(hwnd, msg, wp, lp),
        }
    }
}

/// GDI font for edit controls, matching the UI text size at the given DPI.
pub fn create_edit_font(dpi: f32, size_dip: f32) -> HFONT {
    unsafe {
        let height = -((size_dip * dpi / 96.0).round() as i32);
        CreateFontW(
            height,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (VARIABLE_PITCH.0 | FF_SWISS.0) as u32,
            w!("Segoe UI"),
        )
    }
}
