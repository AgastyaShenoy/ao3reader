# AO3 Desktop Reader 📚

[![Release](https://img.shields.io/github/v/release/AgastyaShenoy/ao3reader?color=red&label=version)](https://github.com/AgastyaShenoy/ao3reader/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-100%25-orange.svg)](https://www.rust-lang.org/)
[![GUI: egui](https://img.shields.io/badge/GUI-eframe%20%2F%20egui-blueviolet.svg)](https://github.com/emilk/egui)

A fast, lightweight, native desktop ebook reader for **Archive of Our Own (AO3)** built in 100% Rust with GPU-accelerated immediate-mode graphics (`eframe` / `egui`).

Designed for pure performance, privacy, and long, comfortable reading sessions:
- **Zero Webview / Zero Chromium bloat**: Standalone native Windows binary (~19.5 MB binary, ~7.7 MB zip distribution).
- **GPU-Accelerated**: Silky smooth 60/120+ FPS scrolling powered natively by DirectX 12.
- **Low Memory Overhead**: Utilizes the high-efficiency `mimalloc` allocator and bounded thread pools.
- **100% Offline Capability**: Download and cache your favorite works locally as structured JSON for distraction-free offline reading.
- **Real E-Reader Page Flipping**: Read like an e-book or paper novel with automatic single and dual-page spreads.
- **Universal Font Support**: Read using any font installed on your PC, search installed fonts, or load any `.ttf` / `.otf` file.
- **Built-in Auto-Update**: Automated GitHub Releases checker with release note previews and one-click downloads.

---

## 🤖 Attribution & Origin

> **💡 Ideated, prompted, and guided by:** [Agastya Shenoy](https://github.com/AgastyaShenoy)  
> **🤖 Developed majorly by AI:** Google **Gemini 3.8 Flash** via Antigravity  

This project was conceived as an exploration into building high-performance, low-level native desktop readers using modern AI pair-programming.

---

## 🌟 What's New in v1.1.0 🚀

- 📖 **Real E-Reader Page Flipping**: Experience distraction-free e-book reading with page spreads! Supports single-page and automatic dual-page layouts on wide displays, edge turn indicators (`‹ / ›`), and mouse scroll wheel page flipping.
- 🔤 **Universal Font Support**: Select and read in any font installed on your system! Includes real-time font search, curated book-optimized typefaces, and custom `.ttf` / `.otf` font file loading with safe proportional fallback.
- 📱 **Responsive Adaptive Work Cards**: Work cards in the library now dynamically adjust on window resize, positioning buttons on the right on wide displays and stacking cleanly on the bottom on narrow screens without icon overlap or text clipping.
- ⚙ **Organized Tabbed Settings Hub**: Brand-new categorized Preferences interface with tabs for Reading & Ergonomics, Typography & Fonts, Theme Palettes, Keyboard Shortcuts, Storage & Updates, and About.
- 📏 **Enhanced Text Wrapping & Metrics**: Multiline summary previews with safe character boundary truncation and perfectly aligned typography sliders.

---

## 🌟 Key Features

### 📖 Comprehensive E-Reader Experience
- **Two Reading Modes**:
  - **📖 E-Reader Mode (Paged / Flip)**: Reads like an authentic Kindle or paper novel. Flip pages using `Arrow Keys`, `PageUp / PageDown`, `Space / Shift+Space`, `J / K`, or the mouse wheel.
  - **📜 Continuous Scroll**: Classic web reading with smooth vertical scrolling.
- **Adaptive Page Spreads**:
  - **Auto**: Automatically switches between single-page on compact screens and side-by-side dual-page book spreads on wider windows ($\ge 1100\text{px}$).
  - **Single Page**: Dedicated clean single-column view.
  - **Dual Page**: Classic open book two-page spread.
- **Universal Custom Font Support**:
  - Choose from standard presets (**Modern Sans**, **Code Monospace**), curated book fonts (**Georgia**, **Palatino**, **Baskerville**, **Garamond**, **Calibri**, etc.), or search across all installed system fonts.
  - Load any downloaded `.ttf` or `.otf` font file directly from your computer.
  - Font customization applies strictly to the reading canvas, leaving app navigation and UI clean and proportional.
- **Distraction-Free Canvas & Zen Mode**:
  - Center-aligned reading column (adjustable width from 450px to 1400px).
  - Press `F11` for Zen Mode (fullscreen borderless immersion).
- **Eye-Care Color Themes**:
  - **Dark Slate** (default modern dark mode)
  - **OLED Black** (pitch-black background, high contrast for OLED displays)
  - **Paper Light** (clean crisp white reading page)
  - **Warm Sepia** (parchment tint for daytime eye-strain reduction)
  - **Nord Arctic** (cool arctic blue palette)
  - **Solarized Light** (calm low-contrast daylight palette)
- **Typography & Ergonomics Controls**:
  - Granular font sizing slider (12 pt to 36 pt) with quick hotkey toggles (`A+` / `A-`).
  - Adjustable line height multiplier (1.1x to 2.6x).
  - Paragraph spacing adjustments and optional first-line book indentation (4 spaces).
  - Collapsible author preface notes and post-chapter end notes.
- **Chapter Navigation & Table of Contents**:
  - Slide-out Table of Contents drawer with word counts per chapter and instant jumping.
  - Previous / Next Chapter buttons with keyboard shortcuts (`Ctrl + Left / Right Arrow`).
  - Auto-scroll to chapter top and persistent position tracking.
- **Reading Metrics & Progress**:
  - Real-time progress bar showing Chapter X of Y and reading percentage.
  - Estimated reading time remaining per chapter and total work (calculated using customizable Words-Per-Minute reading speed).
- **In-Work Search (`Ctrl + F`)**:
  - Search current chapter or entire multi-chapter works with match count and jump navigation (`◀` / `▶`).
- **Bookmarks & Annotations**:
  - Add bookmarks at any chapter with custom personal notes.

### 🌐 Archive of Our Own (AO3) Integration
- **Guest or Logged In**:
  - Read freely as a **Guest** without an AO3 account for all public works.
  - Log in with your **AO3 Username & Password** or paste your **Session Cookie** (`_otw_remember_me`) to access restricted works (locked works with the padlock icon).
  - Credentials and session cookies are stored strictly in your local OS AppData directory and are sent only directly to `archiveofourown.org`.
- **Automatic Adult Confirmation**: Bypasses the AO3 adult content splash screen automatically (`view_adult=true`).
- **Rich Work Metadata & Badges**:
  - Official color-coded rating badges (General = Green, Teen = Gold, Mature = Orange, Explicit = Red, Not Rated = Gray).
  - Category orientation badges (M/M, F/M, F/F, Gen, Multi).
  - Warning badges (violence, character death, or green for "No Archive Warnings Apply").
  - Fandom chips, relationship / ship pills (`♥`), character tags, and freeform tags.
  - Formatted author summaries and blockquotes.
- **Leave Kudos**: Send kudos to the author directly from inside the app with a single click.
- **Download EPUB**: Direct one-click EPUB download from AO3.

### 🔄 In-App Auto-Update Checker
- **GitHub Release Integration**: Directly checks the latest releases from the official repository (`AgastyaShenoy/ao3reader`).
- **Automatic Startup Check**: Configurable in Settings to run silently in the background when the app launches.
- **Update Notification Badge**: Displays a visual indicator when a newer version is available.
- **In-App Changelog Preview**: View release titles, publication dates, and release notes inside the application.
- **One-Click Downloads**: Direct links to open release notes or download the updated binary package.

### 📚 Local Library Management
- **Offline Caching**: Works and chapters are saved as structured JSON on your disk so you can read anytime without internet access.
- **Shelf Organization**:
  - *Currently Reading*
  - *Completed*
  - *Marked for Later*
  - *Favorites*
- **Library Search & Filtering**: Real-time search across titles, authors, fandoms, pairings, and tags.
- **Multi-Format Export**:
  - **Standalone HTML (.html)**: Self-contained HTML file with embedded responsive CSS styling.
  - **Clean Markdown (.md)**: Formatted markdown document with headers, notes, and blockquotes.
  - **Plain Text (.txt)**: Clean reader text format.

---

## ⌨ Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `[→]`, `[PageDown]`, `[Space]`, `[K]` | Turn to Next Page / Spread (in Paged mode) |
| `[←]`, `[PageUp]`, `[Shift+Space]`, `[J]` | Turn to Previous Page / Spread (in Paged mode) |
| `[Ctrl + →]` / `[Ctrl + ←]` | Skip to Next / Previous Chapter |
| `[Home]` / `[End]` | First Page / Last Page of Chapter |
| `[F11]` | Toggle Zen Mode (Border-free fullscreen immersion) |
| `[Ctrl + F]` | Open / Close Search in Work |
| `[Ctrl + B]` | Bookmark current chapter |
| `[A+]` / `[A-]` | Quick increase / decrease font size |
| `[Esc]` | Close search bar, Table of Contents, or appearance drawers |

---

## 🏗 Architecture & Technologies

- **Language**: 100% Rust (Edition 2024).
- **GUI Engine**: [`eframe`](https://github.com/emilk/egui/tree/master/crates/eframe) & [`egui`](https://github.com/emilk/egui) (immediate-mode GPU accelerated via native DirectX 12).
- **Memory Allocator**: [`mimalloc`](https://github.com/microsoft/mimalloc) for minimal heap fragmentation and rapid page reclamation.
- **Networking & Sessions**: [`reqwest`](https://github.com/seanmonstar/reqwest) with cookie jar persistence for AO3 authentication and adult content cookies.
- **HTML Parsing**: [`scraper`](https://github.com/causal-agent/scraper) for extracting metadata, chapters, paragraphs, and styling.
- **Font Parsing**: [`ttf-parser`](https://github.com/RazrFalcon/ttf-parser) for zero-panic font validation and system font scanning.
- **Async Runtime**: [`tokio`](https://tokio.rs/) multi-threaded runtime with bounded workers so network operations never block UI rendering.
- **Data Persistence**: [`serde`](https://serde.rs/) + [`directories`](https://github.com/dirs-dev/directories-rs) storing local configuration and library cache in the standard OS AppData folder.

---

## 🚀 Installation & Running

### Download Binary (Windows)
Pre-compiled standalone binaries are available on the [Releases](https://github.com/AgastyaShenoy/ao3reader/releases) page:
1. Download `ao3reader-v1.1.0-windows-x64.zip` or `ao3reader.exe`.
2. Extract and run `ao3reader.exe` directly (no installer required).

### Building from Source

#### Prerequisites
- [Rust](https://www.rust-lang.org/) 1.80+ (toolchain installed).

```bash
# Clone the repository
git clone https://github.com/AgastyaShenoy/ao3reader.git
cd ao3reader

# Run in development mode
cargo run

# Build optimized release binary
cargo build --release
```

The optimized single binary will be generated at `target/release/ao3reader.exe`.

---

## 📄 License

This project is licensed under the [MIT License](LICENSE) - Copyright (c) 2026 Agastya Shenoy.
