//! AO3 Reader — Native Win32 GDI Software Rendering Prototype (Sumatra PDF Style)
//!
//! This example demonstrates pure CPU software rendering without 3D GPU drivers (no DirectX 12,
//! no Vulkan, no OpenGL, and no wgpu texture pipelines).
//!
//! Sumatra PDF and similar ultra-light readers achieve their famous 5-15 MB RAM footprint by:
//! 1. Using native Win32 GDI / DirectWrite blitting rather than 3D graphics APIs.
//! 2. Zero GPU driver DLLs mapped into process address space (nvwgf2umx.dll, amdvlk64.dll, etc.).
//! 3. Zero multi-megabyte GPU swapchain textures or descriptor heap allocations.
//! 4. Zero GPU font atlas generation in VRAM.

#![cfg(windows)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unused_imports)]

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

// Memory allocator: use mimalloc for clean minimal overhead
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn to_wstring(str: &str) -> Vec<u16> {
    OsStr::new(str).encode_wide().chain(Some(0)).collect()
}

// Convert RGB to Win32 COLORREF (0x00BBGGRR)
const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    pub bg: u32,
    pub bar_bg: u32,
    pub bar_border: u32,
    pub text: u32,
    pub text_muted: u32,
    pub accent: u32,
    pub card_bg: u32,
}

const THEMES: [Theme; 4] = [
    Theme {
        name: "Catppuccin Mocha (Dark)",
        bg: rgb(30, 30, 46),         // #1E1E2E
        bar_bg: rgb(24, 24, 37),     // #181825
        bar_border: rgb(49, 50, 68), // #313244
        text: rgb(205, 214, 244),    // #CDD6F4
        text_muted: rgb(166, 173, 200), // #A6ADC8
        accent: rgb(245, 194, 231),  // #F5C2E7
        card_bg: rgb(40, 40, 60),    // #28283C
    },
    Theme {
        name: "Warm Sepia (Paper)",
        bg: rgb(251, 240, 217),      // #FBF0D9
        bar_bg: rgb(237, 225, 197),  // #EDE1C5
        bar_border: rgb(215, 201, 170),
        text: rgb(46, 40, 42),       // #2E282A
        text_muted: rgb(110, 95, 80),
        accent: rgb(155, 44, 44),    // Muted Crimson
        card_bg: rgb(242, 230, 205),
    },
    Theme {
        name: "Clean Light",
        bg: rgb(255, 255, 255),
        bar_bg: rgb(242, 242, 245),
        bar_border: rgb(220, 220, 225),
        text: rgb(25, 25, 30),
        text_muted: rgb(100, 100, 110),
        accent: rgb(153, 0, 0),      // AO3 Red
        card_bg: rgb(248, 248, 250),
    },
    Theme {
        name: "OLED Pitch Black",
        bg: rgb(0, 0, 0),
        bar_bg: rgb(14, 14, 18),
        bar_border: rgb(35, 35, 45),
        text: rgb(225, 225, 230),
        text_muted: rgb(130, 130, 140),
        accent: rgb(255, 107, 107),
        card_bg: rgb(18, 18, 22),
    },
];

const FONTS: [&str; 5] = [
    "Georgia",
    "Segoe UI",
    "Calibri",
    "Cambria",
    "Consolas",
];

struct FicMetadata {
    title: &'static str,
    author: &'static str,
    fandom: &'static str,
    rating: &'static str,
    words: &'static str,
    chapters: &'static str,
    summary: &'static str,
    notes: &'static str,
    content: &'static str,
}

static SAMPLE_FIC: FicMetadata = FicMetadata {
    title: "Across the Starlit Library",
    author: "Archive Archivist",
    fandom: "Original Fiction / Digital Preservation",
    rating: "General Audiences",
    words: "1,240 words",
    chapters: "1 / 1",
    summary: "A study in ultra-lightweight software rasterization: how Sumatra PDF renders hundreds of pages in under 10 megabytes of memory, while modern hardware-accelerated GUI frameworks consume 120-200 MB just displaying a blank frame.",
    notes: "Prototype testing Path A: Native Windows GDI double-buffered rendering. Zero DirectX. Zero Vulkan. Zero wgpu. Direct to screen.",
    content: r#"Chapter 1: The Weight of an Abstraction

The morning sun spilled across the wooden desk in thin amber ribbons, illuminating dusty stacks of paperbacks that had weathered half a century without once requiring a graphics card driver.

On the screen sat two windows side-by-side.

The first was Sumatra PDF. It held open an eight-hundred-page technical manual filled with vector diagrams, mathematical formulas, and dense typography. Its process footprint in the Windows Task Manager read steady: 8.7 Megabytes. Not gigabytes. Not hundreds of megabytes. Eight point seven.

Beside it sat a modern desktop application built on an immediate-mode GPU engine. It was displaying a greeting screen and three buttons. Its working set: 142.4 Megabytes.

"Where does the weight come from?" asked the student, peering over the terminal.

The engineer tapped the keyboard, pulling up the loaded module list with VMMap.

"Look here," said the engineer, highlighting the loaded system libraries. "To put a single pixel on the screen through a modern 3D graphics pipeline, the operating system must load d3d12.dll, dxgi.dll, and the proprietary hardware driver: nvwgf2umx.dll or amdvlk64.dll. The graphics vendor's driver alone maps eighty to one hundred megabytes of executable code, shader compilers, internal telemetry hooks, and ring-buffer scratchpads directly into your process address space before your application has rendered its very first letter."

"And the font glyphs?"

"In a GPU engine, every character must be rasterized into a high-DPI texture atlas in video RAM. Then, every frame, hundreds of textured quads are sent through vertex shaders, rasterizers, and presentation swapchains. It is magnificent for complex games, dynamic 3D scenes, and 120Hz smooth interface transitions. But for reading a book? For reading quiet words on a quiet screen?"

The engineer turned to the GDI prototype window.

"Here, we speak directly to Windows. The operating system's font engine renders glyphs with sub-pixel ClearType directly to a system bitmap. The bitmap is blitted to the window device context in microseconds. Zero shaders. Zero driver bloat. Zero multi-megabyte framebuffers. When you stop touching the keyboard, the CPU usage drops to 0.00% and stays there."

The student adjusted the font size using the plus key. The letters expanded smoothly, their crisp black serifs aligning with mathematical precision against the warm sepia canvas.

"It feels... instant," the student whispered. "Like turning a page in a physical book."

"Because there are no layers between your finger and the glass," said the engineer. "Just ink, light, and memory."

---

Chapter 2: The Architecture of Pure Text

To understand why software rendering is so enduringly powerful for readers, one must examine what reading actually requires. 

A reader does not need sixty frames per second of volumetric particle effects. A reader needs:
1. Flawless typographic clarity (ClearType anti-aliasing tuned to the physical sub-pixel layout of the monitor).
2. Predictable, uninterrupted battery life on portable laptops.
3. Instantaneous startup time—opening in milliseconds rather than seconds.
4. Minimal memory overhead, allowing fifty books or fanfics to sit open simultaneously in the background without pressurizing the operating system's virtual memory subsystem.

When you flip between pages in this prototype, notice how the Working Set memory stays pinned between 6 and 10 Megabytes. Open Task Manager and verify it for yourself. 

Every keystroke is handled natively by the Windows message pump. The memory counter in the title bar above updates in real time using the Windows NT kernel process counters (GetProcessMemoryInfo).

As we continue developing AO3 Reader, this proof of concept demonstrates what is technically achievable when we choose the exact right tool for the job.

Enjoy the reading experience. Press [T] to toggle themes, [F] to switch fonts, and [+/-] to resize typography.
"#,
};

struct AppState {
    theme_idx: usize,
    font_idx: usize,
    font_size_pt: i32,
    scroll_y: i32,
    max_scroll: i32,
    client_width: i32,
    client_height: i32,
}

static STATE: Mutex<AppState> = Mutex::new(AppState {
    theme_idx: 0,
    font_idx: 0,
    font_size_pt: 19,
    scroll_y: 0,
    max_scroll: 0,
    client_width: 1000,
    client_height: 750,
});

fn get_memory_stats() -> (f64, f64) {
    unsafe {
        let process = GetCurrentProcess();
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

        if GetProcessMemoryInfo(
            process,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ) != 0
        {
            let working_set_mb = counters.WorkingSetSize as f64 / (1024.0 * 1024.0);
            let private_mb = counters.PagefileUsage as f64 / (1024.0 * 1024.0);
            (working_set_mb, private_mb)
        } else {
            (0.0, 0.0)
        }
    }
}

fn update_window_title(hwnd: HWND) {
    let (ws_mb, priv_mb) = get_memory_stats();
    let state = STATE.lock().unwrap();
    let theme_name = THEMES[state.theme_idx].name;
    let font_name = FONTS[state.font_idx];
    let pt = state.font_size_pt;

    let title = format!(
        "AO3 Reader [Win32 GDI Software Renderer] | RAM: {:.2} MB (Working Set) / {:.2} MB (Private) | Theme: {} | Font: {} ({}pt)",
        ws_mb, priv_mb, theme_name, font_name, pt
    );
    let title_w = to_wstring(&title);
    unsafe {
        SetWindowTextW(hwnd, title_w.as_ptr());
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            // Set 1-second timer to refresh live RAM stats in title bar
            SetTimer(hwnd, 1, 1000, None);
            update_window_title(hwnd);
            0
        }
        WM_TIMER => {
            if wparam == 1 {
                update_window_title(hwnd);
            }
            0
        }
        WM_SIZE => {
            let width = (lparam & 0xFFFF) as i32;
            let height = ((lparam >> 16) & 0xFFFF) as i32;
            {
                let mut state = STATE.lock().unwrap();
                state.client_width = width;
                state.client_height = height;
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_ERASEBKGND => {
            // Handled in double buffer on WM_PAINT to eliminate flicker
            1
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as i16) as i32;
            let lines = -(delta / 120) * 60; // 60px per notch
            {
                let mut state = STATE.lock().unwrap();
                state.scroll_y = (state.scroll_y + lines).clamp(0, state.max_scroll);
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_KEYDOWN => {
            let mut state = STATE.lock().unwrap();
            let mut dirty = true;
            match wparam as u16 {
                VK_UP => {
                    state.scroll_y = (state.scroll_y - 50).clamp(0, state.max_scroll);
                }
                VK_DOWN => {
                    state.scroll_y = (state.scroll_y + 50).clamp(0, state.max_scroll);
                }
                VK_PRIOR => {
                    // Page Up
                    let page_step = (state.client_height - 140).max(100);
                    state.scroll_y = (state.scroll_y - page_step).clamp(0, state.max_scroll);
                }
                VK_NEXT | VK_SPACE => {
                    // Page Down or Space
                    let page_step = (state.client_height - 140).max(100);
                    state.scroll_y = (state.scroll_y + page_step).clamp(0, state.max_scroll);
                }
                VK_HOME => {
                    state.scroll_y = 0;
                }
                VK_END => {
                    state.scroll_y = state.max_scroll;
                }
                0x54 => {
                    // 'T' key: Toggle theme
                    state.theme_idx = (state.theme_idx + 1) % THEMES.len();
                }
                0x46 => {
                    // 'F' key: Toggle font
                    state.font_idx = (state.font_idx + 1) % FONTS.len();
                }
                VK_ADD | VK_OEM_PLUS => {
                    // '+' key: Increase font size
                    if state.font_size_pt < 36 {
                        state.font_size_pt += 1;
                    }
                }
                VK_SUBTRACT | VK_OEM_MINUS => {
                    // '-' key: Decrease font size
                    if state.font_size_pt > 12 {
                        state.font_size_pt -= 1;
                    }
                }
                VK_ESCAPE => {
                    PostQuitMessage(0);
                    return 0;
                }
                _ => {
                    dirty = false;
                }
            }

            drop(state);
            if dirty {
                update_window_title(hwnd);
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            let (width, height, theme, font_name, font_pt, scroll_y) = {
                let s = STATE.lock().unwrap();
                (
                    s.client_width,
                    s.client_height,
                    THEMES[s.theme_idx],
                    FONTS[s.font_idx],
                    s.font_size_pt,
                    s.scroll_y,
                )
            };

            if width > 0 && height > 0 {
                // Double-buffered rendering for 100% flicker-free blitting
                let mem_dc = CreateCompatibleDC(hdc);
                let mem_bm = CreateCompatibleBitmap(hdc, width, height);
                let old_bm = SelectObject(mem_dc, mem_bm);

                // 1. Fill entire window background
                let bg_brush = CreateSolidBrush(theme.bg);
                let win_rect = RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                };
                FillRect(mem_dc, &win_rect, bg_brush);
                DeleteObject(bg_brush);

                SetBkMode(mem_dc, TRANSPARENT as i32);

                // DPI scaling factor (assume standard 96 DPI for GDI point conversion)
                let font_height_px = -((font_pt * 96) / 72);

                // Create typography fonts
                let font_w = to_wstring(font_name);
                let body_font = CreateFontW(
                    font_height_px,
                    0,
                    0,
                    0,
                    FW_NORMAL as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    (VARIABLE_PITCH | FF_ROMAN) as u32,
                    font_w.as_ptr(),
                );

                let title_font = CreateFontW(
                    font_height_px - 8,
                    0,
                    0,
                    0,
                    FW_BOLD as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    (VARIABLE_PITCH | FF_ROMAN) as u32,
                    font_w.as_ptr(),
                );

                let segoe_w = to_wstring("Segoe UI");
                let ui_font = CreateFontW(
                    -16,
                    0,
                    0,
                    0,
                    FW_NORMAL as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    (VARIABLE_PITCH | FF_SWISS) as u32,
                    segoe_w.as_ptr(),
                );

                let ui_bold_font = CreateFontW(
                    -16,
                    0,
                    0,
                    0,
                    FW_BOLD as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    CLEARTYPE_QUALITY as u32,
                    (VARIABLE_PITCH | FF_SWISS) as u32,
                    segoe_w.as_ptr(),
                );

                let header_h = 50;
                let footer_h = 36;
                let reading_top = header_h + 20;

                // Max reading line length (classic optimal reading column ~740px)
                let column_width = (width - 120).clamp(400, 780);
                let col_left = (width - column_width) / 2;
                let col_right = col_left + column_width;

                // --- 2. Render Book Content Area with Clipping ---
                let mut current_y = reading_top - scroll_y;

                // Fic Header Card (Tags, Author, Summary)
                let card_h = 170;
                if current_y + card_h > header_h && current_y < height - footer_h {
                    let card_rect = RECT {
                        left: col_left,
                        top: current_y,
                        right: col_right,
                        bottom: current_y + card_h,
                    };
                    let card_brush = CreateSolidBrush(theme.card_bg);
                    FillRect(mem_dc, &card_rect, card_brush);
                    DeleteObject(card_brush);

                    let card_border_brush = CreateSolidBrush(theme.bar_border);
                    FrameRect(mem_dc, &card_rect, card_border_brush);
                    DeleteObject(card_border_brush);

                    // Card contents
                    SelectObject(mem_dc, ui_bold_font);
                    SetTextColor(mem_dc, theme.accent);
                    let mut tag_rect = RECT {
                        left: col_left + 16,
                        top: current_y + 12,
                        right: col_right - 16,
                        bottom: current_y + 34,
                    };
                    let fandom_text = to_wstring(&format!(
                        "{}  •  {}  •  {}  •  {}",
                        SAMPLE_FIC.fandom, SAMPLE_FIC.rating, SAMPLE_FIC.words, SAMPLE_FIC.chapters
                    ));
                    DrawTextW(
                        mem_dc,
                        fandom_text.as_ptr(),
                        fandom_text.len() as i32 - 1,
                        &mut tag_rect,
                        DT_LEFT | DT_TOP | DT_NOPREFIX,
                    );

                    // Summary
                    SelectObject(mem_dc, ui_font);
                    SetTextColor(mem_dc, theme.text_muted);
                    let mut sum_rect = RECT {
                        left: col_left + 16,
                        top: current_y + 40,
                        right: col_right - 16,
                        bottom: current_y + 105,
                    };
                    let sum_text = to_wstring(&format!("Summary: {}", SAMPLE_FIC.summary));
                    DrawTextW(
                        mem_dc,
                        sum_text.as_ptr(),
                        sum_text.len() as i32 - 1,
                        &mut sum_rect,
                        DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX,
                    );

                    // Notes
                    let mut notes_rect = RECT {
                        left: col_left + 16,
                        top: current_y + 112,
                        right: col_right - 16,
                        bottom: current_y + 160,
                    };
                    let notes_text = to_wstring(&format!("Notes: {}", SAMPLE_FIC.notes));
                    DrawTextW(
                        mem_dc,
                        notes_text.as_ptr(),
                        notes_text.len() as i32 - 1,
                        &mut notes_rect,
                        DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX,
                    );
                }
                current_y += card_h + 35;

                // Fic Main Title
                SelectObject(mem_dc, title_font);
                SetTextColor(mem_dc, theme.text);
                let title_w_str = to_wstring(SAMPLE_FIC.title);
                let mut title_rect = RECT {
                    left: col_left,
                    top: current_y,
                    right: col_right,
                    bottom: current_y + 50,
                };
                let drawn_title_h = DrawTextW(
                    mem_dc,
                    title_w_str.as_ptr(),
                    title_w_str.len() as i32 - 1,
                    &mut title_rect,
                    DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX,
                );
                current_y += drawn_title_h + 8;

                // Byline
                SelectObject(mem_dc, ui_font);
                SetTextColor(mem_dc, theme.text_muted);
                let byline = to_wstring(&format!("by {}", SAMPLE_FIC.author));
                let mut byline_rect = RECT {
                    left: col_left,
                    top: current_y,
                    right: col_right,
                    bottom: current_y + 30,
                };
                DrawTextW(
                    mem_dc,
                    byline.as_ptr(),
                    byline.len() as i32 - 1,
                    &mut byline_rect,
                    DT_LEFT | DT_TOP | DT_NOPREFIX,
                );
                current_y += 45;

                // Main Reading Body Text
                SelectObject(mem_dc, body_font);
                SetTextColor(mem_dc, theme.text);

                let content_w = to_wstring(SAMPLE_FIC.content);

                // Calculate total height of the body text
                let mut calc_rect = RECT {
                    left: col_left,
                    top: 0,
                    right: col_right,
                    bottom: 0,
                };
                let total_body_h = DrawTextW(
                    mem_dc,
                    content_w.as_ptr(),
                    content_w.len() as i32 - 1,
                    &mut calc_rect,
                    DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT,
                );

                // Draw actual body text
                let mut body_rect = RECT {
                    left: col_left,
                    top: current_y,
                    right: col_right,
                    bottom: current_y + total_body_h,
                };
                DrawTextW(
                    mem_dc,
                    content_w.as_ptr(),
                    content_w.len() as i32 - 1,
                    &mut body_rect,
                    DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX,
                );

                let content_total_bottom = current_y + total_body_h + 80;
                let computed_max_scroll = (content_total_bottom + scroll_y - (height - footer_h)).max(0);
                {
                    let mut s = STATE.lock().unwrap();
                    s.max_scroll = computed_max_scroll;
                }

                // --- 3. Fixed Top Header Navigation Bar ---
                let top_bar_brush = CreateSolidBrush(theme.bar_bg);
                let top_bar_rect = RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: header_h,
                };
                FillRect(mem_dc, &top_bar_rect, top_bar_brush);
                DeleteObject(top_bar_brush);

                // Header bottom border line
                let border_pen = CreatePen(PS_SOLID as i32, 1, theme.bar_border);
                let old_pen = SelectObject(mem_dc, border_pen);
                MoveToEx(mem_dc, 0, header_h, std::ptr::null_mut());
                LineTo(mem_dc, width, header_h);

                // Header title & logo
                SelectObject(mem_dc, ui_bold_font);
                SetTextColor(mem_dc, theme.accent);
                let logo_text = to_wstring("AO3 READER");
                let mut logo_rect = RECT {
                    left: 20,
                    top: 14,
                    right: 180,
                    bottom: header_h,
                };
                DrawTextW(
                    mem_dc,
                    logo_text.as_ptr(),
                    logo_text.len() as i32 - 1,
                    &mut logo_rect,
                    DT_LEFT | DT_TOP | DT_NOPREFIX,
                );

                SelectObject(mem_dc, ui_font);
                SetTextColor(mem_dc, theme.text);
                let header_title = to_wstring(&format!(
                    "{} — Chapter 1",
                    SAMPLE_FIC.title
                ));
                let mut htitle_rect = RECT {
                    left: 170,
                    top: 14,
                    right: width - 360,
                    bottom: header_h,
                };
                DrawTextW(
                    mem_dc,
                    header_title.as_ptr(),
                    header_title.len() as i32 - 1,
                    &mut htitle_rect,
                    DT_LEFT | DT_TOP | DT_NOPREFIX,
                );

                // Header Memory Pill Badge (Right aligned)
                let (ws_mb, _) = get_memory_stats();
                let mem_badge_text = to_wstring(&format!(
                    "⚡ RAM: {:.1} MB  (vs GPU: ~140 MB)",
                    ws_mb
                ));
                SetTextColor(mem_dc, rgb(166, 227, 161)); // Pastel Green
                let mut badge_rect = RECT {
                    left: width - 340,
                    top: 14,
                    right: width - 20,
                    bottom: header_h,
                };
                DrawTextW(
                    mem_dc,
                    mem_badge_text.as_ptr(),
                    mem_badge_text.len() as i32 - 1,
                    &mut badge_rect,
                    DT_RIGHT | DT_TOP | DT_NOPREFIX,
                );

                // --- 4. Fixed Bottom Footer Status Bar ---
                let bot_bar_brush = CreateSolidBrush(theme.bar_bg);
                let bot_bar_rect = RECT {
                    left: 0,
                    top: height - footer_h,
                    right: width,
                    bottom: height,
                };
                FillRect(mem_dc, &bot_bar_rect, bot_bar_brush);
                DeleteObject(bot_bar_brush);

                // Footer top border line
                MoveToEx(mem_dc, 0, height - footer_h, std::ptr::null_mut());
                LineTo(mem_dc, width, height - footer_h);
                SelectObject(mem_dc, old_pen);
                DeleteObject(border_pen);

                // Footer controls guide
                SelectObject(mem_dc, ui_font);
                SetTextColor(mem_dc, theme.text_muted);
                let shortcuts = to_wstring(
                    "[T] Theme  •  [F] Font  •  [+/-] Font Size  •  [Up/Down/Wheel/Space] Scroll  •  [Esc] Exit"
                );
                let mut sc_rect = RECT {
                    left: 20,
                    top: height - footer_h + 8,
                    right: width - 200,
                    bottom: height,
                };
                DrawTextW(
                    mem_dc,
                    shortcuts.as_ptr(),
                    shortcuts.len() as i32 - 1,
                    &mut sc_rect,
                    DT_LEFT | DT_TOP | DT_NOPREFIX,
                );

                // Progress percentage (Right aligned)
                let progress_pct = if computed_max_scroll > 0 {
                    ((scroll_y as f64 / computed_max_scroll as f64) * 100.0).clamp(0.0, 100.0)
                } else {
                    100.0
                };
                let progress_text = to_wstring(&format!("{:.0}% Finished", progress_pct));
                SetTextColor(mem_dc, theme.text);
                let mut pr_rect = RECT {
                    left: width - 180,
                    top: height - footer_h + 8,
                    right: width - 20,
                    bottom: height,
                };
                DrawTextW(
                    mem_dc,
                    progress_text.as_ptr(),
                    progress_text.len() as i32 - 1,
                    &mut pr_rect,
                    DT_RIGHT | DT_TOP | DT_NOPREFIX,
                );

                // Blit memory buffer to the screen window DC in a single atomic hardware-less operation
                BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

                // Clean up GDI handles
                SelectObject(mem_dc, old_bm);
                DeleteObject(mem_bm);
                DeleteDC(mem_dc);

                DeleteObject(body_font);
                DeleteObject(title_font);
                DeleteObject(ui_font);
                DeleteObject(ui_bold_font);
            }

            EndPaint(hwnd, &ps);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    let class_name = to_wstring("AO3ReaderWin32Renderer");
    let window_title = to_wstring("AO3 Reader — Win32 GDI Software Renderer");

    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(), // Handled in WM_ERASEBKGND / double-buffer
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };

        if RegisterClassExW(&wnd_class) == 0 {
            eprintln!("Failed to register Win32 window class");
            return;
        }

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1060,
            820,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );

        if hwnd.is_null() {
            eprintln!("Failed to create Win32 window");
            return;
        }

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        // Standard Windows message pump
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

